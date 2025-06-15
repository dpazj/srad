use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use log::{debug, info, trace, warn};
use srad_client::{Client, EventLoop};
use srad_types::{utils::timestamp, MetricId};

use crate::{
    events::{DBirth, DData, DDeath, DeviceEvent, NBirth, NData, NDeath, NodeEvent},
    resequencer::{self, Resequencer},
    AppClient, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier,
    SubscriptionConfig,
};

use tokio::{
    select,
    sync::{mpsc::{self, Receiver, Sender}},
    time::sleep,
};

/// A trait the [Application] uses to interface with custom implementations
///
/// `MetricStore` provides an interface for managing metrics for a device or node, creating new metrics from a birth message and updating existing metrics with new data
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use srad_app::generic_app::{MetricStore, StateUpdateError};
/// use srad_app::{MetricBirthDetails, MetricDetails};
/// use srad_types::MetricId;
///
/// struct InMemoryMetricStore {
///     metrics: HashMap<MetricId, MetricDetails>,
///     is_stale: bool,
/// }
///
/// impl MetricStore for InMemoryMetricStore {
///     fn set_stale(&mut self) {
///         self.is_stale = true;
///     }
///
///     fn update_from_birth(&mut self, details: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), StateUpdateError> {
///         for (birth_details, metric_details) in details {
///             let metric_id = birth_details.get_metric_id();
///             self.metrics.insert(metric_id, metric_details);
///         }
///         Ok(())
///     }
///
///     fn update_from_data(&mut self, details: Vec<(MetricId, MetricDetails)>) -> Result<(), StateUpdateError> {
///         for (id, metric_update_details) in details {
///             match self.metrics.get_mut(&id) {
///                 Some(details) => *details = metric_update_details,
///                 None => return Err(StateUpdateError::UnknownMetric)
///             }
///         }
///         Ok(())
///     }
/// }
/// ```

pub trait MetricStore {
    /// Mark metrics in the store to be stale.
    ///
    /// This method is called when one of the following situations is met:
    /// - A DEATH message is received for the associated Node or Device
    /// - The application goes Offline
    /// - The application detects that there is some form of state synchronisation issue and needs to issue a rebirth command to the Node
    fn set_stale(&mut self);

    /// Called on the occurrence of a new BIRTH message to initialise or reset the known metrics for the Node/Devices lifetime
    fn update_from_birth(
        &mut self,
        details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) -> Result<(), StateUpdateError>;

    /// Called on the occurrence of a new DATA message to indicate there is a new value for one or more metrics
    fn update_from_data(
        &mut self,
        details: Vec<(MetricId, MetricDetails)>,
    ) -> Result<(), StateUpdateError>;
}

/// An error type that can be returned by [MetricStore] to indicate an error when updating it's internal state
pub enum StateUpdateError {
    /// The value provided was invalid
    InvalidValue,
    /// The metric provided was unknown
    UnknownMetric,
}

#[derive(PartialEq, Eq)]
enum LifecycleState {
    Birthed,
    Stale,
}

enum ResequenceableEvent {
    NData(NData),
    DBirth(String, DBirth),
    DDeath(String),
    DData(String, DData),
}

#[derive(Debug)]
enum RebirthReason {
    InvalidPayload,
    OutOfSyncBdSeq,
    UnknownNode,
    UnknownDevice,
    UnknownMetric,
    ReorderTimeout,
    ReorderFail,
    RecordedStateStale,
}

/// A configuration struct used to determine how the Application will handle various situations that might require the issuing of a Node Rebirth CMD
pub struct RebirthConfig {
    /// Cooldown time between rebirth requests for a node
    pub rebirth_cooldown: Duration,
    /// Issue rebirths if we encounter an invalid payload
    pub invalid_payload: bool,
    /// Issue rebirths if an unexpected bdseq value in a NDEATH message, indicating our state is not correctly synced
    pub out_of_sync_bdseq: bool,
    /// Issue rebirth if a message is received from a node not seen before
    pub unknown_node: bool,
    /// Issue rebirth if a message is received from a device not seen before
    pub unknown_device: bool,
    /// Issue rebirth if metric is received that has not been seen in a birth/death message
    pub unknown_metric: bool,
    /// Issue rebirth if we were unable to reorder an out of sequence message
    pub reorder_failure: bool,
    /// Issue rebirth if we have the state for the node as stale but we receive an unexpected message
    pub recorded_state_stale: bool,
    /// Issue rebirth if out of sequence messages could not be reordered in the specified timeout
    pub reorder_timeout: Option<Duration>,
}


