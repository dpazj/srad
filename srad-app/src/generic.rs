use std::{collections::{HashMap}, future::Future, ops::DerefMut, pin::Pin, sync::{Arc, Mutex}, time::Duration};

use log::{debug, info, trace, warn};
use srad_client::{Client, EventLoop};
use srad_types::{utils::timestamp, MetricId};

use crate::{events::{DBirth, DData, DeviceEvent, NData, NodeEvent}, resequencer::{self, Resequencer}, AppClient, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier, SubscriptionConfig};

use tokio::{select, sync::{mpsc, oneshot}, time::{sleep}};

use futures::{stream::FuturesUnordered, StreamExt};

pub trait MetricStore {
    fn set_stale(&mut self); 
    fn update_from_birth(&mut self, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), StateUpdateError>; 
    fn update_from_data(&mut self, details: Vec<(MetricId, MetricDetails)>) -> Result<(), StateUpdateError>; 
}

pub enum StateUpdateError {
    InvalidValue,
    UnknownMetric,
}

#[derive(PartialEq, Eq)]
enum LifecycleState {
    Unbirthed, 
    Birthed, 
    Stale,
}

#[derive(Debug)]
enum ResequenceableEvent {
    NData(NData),
    DBirth(String, DBirth),
    DDeath(String),
    DData(String, DData)
}

struct DeviceInner {
    //state: State
    name: String,
    store: Mutex<Option<Box<dyn MetricStore + Send>>>,
}

impl DeviceInner {

    fn set_stale(&self) {
        let mut store = self.store.lock().unwrap();
        if let Some(x) = &mut *store {
            x.set_stale()
        }
    }

    fn handle_birth(&self, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), RebirthReason> {
        let mut store = self.store.lock().unwrap();
        if let Some(x) = &mut *store {
            if let Err(_) = x.update_from_birth(details) {
                return Err (RebirthReason::InvalidPayload)
            }
        }
        Ok (())
    }

    fn handle_death(&self) {
        self.set_stale();
    }

    fn handle_data(&self, details: DData) -> Result<(), RebirthReason> {
        let mut store = self.store.lock().unwrap();
        if let Some(x) = &mut *store {
            if let Err(e) = x.update_from_data(details.metrics_details) {
                return Err(match e {
                    StateUpdateError::InvalidValue => RebirthReason::InvalidPayload,
                    StateUpdateError::UnknownMetric => RebirthReason::UnknownMetric,
                });
            }
        }
        Ok (())
    }


}

#[derive(Clone)]
pub struct Device {
    inner: Arc<DeviceInner>
}

impl Device {

    fn new(name: String) -> Self {
        Self {
           inner: Arc::new(DeviceInner { name, store: Mutex::new(None) } )
        }
    }

    fn set_stale(&self) {
        self.inner.set_stale();
    }

    fn handle_birth(&self, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), RebirthReason> {
        self.inner.handle_birth(details)
    }

    fn handle_death(&self) {
        self.inner.handle_death()
    }

    fn handle_data(&self, details: DData) -> Result<(), RebirthReason> {
        self.inner.handle_data(details)
    }

    pub fn set_metric_store<T: MetricStore + Send + 'static>(&self, store: T) {
        *self.inner.store.lock().unwrap() = Some(Box::new(store))
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

}

pub struct Node {
    id: Arc<NodeIdentifier>,
    lifecycle_state: LifecycleState,
    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<String, Device>,
    store: Option<Box<dyn MetricStore + Send>>,
    bdseq: u8,
    birth_timestamp: u64,
    device_created_cb: Option<DeviceCreatedCallback>,
    reorder_timeout_cancel_token: Option<oneshot::Sender<()>>
}

impl Node {

