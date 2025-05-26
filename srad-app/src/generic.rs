use std::{collections::{HashMap}, pin::Pin, sync::{Arc, Mutex}};

use srad_client::{Client, EventLoop};
use srad_types::{payload::DataType, utils::timestamp, MetricId, MetricValueKind};

use crate::{app, events::{DBirth, DData, DDeath, DeviceEvent, NBirth, NData, NDeath, NodeEvent}, metrics, resequencer::{self, Resequencer}, AppClient, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier, SubscriptionConfig};

struct Metric {
    stale: bool,
    datatype: DataType,
    value: Option<MetricValueKind>
}

impl Metric {

    fn new(datatype: DataType, value: Option<MetricValueKind>) -> Self {
        Self {
            stale: false,
            datatype,
            value
        }
    }
    
}

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

struct State {
    metrics: HashMap<MetricId, Metric>,
    lifecycle_state: LifecycleState,
}

impl State {

    fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            lifecycle_state: LifecycleState::Unbirthed,
        }
    }

    fn set_stale(&mut self) {
        self.lifecycle_state = LifecycleState::Stale;
        for x in self.metrics.values_mut() {
            x.stale = true;
        }
    }

    fn update_from_birth(&mut self, metrics: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), StateUpdateError> {

        self.lifecycle_state = LifecycleState::Birthed;

        self.metrics.clear();

        for (x, y) in metrics {

            let id = if let Some(alias) = x.alias {
                MetricId::Alias(alias)
            } else {
                MetricId::Name(x.name.clone())
            };

            let value = match y.value {
                Some(val) => match MetricValueKind::try_from_metric_value(x.datatype, val) {
                    Ok(value) => Some(value),
                    Err(_) => return Err(StateUpdateError::InvalidValue),
                },
                None => None,
            };

            let metric = Metric::new(x.datatype, value);
            self.metrics.insert(id, metric);
        }

        Ok(())
    }

    fn update_from_data(&mut self, metrics: Vec<(MetricId, MetricDetails)>) -> Result<(), StateUpdateError> {
        for (id, details) in metrics {
            let metric = match self.metrics.get_mut(&id) {
                Some(metric) => metric,
                None => return Err(StateUpdateError::UnknownMetric),
            };

            let value = match details.value {
                Some(val) => match MetricValueKind::try_from_metric_value(metric.datatype, val) {
                    Ok(value) => Some(value),
                    Err(_) => return Err(StateUpdateError::InvalidValue),
                },
                None => None,
            };

            metric.value = value;
        }

        Ok(())
    }

}

enum ResequenceableEvent {
    NData(NData),
    DBirth(String, DBirth),
    DDeath(String),
    DData(String, DData)
}

struct DeviceInner {
    state: State
}

impl DeviceInner {

    fn set_stale(&mut self) {
        self.state.set_stale();
    }

    fn handle_birth(&mut self, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), RebirthReason> {
        if let Err(_) = self.state.update_from_birth(details) {
            return Err (RebirthReason::InvalidPayload)
        }
        Ok (())
    }

    fn handle_death(&mut self) {
        self.set_stale();
    }

    fn handle_data(&mut self, details: DData) -> Result<(), RebirthReason> {
        if let Err(e) = self.state.update_from_data(details.metrics_details) {
            return Err(match e {
                StateUpdateError::InvalidValue => RebirthReason::InvalidPayload,
                StateUpdateError::UnknownMetric => RebirthReason::UnknownMetric,
            });
        }
        Ok (())
    }

}

#[derive(Clone)]
pub struct Device {
    inner: Arc<Mutex<DeviceInner>> 
}

impl Device {

    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(DeviceInner { state: State::new() })) 
        }
    }

    fn set_stale(&self) {
        self.inner.lock().unwrap().set_stale();
    }

    fn handle_birth(&self, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), RebirthReason> {
        self.inner.lock().unwrap().handle_birth(details)
    }

    fn handle_death(&self) {
        self.inner.lock().unwrap().handle_death()
    }

    fn handle_data(&self, details: DData) -> Result<(), RebirthReason> {
        self.inner.lock().unwrap().handle_data(details)
    }

    pub fn set_metric_store<T: MetricStore + Send + 'static>(&self, store: T) {
        //self.inner.state.lock().unwrap().store = Some(Box::new(store))
    }

}