impl RebirthConfig {

    fn evaluate_rebirth_reason(&self, reason: &RebirthReason) -> bool {
        match reason {
            RebirthReason::InvalidPayload => self.invalid_payload,
            RebirthReason::OutOfSyncBdSeq => self.out_of_sync_bdseq,
            RebirthReason::UnknownNode => self.unknown_node,
            RebirthReason::UnknownDevice => self.unknown_device,
            RebirthReason::UnknownMetric => self.unknown_metric,
            RebirthReason::ReorderTimeout => self.reorder_timeout.is_some(),
            RebirthReason::ReorderFail => self.reorder_failure,
            RebirthReason::RecordedStateStale => self.recorded_state_stale,
        }
    }

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
            reorder_failure: true,
            rebirth_cooldown: Duration::from_secs(5),
            recorded_state_stale: true,
        }
    }
}

pub struct Device {
    name: String,
    lifecycle_state: LifecycleState,
    store: Option<Box<dyn MetricStore + Send>>,
}

impl Device {
    fn new(name: String) -> Self {
        Self {
            name,
            lifecycle_state: LifecycleState::Stale,
            store: None,
        }
    }

    fn set_stale(&mut self) {
        self.lifecycle_state = LifecycleState::Stale;
        if let Some(x) = &mut self.store {
            x.set_stale()
        }
    }

    fn handle_birth(
        &mut self,
        details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) -> Result<(), RebirthReason> {
        if let Some(x) = &mut self.store {
            if x.update_from_birth(details).is_err() {
                return Err(RebirthReason::InvalidPayload);
            }
        }
        self.lifecycle_state = LifecycleState::Birthed;
        Ok(())
    }

    fn handle_death(&mut self) {
        self.set_stale();
    }

    fn handle_data(&mut self, details: DData) -> Result<(), RebirthReason> {
        if self.lifecycle_state == LifecycleState::Stale {
            info!("Device {} data but is stale", self.name);
            return Err(RebirthReason::RecordedStateStale);
        }
        if let Some(x) = &mut self.store {
            if let Err(e) = x.update_from_data(details.metrics_details) {
                return Err(match e {
                    StateUpdateError::InvalidValue => RebirthReason::InvalidPayload,
                    StateUpdateError::UnknownMetric => RebirthReason::UnknownMetric,
                });
            }
        }
        Ok(())
    }

    /// Register a [MetricStore] implementation with the device
    pub fn register_metric_store<T: MetricStore + Send + 'static>(&mut self, store: T) {
        self.store = Some(Box::new(store))
    }

    /// Get the name of the device
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct Node {
    id: Arc<NodeIdentifier>,
    client: AppClient,
    rx: Receiver<Message>,
    rebirth_rx: Receiver<RebirthReason>,
    rebirth_tx: Sender<RebirthReason>,

    rebirth_config: Arc<RebirthConfig>,

    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<String, Device>,
    store: Option<Box<dyn MetricStore + Send>>,
    device_created_cb: Option<DeviceCreatedCallback>,

    resequence_timeout_task: Option<tokio::task::AbortHandle>,

    //state
    lifecycle_state: LifecycleState,
    birth_timestamp: u64,
    bdseq: u8,
    last_rebirth: Duration,
}

impl Node {