    pub fn set_metric_store<T: MetricStore + Send + 'static>(&mut self, store: T) {
        self.store = Some(Box::new(store))
    }

    pub fn on_device_created<F>(&mut self, cb: F) 
        where 
            F: Fn(&Device) + Send + Sync + 'static,
    
    {
        self.device_created_cb = Some(Box::pin(cb));
    }

    fn new(id: Arc<NodeIdentifier>) -> Self {
        Self {
            id,
            resequencer: Resequencer::new(),
            devices: HashMap::new(),
            lifecycle_state: LifecycleState::Unbirthed,
            bdseq: 0,
            birth_timestamp: 0,
            store: None, 
            device_created_cb: None,
            reorder_timeout_cancel_token: None
        }
    }

    fn set_stale(&mut self, timestamp: u64) 
    {
        if timestamp < self.birth_timestamp { return }
        debug!("Setting Node = {:?} Stale", self.id);
        self.resequencer.reset();
        self.lifecycle_state = LifecycleState::Stale;
        if let Some(store) = &mut self.store { store.set_stale()};
        for x in self.devices.values_mut() {
            x.set_stale();
        }
    }

    fn cancel_reorder_timeout(&mut self) {
        if let Some(token) = self.reorder_timeout_cancel_token.take() { 
            debug!("Cancelling reorder timeout for Node = {:?}", self.id);
            _ = token.send(()); 
        }
    }

    fn handle_birth(&mut self, timestamp: u64, bdseq: u8, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<bool, Option<RebirthReason>> {
        if timestamp <= self.birth_timestamp { return Err(None) }

        let first_birth = self.lifecycle_state == LifecycleState::Unbirthed;
        if !first_birth && self.bdseq == bdseq { return Err(None)}

        if let Some(store) = &mut self.store { 
            if let Err(_) = store.update_from_birth(details) {
                return Err (Some(RebirthReason::InvalidPayload))
            };
        };

        self.cancel_reorder_timeout();
        self.lifecycle_state = LifecycleState::Birthed;
        self.bdseq = bdseq;
        self.resequencer.reset();
        // seq of birth should always be 0 so the next seq we expect is going to be 1
        self.resequencer.set_next_sequence(1);

        Ok (first_birth)
    }

    fn handle_death(&mut self, bdseq: u8, timestamp: u64) -> Result<(), RebirthReason> {
        let mut res = Ok (());

        self.cancel_reorder_timeout();
        // If we receive a death that we don't expect then we should invalidate our current state as we are out of sync and issue a rebirth
        if bdseq != self.bdseq {
            res = Err(RebirthReason::OutOfSyncBdSeq)
        }
        self.set_stale(timestamp);
        res
    }

    fn process_in_sequence_message(&mut self, message: ResequenceableEvent) -> Result<(), RebirthReason> {

        match message {
            ResequenceableEvent::NData(ndata) => {

                if let Some(store) = &mut self.store {
                    match store.update_from_data(ndata.metrics_details) {
                        Ok(_) => return Ok(()),
                        Err(_) => return Err(RebirthReason::InvalidPayload),
                    }
                }
            },
            ResequenceableEvent::DBirth(device_name, dbirth) => {
                let device = match self.devices.get(&device_name) {
                    Some(dev) => dev.clone(),
                    None => {
                        let dev = Device::new(device_name.clone());
                        self.devices.insert(device_name, dev.clone());
                        if let Some(cb) = &self.device_created_cb {
                            cb(&dev)
                        }

                        dev
                    } 
                };
                device.handle_birth(dbirth.metrics_details)?
            },
            ResequenceableEvent::DDeath(device_name) => {
                let device = match self.devices.get(&device_name) {
                    Some(dev) => dev,
                    None => return Err(RebirthReason::UnknownDevice),
                };
                device.handle_death();
            },
            ResequenceableEvent::DData(device_name, ddata) => {
                let device = match self.devices.get(&device_name) {
                    Some(dev) => dev,
                    None => return Err(RebirthReason::UnknownDevice),
                };
                device.handle_data(ddata)?
            },
        }

        Ok(())
    }

    fn handle_resequencable_message(&mut self, seq: u8, message: ResequenceableEvent) -> Result<bool, RebirthReason> {

        if self.lifecycle_state != LifecycleState::Birthed { 
            debug!("Ignoring message for Node = ({:?}) as its current state is Stale)", self.id);
            return Ok (false) 
        }

        let message = match self.resequencer.process(seq, message) {
            resequencer::ProcessResult::MessageNextInSequence(message) => message,
            resequencer::ProcessResult::OutOfSequenceMessageInserted => return Ok(self.reorder_timeout_cancel_token.is_none()),
            resequencer::ProcessResult::DuplicateMessageSequence => return Err(RebirthReason::ReorderFail),
        };

        self.process_in_sequence_message(message)?;

        loop {
            match self.resequencer.drain() {
                resequencer::DrainResult::Message(message) => self.process_in_sequence_message(message)?,
                resequencer::DrainResult::Empty => {
                    self.cancel_reorder_timeout();
                    break
                },
                resequencer::DrainResult::SequenceMissing => break,
            }
        }

        Ok(false)
    }

}

