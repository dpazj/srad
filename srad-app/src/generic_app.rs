use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use log::{debug, info, trace, warn};
use srad_client::{Client, EventLoop};
use srad_types::{utils::timestamp, MetricId};

use crate::{
    events::{DBirth, DData, DeviceEvent, NData, NodeEvent},
    resequencer::{self, Resequencer},
    AppClient, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier,
    SubscriptionConfig,
};

use tokio::{
    select,
    sync::{mpsc, oneshot},
    time::sleep,
};

use futures::{stream::FuturesUnordered, StreamExt};

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

#[derive(Debug)]
enum ResequenceableEvent {
    NData(NData),
    DBirth(String, DBirth),
    DDeath(String),
    DData(String, DData),
}

/// A struct that represents the instance of a Device.
///
/// Provided to the user in a callback when the Application creates the device; users should register their [MetricStore] implementations with the device then.
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

/// A struct that represents the instance of a Node.
///
/// Provided to the user in a callback when the Application creates the node; users should register their [MetricStore] implementations with the node then.
pub struct Node {
    id: Arc<NodeIdentifier>,
    lifecycle_state: LifecycleState,
    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<String, Device>,
    store: Option<Box<dyn MetricStore + Send>>,
    bdseq: u8,
    birth_timestamp: u64,
    device_created_cb: Option<DeviceCreatedCallback>,
    reorder_timeout_cancel_token: Option<oneshot::Sender<()>>,
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

    fn new(id: Arc<NodeIdentifier>) -> Self {
        Self {
            id,
            resequencer: Resequencer::new(),
            devices: HashMap::new(),
            lifecycle_state: LifecycleState::Stale,
            bdseq: 0,
            birth_timestamp: 0,
            store: None,
            device_created_cb: None,
            reorder_timeout_cancel_token: None,
            last_rebirth: Duration::new(0, 0),
        }
    }

    fn eval_rebirth(&mut self, rebirth_config: &RebirthConfig, reason: &RebirthReason) -> bool {

        if !rebirth_config.evaluate_rebirth_reason(reason) {
            return false
        }

        self.set_stale(timestamp());
        let can_rebirth = self.rebirth_cooldown_expired_and_update(&rebirth_config.rebirth_cooldown);
        if !can_rebirth {
            trace!("Skipping rebirth for Node = ({:?}), reason = ({:?}) as rebirth cooldown not expired", self.id, reason);
            return false 
        }
        true
    }