    /// Register a [MetricStore] implementation with the node
    pub fn register_metric_store<T: MetricStore + Send + 'static>(&mut self, store: T) {
        self.store = Some(Box::new(store))
    }

    /// Register a callback to be notified when a new device for this node is created.
    ///
    /// Typical use should just involve creating and registering custom [MetricStore] implementations with the device.
    pub fn on_device_created<F>(&mut self, cb: F)
    where
        F: Fn(&mut Device) + Send + Sync + 'static,
    {
        self.device_created_cb = Some(Box::pin(cb));
    }

    /// Get the NodeIdentifier of the Node
    pub fn id(&self) -> &Arc<NodeIdentifier> {
        &self.id
    }

    fn new(id: Arc<NodeIdentifier>, client: AppClient, rebirth_config: Arc<RebirthConfig>) -> (Self, NodeHandle) {
        let (tx, rx) = mpsc::channel(1024);
        let (rebirth_tx, rebirth_rx) = mpsc::channel(1);

        let handle = NodeHandle::new(tx, rebirth_tx.clone());

        let node = Self {
            id,
            client,
            rx,
            rebirth_rx,
            rebirth_tx,
            rebirth_config,
            devices: HashMap::new(),
            resequencer: Resequencer::new(),
            resequence_timeout_task: None,
            lifecycle_state: LifecycleState::Stale,
            store: None,
            device_created_cb: None,
            birth_timestamp: 0,
            bdseq: 0,
            last_rebirth: Duration::new(0,0)
        };
        (node, handle)
    }

    fn eval_rebirth(&mut self, reason: &RebirthReason) -> bool {

        if !self.rebirth_config.evaluate_rebirth_reason(reason) {
            return false
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        if (now - self.last_rebirth) < self.rebirth_config.rebirth_cooldown {
            trace!("Skipping rebirth for Node = ({:?}), reason = ({:?}) as rebirth cooldown not expired", self.id, reason);
            return false 
        } 
        self.last_rebirth = now;
        true
    }

    async fn issue_rebirth(&mut self, reason: RebirthReason) {
        if !self.eval_rebirth(&reason) { return }
        self.set_stale(timestamp());
        info!(
            "Issuing rebirth for Node = ({:?}), reason = ({:?})",
            self.id, reason
        );
        _ = self.client.publish_node_rebirth(&self.id.group, &self.id.node).await;
    }

    fn cancel_reorder_timeout(&mut self) {
        if let Some(handle) = self.resequence_timeout_task.take()
        {
            debug!("Node = ({:?}) cancelled reorder timeout", self.id);
            handle.abort();
        }
    }

    fn start_reorder_timeout(&mut self) {
        let timeout = match self.rebirth_config.reorder_timeout {
            Some(duration) => duration,
            None => return,
        };

        let rebirth_tx = self.rebirth_tx.clone();
        let id = self.id.clone();
        let abort_handle = tokio::spawn(async move {
            sleep(timeout).await;
            warn!("Unable to reorder out of sequence messages in {}ms - Node = ({id:?}) issuing rebirth.", timeout.as_millis());
            _ = rebirth_tx.try_send(RebirthReason::ReorderTimeout);
        }).abort_handle();
       self.resequence_timeout_task = Some(abort_handle)
    }

    async fn handle_birth(&mut self, birth: NBirth) {
        if birth.timestamp <= self.birth_timestamp {
            debug!("Node {:?} got birth with birth timestamp {} older than most recent timestamp {} - ignoring", self.id, birth.timestamp, self.birth_timestamp);
            return;
        }

        // If this is a duplicate birth message that we will have already received we dont need to notify the metric store
        if !(self.lifecycle_state == LifecycleState::Birthed && self.bdseq == birth.bdseq) {
            if let Some(store) = &mut self.store {
                if store.update_from_birth(birth.metrics_details).is_err() {
                    self.issue_rebirth(RebirthReason::InvalidPayload).await;
                    return
                };
            };
        };

        self.cancel_reorder_timeout();
        self.birth_timestamp = birth.timestamp;
        self.lifecycle_state = LifecycleState::Birthed;
        self.bdseq = birth.bdseq;
        self.resequencer.reset();
        // seq of birth should always be 0 so the next seq we expect is going to be 1
        self.resequencer.set_next_sequence(1);
    }

    fn set_stale(&mut self, timestamp: u64) {
        if self.lifecycle_state == LifecycleState::Stale {
            return
        }
        if timestamp < self.birth_timestamp {
            return
        }
        debug!("Setting Node = {:?} Stale", self.id);
        self.resequencer.reset();
        self.cancel_reorder_timeout();
        self.lifecycle_state = LifecycleState::Stale;
        if let Some(store) = &mut self.store {
            store.set_stale()
        };
        for x in self.devices.values_mut() {
            x.set_stale();
        }
    }

    async fn handle_death(&mut self, bdseq: u8, timestamp: u64) {
        self.cancel_reorder_timeout();
        self.set_stale(timestamp);
        // If we receive a death that we don't expect then we should invalidate our current state as we are out of sync and issue a rebirth
        if bdseq != self.bdseq {
            self.issue_rebirth(RebirthReason::OutOfSyncBdSeq).await
        }
    }

    fn process_in_sequence_message(
        &mut self,
        message: ResequenceableEvent,
    ) -> Result<(), RebirthReason> {
        match message {
            ResequenceableEvent::NData(ndata) => {
                if let Some(store) = &mut self.store {
                    match store.update_from_data(ndata.metrics_details) {
                        Ok(_) => return Ok(()),
                        Err(_) => return Err(RebirthReason::InvalidPayload),
                    }
                }
            }
            ResequenceableEvent::DBirth(device_name, dbirth) => {
                let device = match self.devices.get_mut(&device_name) {
                    Some(dev) => dev,
                    None => {
                        let dev = Device::new(device_name.clone());
                        self.devices.insert(device_name.clone(), dev);
                        let dev_ref = self.devices.get_mut(&device_name).unwrap();
                        if let Some(cb) = &self.device_created_cb {
                            cb(dev_ref)
                        }
                        dev_ref
                    }
                };
                device.handle_birth(dbirth.metrics_details)?
            }
            ResequenceableEvent::DDeath(device_name) => {
                let device = match self.devices.get_mut(&device_name) {
                    Some(dev) => dev,
                    None => return Err(RebirthReason::UnknownDevice),
                };
                device.handle_death();
            }
            ResequenceableEvent::DData(device_name, ddata) => {
                let device = match self.devices.get_mut(&device_name) {
                    Some(dev) => dev,
                    None => return Err(RebirthReason::UnknownDevice),
                };
                device.handle_data(ddata)?
            }
        }

        Ok(())
    }

    async fn drain_resequence_buffer(&mut self) -> Result<(), RebirthReason> {

        loop {
            match self.resequencer.drain() {
                resequencer::DrainResult::Message(message) => {
                    self.process_in_sequence_message(message)?
                }
                resequencer::DrainResult::Empty => {
                    self.cancel_reorder_timeout();
                    break;
                }
                resequencer::DrainResult::SequenceMissing => break,
            }
        }
        Ok(())
    } 
    
    async fn handle_resequencable_message(
        &mut self,
        seq: u8,
        timestamp: u64,
        message: ResequenceableEvent
    ) -> Result<(), RebirthReason> {

        if timestamp < self.birth_timestamp {
            debug!("Ignoring message for Node = ({:?}) as it's timestamp is before the current birth timestamp", self.id);
            return Ok(())
        }

        if self.lifecycle_state != LifecycleState::Birthed {
            debug!(
                "Node = ({:?}) received message but its current state is stale",
                self.id
            );
            return Err(RebirthReason::RecordedStateStale)
        }

        let message = match self.resequencer.process(seq, message) {
            resequencer::ProcessResult::MessageNextInSequence(message) => message,
            resequencer::ProcessResult::OutOfSequenceMessageInserted => {
                debug!(
                    "Node {:?} Got out of order seq {}, expected {}",
                    self.id,
                    seq,
                    self.resequencer.next_sequence()
                );

                if self.resequence_timeout_task.is_none() {
                    self.start_reorder_timeout();
                }
                return Ok(());
            }
            resequencer::ProcessResult::DuplicateMessageSequence => {
                return Err(RebirthReason::ReorderFail)
            }
        };

        self.process_in_sequence_message(message)?;
        self.drain_resequence_buffer().await
    }

    async fn handle_resequenceable_message_wrapper(&mut self, seq: u8, timestamp: u64, message: ResequenceableEvent) {
        if let Err(rebirth_reason) = self.handle_resequencable_message(seq, timestamp, message).await {
            self.issue_rebirth(rebirth_reason).await
        }
    }

    async fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::NBirth(nbirth) => self.handle_birth(nbirth).await,
            Message::NDeath(ndeath, timestamp) => self.handle_death(ndeath.bdseq, timestamp).await,
            Message::NData(ndata) => self.handle_resequenceable_message_wrapper(ndata.seq, ndata.timestamp, ResequenceableEvent::NData(ndata)).await,
            Message::DBirth(dev, dbirth) => self.handle_resequenceable_message_wrapper(dbirth.seq, dbirth.timestamp, ResequenceableEvent::DBirth(dev, dbirth)).await,
            Message::DData(dev, ddata) => self.handle_resequenceable_message_wrapper(ddata.seq, ddata.timestamp, ResequenceableEvent::DData(dev, ddata)).await,
            Message::DDeath(dev, ddeath) => self.handle_resequenceable_message_wrapper(ddeath.seq, ddeath.timestamp, ResequenceableEvent::DDeath(dev)).await,
            Message::Offline => self.set_stale(timestamp()),
        }
    }

    async fn run(mut self) {

        loop {
            select! {
                Some(reason) = self.rebirth_rx.recv() => self.issue_rebirth(reason).await,
                msg = self.rx.recv() => {
                    match msg {
                        Some(msg) => self.handle_message(msg).await,
                        None => break,
                    }
                }
            }
        }

    }
}