struct NodeState {
    lifecycle_state: LifecycleState,
    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<String, Device>,
    store: Option<Box<dyn MetricStore + Send>>,
    bdseq: u8,
    birth_timestamp: u64,
    stale_timestamp: u64,
    device_created_cb: Option<DeviceCreatedCallback>,
}

impl NodeState {

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

        if self.bdseq == bdseq { return Err(None)}

        if let Some(store) = &mut self.store { 
            if let Err(_) = store.update_from_birth(details) {
                return Err (Some(RebirthReason::InvalidPayload))
            };
        };

        let first_birth = self.lifecycle_state == LifecycleState::Unbirthed;
        self.lifecycle_state = LifecycleState::Birthed;
        self.bdseq = bdseq;
        self.resequencer.reset();

        Ok (first_birth)
    }

    fn handle_death(&mut self, bdseq: u8, timestamp: u64) -> Result<(), RebirthReason> {
        let mut res = Ok (());

        // If we receive a death that we dont expect then we should invalidate our current state as we are out of sync and issue a rebirth
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
                        let dev = Device::new();
                        self.devices.insert(device_name, dev.clone());
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

        let message = match self.resequencer.process(seq, message) {
            resequencer::ProcessResult::MessageNextInSequence(message) => message,
            resequencer::ProcessResult::OutOfSequenceMessageInserted => return Ok(()),
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

struct NodeInner {
    state: Mutex<NodeState>,
    id: Arc<NodeIdentifier>,
    client: AppClient,
    rebirth_config: Arc<RebirthConfig>,
}

impl NodeInner {

    fn new(id: Arc<NodeIdentifier>, client: AppClient, rebirth_config: Arc<RebirthConfig>) -> Self {
        Self {
            state: Mutex::new(NodeState::new()),
            id,
            client,
            rebirth_config
        }
    }

}

#[derive(Clone)]
pub struct Node {
    inner: Arc<NodeInner>
}

impl Node {

    fn new(client: AppClient, id: Arc<NodeIdentifier>, rebirth_config: Arc<RebirthConfig>) -> Self {
        Self { 
            inner: Arc::new (NodeInner::new(id, client, rebirth_config)),
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
        let mut state = self.inner.state.lock().unwrap();
        match state.handle_birth(timestamp, bdseq, details) {
            Ok(first_birth) => {
                if first_birth {
                    if let Some(cb) = cb {
                        cb(Node { inner: self.inner.clone() })
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
        self.inner.state.lock().unwrap().handle_death(bdseq, timestamp)
    }

    fn handle_resequencable_message(&self, seq: u8, message: ResequenceableEvent) -> Result<(), RebirthReason> {
        self.inner.state.lock().unwrap().handle_resequencable_message(seq, message)
    }

    fn set_stale(&self, timestamp: u64) {
        self.inner.state.lock().unwrap().set_stale(timestamp);
    }

    pub fn set_metric_store<T: MetricStore + Send + 'static>(&self, store: T) {
        self.inner.state.lock().unwrap().store = Some(Box::new(store))
    }

    pub fn set_device_created_cb<F>(&self, cb: F) 
        where 
            F: Fn(Device) + Send + Sync + 'static,
    
    {
        self.inner.state.lock().unwrap().device_created_cb = Some(Box::pin(cb));
    }

}

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

pub struct ApplicationState {
    nodes: HashMap<Arc<NodeIdentifier>, Node>,
}

pub type OnlineCallback = Pin<Box<dyn Fn() + Send>>;
pub type OfflineCallback = Pin<Box<dyn Fn() + Send>>;

pub type NodeCreatedCallback = Pin<Box<dyn Fn(Node) + Send + Sync>>;
pub type DeviceCreatedCallback = Pin<Box<dyn Fn(Device) + Send + Sync>>;

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
    state: Arc<Mutex<ApplicationState>>,
    eventloop: AppEventLoop,
    client: AppClient,
    rebirth_config: Arc<RebirthConfig>,
    cbs: AppCallbacks,
}

impl Application {
    
    pub fn new<E: EventLoop + Send + 'static, C: Client + Send + Sync + 'static, S: Into<String>>(
        app_id : S, 
        eventloop: E,
        client: C,
        subscription_config: SubscriptionConfig,
    ) -> Self {
        let (eventloop, client) = AppEventLoop::new(app_id, subscription_config, eventloop, client);

        Self {
            state: Arc::new(Mutex::new(ApplicationState { nodes: HashMap::new() })),
            eventloop,
            client,
            rebirth_config: Arc::new(RebirthConfig::default()),
            cbs: AppCallbacks::new() 
        }
    }

    fn get_node_or_issue_rebirth(&mut self, id: NodeIdentifier) -> Option<Node> {

        let mut app_state = self.state.lock().unwrap();
        if let Some(node) = app_state.nodes.get(&id) {
            return Some(node.clone());
        }

        let id = Arc::new(id);
        let node = Node::new(self.client.clone(), id.clone(), self.rebirth_config.clone());
        app_state.nodes.insert(id, node.clone());

        tokio::spawn(async move {
            node.issue_rebirth(RebirthReason::UnknownNode).await;
        });

        None 
    }

    pub async fn run(&mut self) {
        loop {
            match self.eventloop.poll().await {
                AppEvent::Online => {
                    if let Some(on_online) = &self.cbs.online {
                        on_online()
                    }
                },
                AppEvent::Offline => {
                    let state = self.state.clone();
                    let timestamp = timestamp();
                    tokio::spawn(async move {
                        let app_state = state.lock().unwrap();
                        for x in app_state.nodes.values() {
                            x.set_stale(timestamp);
                        }
                    });
                    if let Some(on_offline) = &self.cbs.offline {
                        on_offline()
                    }
                },
                AppEvent::Node(node_event) => {
                    let id = node_event.id;
                    match node_event.event {
                        NodeEvent::NBirth(nbirth) => {
                            let mut app_state = self.state.lock().unwrap();
                            let node = match app_state.nodes.get(&id) {
                                Some(node) => node.clone(),
                                None => {
                                    let id = Arc::new(id);
                                    let node = Node::new(self.client.clone(), id.clone(), self.rebirth_config.clone());
                                    app_state.nodes.insert(id, node.clone());
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
                            let app_state = self.state.lock().unwrap();
                            let node = match app_state.nodes.get(&id) {
                                Some(node) => node.clone(),
                                None => continue,
                            };
                            let timestamp = timestamp();
                            tokio::spawn(async move {
                                node.handle_death(ndeath.bdseq, timestamp)
                            });
                        },
                        NodeEvent::NData(ndata) => {
                            if let Some(node) = self.get_node_or_issue_rebirth(id) {
                                tokio::spawn(async move {
                                    if let Err(e) = node.handle_resequencable_message(ndata.seq,ResequenceableEvent::NData(ndata)) {
                                        node.issue_rebirth(e).await
                                    }
                                });
                            }
                        },
                    }
                },
                AppEvent::Device(device_event) => {

                    let node = match self.get_node_or_issue_rebirth(device_event.id) {
                        Some(node) => node,
                        None => return,
                    };
                    let device_name = device_event.name;

                    match device_event.event {
                        DeviceEvent::DBirth(dbirth) => {
                            tokio::spawn(async move {
                                if let Err(e) = node.handle_resequencable_message(dbirth.seq, ResequenceableEvent::DBirth(device_name, dbirth)) {
                                    node.issue_rebirth(e).await
                                }
                            });
                        },
                        DeviceEvent::DDeath(ddeath) => {
                            tokio::spawn(async move {
                                if let Err(e) = node.handle_resequencable_message(ddeath.seq, ResequenceableEvent::DDeath(device_name)) {
                                    node.issue_rebirth(e).await
                                }
                            });
                        },
                        DeviceEvent::DData(ddata) => {
                            tokio::spawn(async move {
                                if let Err(e) = node.handle_resequencable_message(ddata.seq, ResequenceableEvent::DData(device_name, ddata)) {
                                    node.issue_rebirth(e).await
                                }
                            });
                        },
                    }
                },
                AppEvent::InvalidPayload(details) => {

                    

                },
                AppEvent::Cancelled => break,
            }
        }        
    }
}