enum RebirthTimerMessage {
    Start(ArcNode, oneshot::Receiver<()>),
}

struct NodeWrapper {
    node: Mutex<Node>,
    id: Arc<NodeIdentifier>,
    client: AppClient,
    rebirth_config: Arc<RebirthConfig>,
    rebirth_timer_tx: mpsc::Sender<RebirthTimerMessage>
}

impl NodeWrapper {

    fn new(id: Arc<NodeIdentifier>, client: AppClient, rebirth_config: Arc<RebirthConfig>, rebirth_timer_tx: mpsc::Sender<RebirthTimerMessage>) -> Self {
        Self {
            node: Mutex::new(Node::new(id.clone())),
            id,
            client,
            rebirth_config,
            rebirth_timer_tx
        }
    }

}

#[derive(Clone)]
struct ArcNode {
    inner: Arc<NodeWrapper>
}

impl ArcNode {

    fn new(client: AppClient, id: Arc<NodeIdentifier>, rebirth_config: Arc<RebirthConfig>, rebirth_timer_tx: mpsc::Sender<RebirthTimerMessage>) -> Self {
        Self { 
            inner: Arc::new (NodeWrapper::new(id, client, rebirth_config, rebirth_timer_tx)),
        }
    }

    fn evaluate_rebirth_reason(config: &RebirthConfig, reason: &RebirthReason) -> bool
    {
        match reason {
            RebirthReason::InvalidPayload => config.invalid_payload,
            RebirthReason::OutOfSyncBdSeq => config.out_of_sync_bdseq,
            RebirthReason::UnknownNode => config.unknown_node,
            RebirthReason::UnknownDevice => config.unknown_device,
            RebirthReason::UnknownMetric => config.unknown_metric,
            RebirthReason::ReorderTimeout => config.reorder_timeout != None,
            RebirthReason::ReorderFail => config.reorder_failure
        }
    }

    async fn issue_rebirth(&self, reason: RebirthReason) {
        if !Self::evaluate_rebirth_reason(&self.inner.rebirth_config, &reason) {
            return
        }
        info!("Issuing rebirth for Node = ({:?}), reason = ({:?})", self.inner.id, reason);
        _ = self.inner.client.publish_node_rebirth(&self.inner.id.group, &self.inner.id.node).await;
    }

    fn handle_birth(&self, timestamp: u64, bdseq: u8, details: Vec<(MetricBirthDetails, MetricDetails)>, cb: &Option<NodeCreatedCallback>) -> Result<(), RebirthReason> {
        let mut node= self.inner.node.lock().unwrap();
        match node.handle_birth(timestamp, bdseq, details) {
            Ok(first_birth) => {
                if first_birth {
                    if let Some(cb) = cb {
                        cb(&self.inner.id, node.deref_mut())
                    }
                } 
            },
            Err(rr) => {
                if let Some(rr) = rr { return Err(rr)} else {return Ok(())};
            },
        }
        Ok(())
    }

    fn handle_death(&self, bdseq: u8, timestamp: u64) -> Result<(), RebirthReason> {
        self.inner.node.lock().unwrap().handle_death(bdseq, timestamp)
    }

    async fn handle_resequencable_message(&self, seq: u8, message: ResequenceableEvent) -> Result<(), RebirthReason> {
        let rx = {
            let mut node = self.inner.node.lock().unwrap();
            let detected_out_of_order = node.handle_resequencable_message(seq, message)?;
            if detected_out_of_order && self.inner.rebirth_config.reorder_timeout.is_some() {
                debug!("Detected out of order message for Node = {:?}", node.id);
                let (tx, rx) = oneshot::channel();
                node.reorder_timeout_cancel_token = Some(tx);
                Some(rx)
            } else { None }
        };
        if let Some(rx) = rx {
            _ = self.inner.rebirth_timer_tx.send(RebirthTimerMessage::Start(
                ArcNode {inner: self.inner.clone()},
                rx
            ) ).await;
        }
        Ok(())
    }

    fn set_stale(&self, timestamp: u64) {
        self.inner.node.lock().unwrap().set_stale(timestamp);
    }

}

#[derive(Debug)]
enum RebirthReason {
    InvalidPayload,
    OutOfSyncBdSeq,
    UnknownNode,
    UnknownDevice,
    UnknownMetric,
    ReorderTimeout, 
    ReorderFail
}