#[derive(Debug)]
enum Message {
    NBirth(NBirth),
    NData(NData),
    NDeath(NDeath, u64),
    DBirth(String,DBirth),
    DData(String,DData),
    DDeath(String,DDeath),
    Offline,
}

#[derive(Clone)]
struct NodeHandle {
    tx: Sender<Message>,
    rebirth_tx: Sender<RebirthReason>,
}

impl NodeHandle {

    fn new(tx: Sender<Message>, rebirth_tx: Sender<RebirthReason>) -> Self {
        Self {
            tx,
            rebirth_tx,
        }
    }

    async fn send_message(&self ,message: Message) {
        _ = self.tx.send(message).await;
    }

    fn issue_rebirth(&self, reason: RebirthReason) {
        //try send since if there is already a rebirth request waiting to be processed then no need to issue another
        _ = self.rebirth_tx.try_send(reason);
    }

}


struct ApplicationState {
    nodes: HashMap<Arc<NodeIdentifier>, NodeHandle>,
}

pub type OnlineCallback = Pin<Box<dyn Fn() + Send>>;
pub type OfflineCallback = Pin<Box<dyn Fn() + Send>>;

pub type NodeCreatedCallback = Pin<Box<dyn Fn(&mut Node) + Send + Sync>>;
pub type DeviceCreatedCallback = Pin<Box<dyn Fn(&mut Device) + Send + Sync>>;