    fn set_stale(&mut self, timestamp: u64) {
        if self.lifecycle_state == LifecycleState::Stale {
            return;
        }
        if timestamp < self.birth_timestamp {
            return;
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

    fn rebirth_cooldown_expired_and_update(&mut self, cooldown: &Duration) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        if (now - self.last_rebirth) < *cooldown {
            return false;
        }
        self.last_rebirth = now;
        true
    }

    fn cancel_reorder_timeout(&mut self) {
        if let Some(token) = self.reorder_timeout_cancel_token.take() {
            debug!("Cancelling reorder timeout for Node = {:?}", self.id);
            _ = token.send(());
        }
    }

    fn handle_birth(
        &mut self,
        timestamp: u64,
        bdseq: u8,
        details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) -> Result<(), Option<RebirthReason>> {
        if timestamp <= self.birth_timestamp {
            debug!("Node {:?} got birth with birth timestamp {} older than most recent timestamp {} - ignoring", self.id, timestamp, self.birth_timestamp);
            return Err(None);
        }

        // If this is a duplicate birth message that we will have already received we dont need to notify the metric store
        if !(self.lifecycle_state == LifecycleState::Birthed && self.bdseq == bdseq) {
            if let Some(store) = &mut self.store {
                if store.update_from_birth(details).is_err() {
                    return Err(Some(RebirthReason::InvalidPayload));
                };
            };
        };

        self.cancel_reorder_timeout();
        self.birth_timestamp = timestamp;
        self.lifecycle_state = LifecycleState::Birthed;
        self.bdseq = bdseq;
        self.resequencer.reset();
        // seq of birth should always be 0 so the next seq we expect is going to be 1
        self.resequencer.set_next_sequence(1);

        Ok(())
    }

    fn handle_death(&mut self, bdseq: u8, timestamp: u64) -> Result<(), RebirthReason> {
        let mut res = Ok(());

        self.cancel_reorder_timeout();
        // If we receive a death that we don't expect then we should invalidate our current state as we are out of sync and issue a rebirth
        if bdseq != self.bdseq {
            res = Err(RebirthReason::OutOfSyncBdSeq)
        }
        self.set_stale(timestamp);
        res
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

    fn handle_resequencable_message(
        &mut self,
        seq: u8,
        timestamp: u64,
        message: ResequenceableEvent,
    ) -> Result<bool, RebirthReason> {
        if self.lifecycle_state != LifecycleState::Birthed {
            debug!(
                "Node = ({:?}) received message but its current state is stale",
                self.id
            );
            return Err(RebirthReason::RecordedStateStale);
        }

        if timestamp < self.birth_timestamp {
            debug!("Ignoring message for Node = ({:?}) as it's timestamp is before the current birth timestamp", self.id);
            return Ok(false);
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
                return Ok(self.reorder_timeout_cancel_token.is_none());
            }
            resequencer::ProcessResult::DuplicateMessageSequence => {
                return Err(RebirthReason::ReorderFail)
            }
        };

        self.process_in_sequence_message(message)?;

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
    rebirth_timer_tx: mpsc::Sender<RebirthTimerMessage>,
}

impl NodeWrapper {
    fn new(
        id: Arc<NodeIdentifier>,
        client: AppClient,
        rebirth_config: Arc<RebirthConfig>,
        rebirth_timer_tx: mpsc::Sender<RebirthTimerMessage>,
        node: Node,
    ) -> Self {
        Self {
            node: Mutex::new(node),
            id,
            client,
            rebirth_config,
            rebirth_timer_tx,
        }
    }
}

#[derive(Clone)]
struct ArcNode {
    inner: Arc<NodeWrapper>,
}

impl ArcNode {
    fn new(
        client: AppClient,
        id: Arc<NodeIdentifier>,
        rebirth_config: Arc<RebirthConfig>,
        rebirth_timer_tx: mpsc::Sender<RebirthTimerMessage>,
        node: Node,
    ) -> Self {
        Self {
            inner: Arc::new(NodeWrapper::new(
                id,
                client,
                rebirth_config,
                rebirth_timer_tx,
                node,
            )),
        }
    }

    async fn publish_rebirth(&self, reason: RebirthReason) {
        info!(
            "Issuing rebirth for Node = ({:?}), reason = ({:?})",
            self.inner.id, reason
        );

        _ = self
            .inner
            .client
            .publish_node_rebirth(&self.inner.id.group, &self.inner.id.node)
            .await;
    }

    async fn try_rebirth(&self, reason: RebirthReason) {
        if !self.inner.node.lock().unwrap().eval_rebirth(&self.inner.rebirth_config, &reason) { return }
        self.publish_rebirth(reason).await
    }

    fn try_rebirth_task(&self, reason: RebirthReason) {
        if !self.inner.node.lock().unwrap().eval_rebirth(&self.inner.rebirth_config, &reason) { return }
        self.start_rebirth_task(reason);
    }

    fn start_rebirth_task(&self, reason: RebirthReason){
        let node = self.clone();
        tokio::spawn(async move {
            node.publish_rebirth(reason).await
        });
    }

    fn handle_birth(
        &self,
        timestamp: u64,
        bdseq: u8,
        details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) {
        let mut node = self.inner.node.lock().unwrap();
        match node.handle_birth(timestamp, bdseq, details) {
            Ok(_) => (),
            Err(rr) => {
                match rr {
                    Some(reason) => {
                        if node.eval_rebirth(&self.inner.rebirth_config, &reason){
                            self.start_rebirth_task(reason);
                        }
                    },
                    None => (),
                }
            }
        }
    }

    fn handle_death(&self, bdseq: u8, timestamp: u64) -> Result<(), RebirthReason> {
        self.inner
            .node
            .lock()
            .unwrap()
            .handle_death(bdseq, timestamp)
    }

    async fn handle_resequencable_message(
        &self,
        seq: u8,
        timestamp: u64,
        message: ResequenceableEvent,
    ){

        let res = {
            let mut node = self.inner.node.lock().unwrap();
            match node.handle_resequencable_message(seq, timestamp, message) {
                Ok(out_of_order) => {
                    if out_of_order && self.inner.rebirth_config.reorder_timeout.is_some() {
                        debug!("Detected out of order message for Node = {:?}", node.id);
                        let (tx, rx) = oneshot::channel();
                        node.reorder_timeout_cancel_token = Some(tx);
                        Ok(Some(rx))
                    } else {
                        Ok(None)
                    }
                },
                Err(rebirth_reason) => {
                    node.eval_rebirth(&self.inner.rebirth_config, &rebirth_reason);
                    Err(rebirth_reason)
                }
            }
        };

        match res {
            Ok(rx) => {
                if let Some(rx) = rx {
                    _ = self
                        .inner
                        .rebirth_timer_tx
                        .send(RebirthTimerMessage::Start(
                            ArcNode {
                                inner: self.inner.clone(),
                            },
                            rx,
                        ))
                        .await;
                }
            },
            Err(rr) => self.publish_rebirth(rr).await,
        }

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

struct ApplicationState {
    nodes: HashMap<Arc<NodeIdentifier>, ArcNode>,
    reorder_timers: FuturesUnordered<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
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
    rebirth_rx: mpsc::Receiver<RebirthTimerMessage>,
    rebirth_tx: mpsc::Sender<RebirthTimerMessage>,
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
        let (rebirth_tx, rebirth_rx) = mpsc::channel(1);
        let app = Self {
            state: ApplicationState {
                nodes: HashMap::new(),
                reorder_timers: FuturesUnordered::new(),
            },
            eventloop,
            client: client.clone(),
            rebirth_config: Arc::new(RebirthConfig::default()),
            cbs: AppCallbacks::new(),
            rebirth_rx,
            rebirth_tx,
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

    fn create_node(&mut self, id: NodeIdentifier) -> ArcNode {
        info!("Creating new Node = ({:?})", id);
        let id = Arc::new(id);
        let mut node = Node::new(id.clone());

        if let Some(cb) = &self.cbs.node_created {
            cb(&mut node)
        }

        let arc_node = ArcNode::new(
            self.client.clone(),
            id.clone(),
            self.rebirth_config.clone(),
            self.rebirth_tx.clone(),
            node,
        );
        self.state.nodes.insert(id, arc_node.clone());
        arc_node
    }

    fn get_node_or_issue_rebirth(&mut self, id: NodeIdentifier) -> Option<ArcNode> {
        let node = {
            if let Some(node) = self.state.nodes.get(&id) {
                return Some(node.clone())
            } else {
                self.create_node(id)
            }
        };
        node.try_rebirth_task(RebirthReason::UnknownNode);
        None
    }

    fn node_handle_resequenceable_message(
        node: ArcNode,
        seq: u8,
        timestamp: u64,
        event: ResequenceableEvent,
    ) {
        tokio::spawn(async move {
            node.handle_resequencable_message(seq, timestamp, event).await;
        });
    }

    fn get_or_create_node(&mut self, id: NodeIdentifier) -> ArcNode {
        match self.state.nodes.get(&id) {
            Some(node) => node.clone(),
            None => self.create_node(id),
        }
    }

    fn handle_event(&mut self, event: AppEvent) -> bool {
        trace!("Application event = ({event:?})");
        match event {
            AppEvent::Online => {
                if let Some(on_online) = &self.cbs.online {
                    on_online()
                }
            }
            AppEvent::Offline => {
                let timestamp = timestamp();
                for x in self.state.nodes.values() {
                    x.set_stale(timestamp);
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
                        let timestamp = nbirth.timestamp;
                        let bdseq = nbirth.bdseq;
                        let details = nbirth.metrics_details;
                        node.handle_birth(timestamp, bdseq, details)
                    }
                    NodeEvent::Death(ndeath) => {
                        let node = match self.state.nodes.get(&id) {
                            Some(node) => node.clone(),
                            None => return false,
                        };
                        let timestamp = timestamp();
                        tokio::spawn(async move { node.handle_death(ndeath.bdseq, timestamp) });
                    }
                    NodeEvent::Data(ndata) => {
                        if let Some(node) = self.get_node_or_issue_rebirth(id) {
                            Self::node_handle_resequenceable_message(
                                node,
                                ndata.seq,
                                ndata.timestamp,
                                ResequenceableEvent::NData(ndata),
                            )
                        }
                    }
                }
            }
            AppEvent::Device(device_event) => {
                let node = match self.get_node_or_issue_rebirth(device_event.id) {
                    Some(node) => node,
                    None => return false,
                };
                let device_name = device_event.name;

                match device_event.event {
                    DeviceEvent::Birth(dbirth) => Self::node_handle_resequenceable_message(
                        node,
                        dbirth.seq,
                        dbirth.timestamp,
                        ResequenceableEvent::DBirth(device_name, dbirth),
                    ),
                    DeviceEvent::Death(ddeath) => Self::node_handle_resequenceable_message(
                        node,
                        ddeath.seq,
                        ddeath.timestamp,
                        ResequenceableEvent::DDeath(device_name),
                    ),
                    DeviceEvent::Data(ddata) => Self::node_handle_resequenceable_message(
                        node,
                        ddata.seq,
                        ddata.timestamp,
                        ResequenceableEvent::DData(device_name, ddata),
                    ),
                }
            }
            AppEvent::InvalidPayload(details) => {
                debug!(
                    "Got invalid payload from Node = {:?}, Error = {:?}",
                    details.node_id, details.error
                );
                if self.rebirth_config.invalid_payload {
                    let node = self.get_or_create_node(details.node_id);
                    node.try_rebirth_task(RebirthReason::InvalidPayload);
                }
            }
            AppEvent::Cancelled => return true,
        };
        false
    }

    fn handle_rx_request(&mut self, request: RebirthTimerMessage) {
        match request {
            RebirthTimerMessage::Start(node, cancel_token) => {
                let duration = match self.rebirth_config.reorder_timeout {
                    Some(duration) => duration,
                    None => return,
                };
                self.state.reorder_timers.push(Box::pin(async move {
                    select! {
                        _ = sleep(duration) => {
                            _ = node.try_rebirth(RebirthReason::ReorderTimeout).await;
                        },
                        _ = cancel_token => ()
                    };
                }));
            }
        }
    }

    /// Run the Application
    ///
    /// Runs the Application until [AppClient::cancel()] is called
    pub async fn run(mut self) {
        loop {
            select! {
                event = self.eventloop.poll() => {
                    if self.handle_event(event) { break }
                },
                Some(request) = self.rebirth_rx.recv() => self.handle_rx_request(request),
                Some(_) = self.state.reorder_timers.next() => (),
            }
        }
    }
}
