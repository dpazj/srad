use std::{collections::{HashMap, VecDeque}, future::Future, ops::DerefMut, pin::Pin, rc::Rc, sync::{Arc, Mutex}, time::Duration};

use srad_client::{Client, EventLoop};
use srad_types::{payload::DataType, utils::timestamp, MetricId, MetricValueKind};

use crate::{app, events::{DBirth, DData, DDeath, DeviceEvent, NBirth, NData, NDeath, NodeEvent}, metrics, resequencer::{self, Resequencer}, AppClient, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier, SubscriptionConfig};

use tokio::{select, sync::mpsc, time::{sleep, Sleep}};
use tokio::time::timeout;

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


// impl State {

//     fn new() -> Self {
//         Self {
//             metrics: HashMap::new(),
//             lifecycle_state: LifecycleState::Unbirthed,
//         }
//     }

//     fn set_stale(&mut self) {
//         self.lifecycle_state = LifecycleState::Stale;
//         for x in self.metrics.values_mut() {
//             x.stale = true;
//         }
//     }

//     fn update_from_birth(&mut self, metrics: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), StateUpdateError> {

//         self.lifecycle_state = LifecycleState::Birthed;

//         self.metrics.clear();

//         for (x, y) in metrics {

//             let id = if let Some(alias) = x.alias {
//                 MetricId::Alias(alias)
//             } else {
//                 MetricId::Name(x.name.clone())
//             };

//             let value = match y.value {
//                 Some(val) => match MetricValueKind::try_from_metric_value(x.datatype, val) {
//                     Ok(value) => Some(value),
//                     Err(_) => return Err(StateUpdateError::InvalidValue),
//                 },
//                 None => None,
//             };

//             let metric = Metric::new(x.datatype, value);
//             self.metrics.insert(id, metric);
//         }

//         Ok(())
//     }

//     fn update_from_data(&mut self, metrics: Vec<(MetricId, MetricDetails)>) -> Result<(), StateUpdateError> {
//         for (id, details) in metrics {
//             let metric = match self.metrics.get_mut(&id) {
//                 Some(metric) => metric,
//                 None => return Err(StateUpdateError::UnknownMetric),
//             };

//             let value = match details.value {
//                 Some(val) => match MetricValueKind::try_from_metric_value(metric.datatype, val) {
//                     Ok(value) => Some(value),
//                     Err(_) => return Err(StateUpdateError::InvalidValue),
//                 },
//                 None => None,
//             };

//             metric.value = value;
//         }

//         Ok(())
//     }