struct AppCallbacks {
    online: Option<OnlineCallback>,
    offline: Option<OfflineCallback>,
    node_created: Option<NodeCreatedCallback>,
}

impl AppCallbacks {
    fn new() -> Self {
        Self {
            online: None,
            offline: None,
            node_created: None,
        }
    }
}

/// The Application struct.
///
/// Internally uses an [AppEventLoop]. The corresponding [AppClient] returned from [Application::new()] can be used to interact with the Sparkplug namespace by publishing CMD messages.
pub struct Application {
    state: ApplicationState,
    eventloop: AppEventLoop,
    client: AppClient,
    rebirth_config: Arc<RebirthConfig>,
    cbs: AppCallbacks,
}

impl Application {
    /// Create a new [Application] instance.
    pub fn new<
        E: EventLoop + Send + 'static,
        C: Client + Send + Sync + 'static,
        S: Into<String>,
    >(
        app_id: S,
        eventloop: E,
        client: C,
        subscription_config: SubscriptionConfig,
    ) -> (Self, AppClient) {
        let (eventloop, client) = AppEventLoop::new(app_id, subscription_config, eventloop, client);
        //let (rebirth_tx, rebirth_rx) = mpsc::channel(1);
        let app = Self {
            state: ApplicationState {
                nodes: HashMap::new()
            },
            eventloop,
            client: client.clone(),
            rebirth_config: Arc::new(RebirthConfig::default()),
            cbs: AppCallbacks::new(),
        };
        (app, client)
    }

    /// Register a callback to be notified when a new node is created.
    ///
    /// This is called when the application first discovers the existence of a node, not to be confused with receiving a NBIRTH message from the node.
    ///
    /// Typical use should just involve creating and registering custom [MetricStore] implementations and device added callbacks with the node.
    /// *Note*: This callback is blocking and is called directly from the EventLoop. Blocking will prevent progression.
    pub fn on_node_created<F>(mut self, cb: F) -> Self
    where
        F: Fn(&mut Node) + Send + Sync + 'static,
    {
        self.cbs.node_created = Some(Box::pin(cb));
        self
    }

    /// Register a callback to be notified the Application is Online and has published it's state online message.
    ///
    /// *Note*: This callback is blocking and is called directly from the EventLoop. Blocking will prevent progression.
    pub fn on_online<F>(mut self, cb: F) -> Self
    where
        F: Fn() + Send + 'static,
    {
        self.cbs.online = Some(Box::pin(cb));
        self
    }