pub struct RebirthConfig {
    pub invalid_payload: bool,
    pub out_of_sync_bdseq: bool,
    pub unknown_node: bool,
    pub unknown_device: bool,
    pub unknown_metric: bool,
    pub reorder_timeout: Option<Duration>,
    pub reorder_failure: bool
}

impl Default for RebirthConfig {
    fn default() -> Self {
        Self { 
            invalid_payload: false,
            out_of_sync_bdseq: true,
            unknown_node: true,
            unknown_device: true,
            unknown_metric: true,
            reorder_timeout: Some(Duration::from_secs(3)),
            reorder_failure: true
        }
    }
}

struct ApplicationState {
    nodes: HashMap<Arc<NodeIdentifier>, ArcNode>,
    reorder_timers: FuturesUnordered<Pin<Box<dyn Future<Output=()> + Send + 'static>>> 
}

pub type OnlineCallback = Pin<Box<dyn Fn() + Send>>;
pub type OfflineCallback = Pin<Box<dyn Fn() + Send>>;

pub type NodeCreatedCallback = Pin<Box<dyn Fn(&NodeIdentifier, &mut Node) + Send + Sync>>;
pub type DeviceCreatedCallback = Pin<Box<dyn Fn(&Device) + Send + Sync>>;

struct AppCallbacks {
    online: Option<OnlineCallback>,
    offline: Option<OfflineCallback>,
    node_created: Option<NodeCreatedCallback>,
}

impl AppCallbacks {
    fn new() -> Self {
        Self { online: None, offline: None, node_created: None }
    }
}

pub struct Application {
    state: ApplicationState,
    eventloop: AppEventLoop,
    client: AppClient,
    rebirth_config: Arc<RebirthConfig>,
    cbs: AppCallbacks,
    rebirth_rx: mpsc::Receiver<RebirthTimerMessage>,
    rebirth_tx: mpsc::Sender<RebirthTimerMessage>,
}

impl Application {
    
    pub fn new<E: EventLoop + Send + 'static, C: Client + Send + Sync + 'static, S: Into<String>>(
        app_id : S, 
        eventloop: E,
        client: C,
        subscription_config: SubscriptionConfig,
    ) -> (Self, AppClient) {
        let (eventloop, client) = AppEventLoop::new(app_id, subscription_config, eventloop, client);
        let (rebirth_tx, rebirth_rx) = mpsc::channel(1);
        let app = Self {
            state: ApplicationState { nodes: HashMap::new(), reorder_timers: FuturesUnordered::new() },
            eventloop,
            client: client.clone(),
            rebirth_config: Arc::new(RebirthConfig::default()),
            cbs: AppCallbacks::new(),
            rebirth_rx,
            rebirth_tx
        };
        (app, client)
    }

    pub fn on_node_created<F>(mut self, cb: F) -> Self 
        where 
            F: Fn(&NodeIdentifier, &mut Node) + Send + Sync + 'static 
    {
        self.cbs.node_created = Some(Box::pin(cb));
        self
    }

    pub fn on_online<F>(mut self, cb: F) -> Self 
        where 
            F: Fn() + Send + 'static 
    {
        self.cbs.online = Some(Box::pin(cb));
        self
    }

    pub fn on_offline<F>(mut self, cb: F) -> Self 
        where 
            F: Fn() + Send + 'static 
    {
        self.cbs.offline = Some(Box::pin(cb));
        self
    }

    pub fn with_rebirth_config(mut self, config: RebirthConfig) -> Self {
        self.rebirth_config = Arc::new(config);
        self
    }

    fn get_node_or_issue_rebirth(&mut self, id: NodeIdentifier) -> Option<ArcNode> {


        let node = {
            if let Some(node) = self.state.nodes.get(&id) {

                if node.inner.node.lock().unwrap().lifecycle_state == LifecycleState::Birthed {
                    return Some(node.clone());
                }

                node.clone()
            }
            else {
                let id = Arc::new(id);
                let node = ArcNode::new(self.client.clone(), id.clone(), self.rebirth_config.clone(), self.rebirth_tx.clone());
                self.state.nodes.insert(id, node.clone());
                node
            }
        };


        tokio::spawn(async move {
            node.issue_rebirth(RebirthReason::UnknownNode).await;
        });

        None 
    }

    fn node_handle_resequenceable_message(node: ArcNode, seq: u8, event: ResequenceableEvent)
    {
        tokio::spawn(async move {
            if let Err(e) = node.handle_resequencable_message(seq, event).await {
                node.issue_rebirth(e).await
            }
        });
    } 