// }

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
    lifecycle_state: LifecycleState,
    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<String, Device>,
    store: Option<Box<dyn MetricStore + Send>>,
    bdseq: u8,
    birth_timestamp: u64,
    stale_timestamp: u64,
    device_created_cb: Option<DeviceCreatedCallback>,
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

    fn new() -> Self {
        Self {
            resequencer: Resequencer::new(),
            devices: HashMap::new(),
            lifecycle_state: LifecycleState::Unbirthed,
            bdseq: 0,
            birth_timestamp: 0,
            stale_timestamp: 0,
            store: None, 
            device_created_cb: None
        }
    }

    fn set_stale(&mut self, timestamp: u64) 
    {
        if timestamp > self.birth_timestamp { return }
        self.resequencer.reset();
        if let Some(store) = &mut self.store { store.set_stale()};
        self.stale_timestamp = timestamp;
        for x in self.devices.values_mut() {
            x.set_stale();
        }
    }

    fn handle_birth(&mut self, timestamp: u64, bdseq: u8, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<bool, Option<RebirthReason>> {
        if timestamp <= self.birth_timestamp { return Err(None) }
        if timestamp < self.stale_timestamp { return Err(None) }

        let first_birth = self.lifecycle_state == LifecycleState::Unbirthed;
        if !first_birth && self.bdseq == bdseq { return Err(None)}

        if let Some(store) = &mut self.store { 
            if let Err(_) = store.update_from_birth(details) {
                return Err (Some(RebirthReason::InvalidPayload))
            };
        };

        self.lifecycle_state = LifecycleState::Birthed;
        self.bdseq = bdseq;
        self.resequencer.reset();
        // seq of birth should always be 0 so the next seq we expect is going to be 1
        self.resequencer.set_next_sequence(1);

        Ok (first_birth)
    }

    fn handle_death(&mut self, bdseq: u8, timestamp: u64) -> Result<(), RebirthReason> {
        let mut res = Ok (());

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

    fn handle_resequencable_message(&mut self, seq: u8, message: ResequenceableEvent) -> Result<(), RebirthReason> {

        if self.lifecycle_state != LifecycleState::Birthed { return Ok (()) }

        println!("resequence: message {message:?}");

        let message = match self.resequencer.process(seq, message) {
            resequencer::ProcessResult::MessageNextInSequence(message) => message,
            resequencer::ProcessResult::OutOfSequenceMessageInserted => return Ok(()),
            //resequencer::ProcessResult::DuplicateMessageSequence => return Ok(()),
            resequencer::ProcessResult::DuplicateMessageSequence => return Err(RebirthReason::ReorderTimeout),
        };

        self.process_in_sequence_message(message)?;

        loop {
            match self.resequencer.drain() {
                resequencer::DrainResult::Message(message) => self.process_in_sequence_message(message)?,
                resequencer::DrainResult::Empty => break,
                resequencer::DrainResult::SequenceMissing => break,
            }
        }

        Ok(())
    }

}

enum RebirthTimerMessage {
    Start(ArcNode),
    Cancel(Arc<NodeIdentifier>),
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
            node: Mutex::new(Node::new()),
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

    fn evaluate_rebirth_reason(config: &RebirthConfig, reason: RebirthReason) -> bool
    {
        match reason {
            RebirthReason::InvalidPayload => config.invalid_payload,
            RebirthReason::OutOfSyncBdSeq => config.out_of_sync_bdseq,
            RebirthReason::UnknownNode => config.unknown_node,
            RebirthReason::UnknownDevice => config.unknown_device,
            RebirthReason::UnknownMetric => config.unknown_metric,
            RebirthReason::ReorderTimeout => config.reorder_timeout,
        }
    }

    async fn issue_rebirth(&self, reason: RebirthReason) {
        if !Self::evaluate_rebirth_reason(&self.inner.rebirth_config, reason) {
            return
        }
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

    fn handle_resequencable_message(&self, seq: u8, message: ResequenceableEvent) -> Result<(), RebirthReason> {
        self.inner.node.lock().unwrap().handle_resequencable_message(seq, message)
    }

    fn set_stale(&self, timestamp: u64) {
        self.inner.node.lock().unwrap().set_stale(timestamp);
    }


    pub fn id(&self) -> &NodeIdentifier {
        &self.inner.id
    }

}

#[derive(Debug)]
enum RebirthReason {
    InvalidPayload,
    OutOfSyncBdSeq,
    UnknownNode,
    UnknownDevice,
    UnknownMetric,
    ReorderTimeout
}

pub struct RebirthConfig {
    invalid_payload: bool,
    out_of_sync_bdseq: bool,
    unknown_node: bool,
    unknown_device: bool,
    unknown_metric: bool,
    reorder_timeout: bool
}

impl Default for RebirthConfig {
    fn default() -> Self {
        Self { 
            invalid_payload: true,
            out_of_sync_bdseq: true,
            unknown_node: true,
            unknown_device: true,
            unknown_metric: true,
            reorder_timeout: true 
        }
    }
}

struct ApplicationState {
    nodes: HashMap<Arc<NodeIdentifier>, ArcNode>,
    reorder_timers: HashMap<Arc<NodeIdentifier>, ()>
}

pub type OnlineCallback = Pin<Box<dyn Fn() + Send>>;
pub type OfflineCallback = Pin<Box<dyn Fn() + Send>>;

pub type NodeCreatedCallback = Pin<Box<dyn Fn(&NodeIdentifier, &mut Node) + Send + Sync>>;
pub type DeviceCreatedCallback = Pin<Box<dyn Fn(&Device) + Send + Sync>>;

struct AppCallbacks {
    online: Option<OnlineCallback>,
    offline: Option<OfflineCallback>,
    node_created: Arc<Option<NodeCreatedCallback>>,
}

impl AppCallbacks {
    fn new() -> Self {
        Self { online: None, offline: None, node_created: Arc::new(None) }
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
    ) -> Self {
        let (eventloop, client) = AppEventLoop::new(app_id, subscription_config, eventloop, client);
        let (rebirth_tx, rebirth_rx) = mpsc::channel(1);
        Self {
            state: ApplicationState { nodes: HashMap::new(), reorder_timers: HashMap::new()},
            eventloop,
            client,
            rebirth_config: Arc::new(RebirthConfig::default()),
            cbs: AppCallbacks::new(),
            rebirth_rx,
            rebirth_tx
        }
    }

    pub fn on_node_created<F>(mut self, cb: F) -> Self 
        where 
            F: Fn(&NodeIdentifier, &mut Node) + Send + Sync + 'static 
    {
        self.cbs.node_created = Arc::new(Some(Box::pin(cb)));
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
            if let Err(e) = node.handle_resequencable_message(seq, event) {
                node.issue_rebirth(e).await
            }
        });
    } 

    fn handle_event(&mut self, event: AppEvent) -> bool {
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
                        let node = match self.state.nodes.get(&id) {
                            Some(node) => node.clone(),
                            None => {
                                let id = Arc::new(id);
                                let node = ArcNode::new(self.client.clone(), id.clone(), self.rebirth_config.clone(), self.rebirth_tx.clone());
                                self.state.nodes.insert(id, node.clone());
                                node
                            },
                        };
                        let timestamp = nbirth.timestamp;
                        let bdseq = nbirth.bdseq;
                        let details = nbirth.metrics_details;
                        let cb = self.cbs.node_created.clone();
                        tokio::spawn(async move {
                            if let Err(e) = node.handle_birth(timestamp, bdseq, details, &cb) {
                                node.issue_rebirth(e).await
                            }
                        });
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

            },
            AppEvent::Cancelled => return true,
        };
        return false;
 
    }

    fn handle_rx_request(&mut self, request: RebirthTimerMessage) {
        match request {
            RebirthTimerMessage::Start(node) => {
                let fut = Box::pin(async {
                    sleep(Duration::from_secs(3)).await;
                    node.issue_rebirth(RebirthReason::ReorderTimeout)
                });
                //self.state.reorder_timers.insert(node.inner.id.clone(), fut);
            },
            RebirthTimerMessage::Cancel(node_identifier) => {
                self.state.reorder_timers.remove(&node_identifier);
            },
        }
    }

    async fn poll_for_timeout(&self) {
        let timeout = sleep(Duration::from_millis(1000));

        //let mut futures: [Pin<&mut Sleep>; ]

        for timeout in self.state.reorder_timers.values() {
            
            //let a = x.as_mut();
        }
    }

    pub async fn run(&mut self) {
        loop {
            select! {
                event = self.eventloop.poll() => {
                    if self.handle_event(event) == true { break }
                },
                request = self.rebirth_rx.recv() => {

                }
            }
        }
    }
}