    /// Register a callback to be notified the Application is Offline i.e it has disconnected from the broker.
    ///
    /// *Note*: This callback is blocking and is called directly from the EventLoop. Blocking will prevent progression.
    pub fn on_offline<F>(mut self, cb: F) -> Self
    where
        F: Fn() + Send + 'static,
    {
        self.cbs.offline = Some(Box::pin(cb));
        self
    }

    /// Provide the `Application` with a configuration for how it should handle various Rebirth conditions
    pub fn with_rebirth_config(mut self, config: RebirthConfig) -> Self {
        self.rebirth_config = Arc::new(config);
        self
    }

    fn create_node(&mut self, id: NodeIdentifier) -> NodeHandle {
        info!("Creating new Node = ({:?})", id);
        let id = Arc::new(id);

        let (mut node, handle) = Node::new(
            id.clone(), 
            self.client.clone(), 
            self.rebirth_config.clone()
        );
        if let Some(cb) = &self.cbs.node_created {
            cb(&mut node)
        }
        tokio::spawn(node.run());
        self.state.nodes.insert(id, handle.clone());
        handle 
    }

    async fn get_node_or_issue_rebirth(&mut self, id: NodeIdentifier) -> Option<NodeHandle> {

        if let Some(node) = self.state.nodes.get(&id) {
            return Some(node.clone())
        } 

        let node = self.create_node(id.clone());
        node.issue_rebirth(RebirthReason::UnknownNode); 
        None
    }

    fn get_or_create_node(&mut self, id: NodeIdentifier) -> NodeHandle {
        match self.state.nodes.get(&id) {
            Some(node) => node.clone(),
            None => self.create_node(id),
        }
    }

    async fn handle_event(&mut self, event: AppEvent) -> bool {
        trace!("Application event = ({event:?})");
        match event {
            AppEvent::Online => {
                if let Some(on_online) = &self.cbs.online {
                    on_online()
                }
            }
            AppEvent::Offline => {
                for x in self.state.nodes.values() {
                    x.send_message(Message::Offline).await
                }
                if let Some(on_offline) = &self.cbs.offline {
                    on_offline()
                }
            }
            AppEvent::Node(node_event) => {
                let id = node_event.id;
                match node_event.event {
                    NodeEvent::Birth(nbirth) => {
                        let node = self.get_or_create_node(id);
                        node.send_message(Message::NBirth(nbirth)).await;
                    }
                    NodeEvent::Death(ndeath) => {
                        let node = match self.state.nodes.get(&id){
                            Some(node) => node,
                            None => return true,
                        };
                        node.send_message(Message::NDeath(ndeath, timestamp())).await
                    }
                    NodeEvent::Data(ndata) => {
                        let node = match self.get_node_or_issue_rebirth(id).await {
                            Some(node) => node,
                            None => return true,
                        };
                        node.send_message(Message::NData(ndata)).await
                    }
                }
            }
            AppEvent::Device(device_event) => {
                let node = match self.get_node_or_issue_rebirth(device_event.id).await {
                    Some(node) => node,
                    None => return true,
                };
                let device_name = device_event.name;

                match device_event.event {
                    DeviceEvent::Birth(dbirth) => node.send_message(Message::DBirth(device_name,dbirth)).await, 
                    DeviceEvent::Death(ddeath) => node.send_message(Message::DDeath(device_name, ddeath)).await,
                    DeviceEvent::Data(ddata) => node.send_message(Message::DData(device_name, ddata)).await,
                }
            }
            AppEvent::InvalidPayload(details) => {
                debug!(
                    "Got invalid payload from Node = {:?}, Error = {:?}",
                    details.node_id, details.error
                );
                if self.rebirth_config.invalid_payload {
                    let node = self.get_or_create_node(details.node_id);
                    node.issue_rebirth(RebirthReason::InvalidPayload);
                }
            }
            AppEvent::Cancelled => return false,
        };
        true 
    }

    /// Run the Application
    ///
    /// Runs the Application until [AppClient::cancel()] is called
    pub async fn run(mut self) {
        loop {

            let event = self.eventloop.poll().await; 
            if !self.handle_event(event).await { break }
        }
    }

}