    fn get_or_create_node(&mut self, id: NodeIdentifier) -> ArcNode {
        match self.state.nodes.get(&id) {
            Some(node) => node.clone(),
            None => {
                info!("Creating new Node = ({:?})", id);
                let id = Arc::new(id);
                let node = ArcNode::new(self.client.clone(), id.clone(), self.rebirth_config.clone(), self.rebirth_tx.clone());
                self.state.nodes.insert(id, node.clone());
                node
            },
        }
    }

    fn handle_event(&mut self, event: AppEvent) -> bool {
        trace!("Application event = ({event:?})");
        match event {
            AppEvent::Online => {
                if let Some(on_online) = &self.cbs.online {
                    on_online()
                }
            },
            AppEvent::Offline => {
                let timestamp = timestamp();
                for x in self.state.nodes.values() {
                    x.set_stale(timestamp);
                }
                if let Some(on_offline) = &self.cbs.offline {
                    on_offline()
                }
            },
            AppEvent::Node(node_event) => {
                let id = node_event.id;
                match node_event.event {
                    NodeEvent::NBirth(nbirth) => {
                        let node = self.get_or_create_node(id);
                        let timestamp = nbirth.timestamp;
                        let bdseq = nbirth.bdseq;
                        let details = nbirth.metrics_details;
                        if let Err(e) = node.handle_birth(timestamp, bdseq, details, &self.cbs.node_created) {
                            tokio::spawn(async move {
                                node.issue_rebirth(e).await
                            });
                        }
                    },
                    NodeEvent::NDeath(ndeath) => {
                        let node = match self.state.nodes.get(&id) {
                            Some(node) => node.clone(),
                            None => return false,
                        };
                        let timestamp = timestamp();
                        tokio::spawn(async move {
                            node.handle_death(ndeath.bdseq, timestamp)
                        });
                    },
                    NodeEvent::NData(ndata) => {
                        if let Some(node) = self.get_node_or_issue_rebirth(id) {
                            Self::node_handle_resequenceable_message(node, ndata.seq, ResequenceableEvent::NData(ndata))
                        }
                    },
                }
            },
            AppEvent::Device(device_event) => {

                let node = match self.get_node_or_issue_rebirth(device_event.id) {
                    Some(node) => node,
                    None => return false,
                };
                let device_name = device_event.name;

                match device_event.event {
                    DeviceEvent::DBirth(dbirth) => Self::node_handle_resequenceable_message(node, dbirth.seq, ResequenceableEvent::DBirth(device_name, dbirth)),
                    DeviceEvent::DDeath(ddeath) => Self::node_handle_resequenceable_message(node, ddeath.seq, ResequenceableEvent::DDeath(device_name)),
                    DeviceEvent::DData(ddata) => Self::node_handle_resequenceable_message(node, ddata.seq, ResequenceableEvent::DData(device_name, ddata)),
                }
            },
            AppEvent::InvalidPayload(details) => {
                debug!("Got invalid payload from Node = {:?}, Error = {:?}", details.node_id, details.error);
                if self.rebirth_config.invalid_payload {
                    let node = self.get_or_create_node(details.node_id);
                    tokio::spawn(async move {
                        node.issue_rebirth(RebirthReason::InvalidPayload).await
                    });
                }
            },
            AppEvent::Cancelled => return true,
        };
        return false;
    }

    fn handle_rx_request(&mut self, request: RebirthTimerMessage) {
        match request {
            RebirthTimerMessage::Start(node, cancel_token) => {
                let duration = match self.rebirth_config.reorder_timeout {
                    Some(duration) => duration.clone(),
                    None => return,
                };
                self.state.reorder_timers.push(Box::pin(
                    async move {
                        select! {
                            _ = sleep(duration) => {
                                _ = node.issue_rebirth(RebirthReason::ReorderTimeout).await;
                            },
                            _ = cancel_token => ()
                        };
                    }
                ));
            }
        }
    }

    pub async fn run(&mut self) {
        self.state.nodes.clear();
        self.state.reorder_timers.clear();
        self.state.reorder_timers.next().await;
        loop {
            select! {
                event = self.eventloop.poll() => {
                    if self.handle_event(event) == true { break }
                },
                Some(request) = self.rebirth_rx.recv() => self.handle_rx_request(request),
                Some(_) = self.state.reorder_timers.next() => (),
            }
        }
    }

}
