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
    events::{DBirth, DData, DDeath, DeviceEvent, NBirth, NData, NDeath, NodeEvent},
    resequencer::{self, Resequencer},
    AppClient, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier,
    SubscriptionConfig,
};

use tokio::{
    select,
    sync::{mpsc::{self, Receiver, Sender}, oneshot},
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

struct Node {
    id: Arc<NodeIdentifier>,
    client: AppClient,
    rx: Receiver<Message>,

    resequencer: Resequencer<ResequenceableEvent>,

    //state
    lifecycle_state: LifecycleState,
    birth_timestamp: u64,
    bdseq: u8,
}

impl Node {

    fn new(id: Arc<NodeIdentifier>, client: AppClient) -> (Self, NodeHandle) {
        let (tx, rx) = mpsc::channel(1024);
        let handle = NodeHandle::new(tx);

        let node = Self {
            id,
            client,
            rx,
            resequencer: Resequencer::new(),
            lifecycle_state: LifecycleState::Stale,
            birth_timestamp: 0,
            bdseq: 0
        };
        (node, handle)
    }

    async fn issue_rebirth(&mut self) {

    }

    fn cancel_reorder_timeout(&self) {

    }

    async fn handle_birth(&mut self, birth: NBirth) {
        if birth.timestamp <= self.birth_timestamp {
            debug!("Node {:?} got birth with birth timestamp {} older than most recent timestamp {} - ignoring", self.id, birth.timestamp, self.birth_timestamp);
            return;
        }

        // If this is a duplicate birth message that we will have already received we dont need to notify the metric store
        if !(self.lifecycle_state == LifecycleState::Birthed && self.bdseq == birth.bdseq) {
            // if let Some(store) = &mut self.store {
            //     if store.update_from_birth(birth.metrics_details).is_err() {
            //         return Err(Some(RebirthReason::InvalidPayload));
            //     };
            // };
        };

        self.cancel_reorder_timeout();
        self.birth_timestamp = birth.timestamp;
        self.lifecycle_state = LifecycleState::Birthed;
        self.bdseq = birth.bdseq;

        // seq of birth should always be 0 so the next seq we expect is going to be 1
        self.resequencer.set_next_sequence(1);


    }

    async fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::NBirth(nbirth) => self.handle_birth(nbirth).await,
            Message::NData(ndata) => todo!(),
            Message::NDeath(ndeath, timestamp) => todo!(),
            Message::DBirth(nbirth) => todo!(),
            Message::DData(ndata) => todo!(),
            Message::DDeath(ddeath) => todo!(),
            Message::Offline => todo!(),
        }
    }

    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            self.handle_message(msg).await 
        }
    }
}

enum Message {
    NBirth(NBirth),
    NData(NData),
    NDeath(NDeath, u64),
    DBirth(DBirth),
    DData(DData),
    DDeath(DDeath),
    Offline
}

#[derive(Clone)]
struct NodeHandle {
    tx: Sender<Message>
}

impl NodeHandle {

    fn new(tx: Sender<Message>) -> Self {
        Self {
            tx
        }
    }

    async fn send_message(&self ,message: Message) {
        self.tx.send(message).await.unwrap();
    }

}


struct ApplicationState {
    nodes: HashMap<Arc<NodeIdentifier>, NodeHandle>,
    reorder_timers: FuturesUnordered<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
}

pub type OnlineCallback = Pin<Box<dyn Fn() + Send>>;
pub type OfflineCallback = Pin<Box<dyn Fn() + Send>>;

pub type NodeCreatedCallback = Pin<Box<dyn Fn(&mut Node) + Send + Sync>>;
//pub type DeviceCreatedCallback = Pin<Box<dyn Fn(&mut Device) + Send + Sync>>;

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
    // rebirth_rx: mpsc::Receiver<RebirthTimerMessage>,
    // rebirth_tx: mpsc::Sender<RebirthTimerMessage>,
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
                nodes: HashMap::new(),
                reorder_timers: FuturesUnordered::new(),
            },
            eventloop,
            client: client.clone(),
            rebirth_config: Arc::new(RebirthConfig::default()),
            cbs: AppCallbacks::new(),
            // rebirth_rx,
            // rebirth_tx,
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

        let (mut node, handle) = Node::new(id.clone(), self.client.clone());
        if let Some(cb) = &self.cbs.node_created {
            cb(&mut node)
        }
        tokio::spawn(node.run());
        self.state.nodes.insert(id, handle.clone());
        handle 
    }

    fn get_node_or_issue_rebirth(&mut self, id: NodeIdentifier) -> Option<NodeHandle> {

        if let Some(node) = self.state.nodes.get(&id) {
                return Some(node.clone())
        } 

        let client = self.client.clone(); 
        self.create_node(id.clone());
        tokio::spawn(async move {
            client.publish_node_rebirth(&id.group, &id.node).await;
        });
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
                    //x.set_stale();
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
                        let node = match self.get_node_or_issue_rebirth(id) {
                            Some(node) => node,
                            None => return true,
                        };
                        node.send_message(Message::NDeath(ndeath, timestamp())).await
                    }
                    NodeEvent::Data(ndata) => {
                        let node = match self.get_node_or_issue_rebirth(id) {
                            Some(node) => node,
                            None => return true,
                        };
                        node.send_message(Message::NData(ndata)).await
                    }
                }
            }
            AppEvent::Device(device_event) => {
                let node = match self.get_node_or_issue_rebirth(device_event.id) {
                    Some(node) => node,
                    None => return true,
                };
                let device_name = device_event.name;

                match device_event.event {
                    DeviceEvent::Birth(dbirth) => node.send_message(Message::DBirth(dbirth)).await, 
                    DeviceEvent::Death(ddeath) => node.send_message(Message::DDeath(ddeath)).await,
                    DeviceEvent::Data(ddata) => node.send_message(Message::DData(ddata)).await,
                }
            }
            AppEvent::InvalidPayload(details) => {
                // debug!(
                //     "Got invalid payload from Node = {:?}, Error = {:?}",
                //     details.node_id, details.error
                // );
                // if self.rebirth_config.invalid_payload {
                //     let node = self.get_or_create_node(details.node_id);
                //     node.try_rebirth_task(RebirthReason::InvalidPayload);
                // }
            }
            AppEvent::Cancelled => return false,
        };
        true 
    }

    // fn handle_rx_request(&mut self, request: RebirthTimerMessage) {
    //     match request {
    //         RebirthTimerMessage::Start(node, cancel_token) => {
    //             let duration = match self.rebirth_config.reorder_timeout {
    //                 Some(duration) => duration,
    //                 None => return,
    //             };
    //             self.state.reorder_timers.push(Box::pin(async move {
    //                 select! {
    //                     _ = sleep(duration) => {
    //                         _ = node.try_rebirth(RebirthReason::ReorderTimeout).await;
    //                     },
    //                     _ = cancel_token => ()
    //                 };
    //             }));
    //         }
    //     }
    // }

    /// Run the Application
    ///
    /// Runs the Application until [AppClient::cancel()] is called
    pub async fn run(mut self) {
        loop {
            select! {
                event = self.eventloop.poll() => {
                    if !self.handle_event(event).await { break }
                },
                //Some(request) = self.rebirth_rx.recv() => self.handle_rx_request(request),
                Some(_) = self.state.reorder_timers.next() => (),
            }
        }
    }
}
