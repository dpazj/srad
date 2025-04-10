use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use log::{debug, error, info, warn};
use srad_client::{
    Client, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, MessageKind, NodeMessage,
    StatePayload,
};
use srad_types::{
    constants::{BDSEQ, NODE_CONTROL_REBIRTH},
    payload::{Metric, Payload, ToMetric},
    topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter},
    utils::{self, timestamp},
    MetricId, MetricValue,
};

use crate::{
    config::SubscriptionConfig,
    metrics::{
        get_metric_birth_details_from_birth_metrics,
        get_metric_id_and_details_from_payload_metrics, MetricBirthDetails, MetricDetails,
        PublishMetric,
    },
};

use tokio::{
    select,
    sync::mpsc::{self, Receiver},
    task,
    time::timeout,
};

/// A token used to help track birth death lifecycles.
///
/// `BirthToken` allows implementing applications to determine if a message for device or node belongs to the
/// current or previous birth death lifecycle. This is required due to the async nature of the app
/// where data messages might arrive after an object has produced a death message or has re-birthed.
///
/// Typically, a `BirthToken` will be provided on a birth callback. An implementing application should then use this token to
/// compare the token provided with other callbacks that reference a node or device.
pub struct BirthToken(Arc<AtomicBool>);

impl BirthToken {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    fn refresh(&mut self) {
        self.invalidate();
        self.0 = Arc::new(AtomicBool::new(false))
    }

    fn invalidate(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn eq(&self, other: &BirthToken) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn is_dead(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

struct DeviceState {
    birth_token: BirthToken,
}

struct NodeState {
    seq: u8,
    bdseq: u8,
    birth_token: BirthToken,
    devices: HashMap<String, DeviceState>,
}

enum SeqState {
    OrderGood,
}

impl NodeState {
    fn new(seq: u8, bdseq: u8) -> Self {
        Self {
            seq,
            bdseq,
            devices: HashMap::new(),
            birth_token: BirthToken::new(),
        }
    }

    fn reset_seq(&mut self) {
        self.seq = 0
    }

    fn validate_and_update_seq(&mut self, seq: u8) -> SeqState {
        self.seq = seq;
        SeqState::OrderGood
    }

    fn get_device(&mut self, device_name: &String) -> Option<&mut DeviceState> {
        self.devices.get_mut(device_name)
    }

    fn add_device(&mut self, name: String) -> BirthToken {
        let tok = BirthToken::new();
        self.devices.insert(
            name,
            DeviceState {
                birth_token: tok.clone(),
            },
        );
        tok
    }
}

/// Used to uniquely identify a node
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct NodeIdentifier {
    pub group: String,
    pub node: String,
}

/// Represents a situation which the application has identified where a implementation might wish to issue a Node rebirth CMD
#[derive(Debug)]
pub enum RebirthReason {
    UnknownNode,
    UnknownDevice,
    SeqOutOfOrder,
    MalformedPayload,
}

/// Details surrounding the situation where the application has identified a [RebirthReason].
pub struct RebirthReasonDetails {
    pub node_id: NodeIdentifier,
    pub device: Option<String>,
    pub reason: RebirthReason,
}

impl RebirthReasonDetails {
    fn new(node_id: NodeIdentifier, reason: RebirthReason) -> Self {
        Self {
            node_id,
            device: None,
            reason,
        }
    }

    fn with_device(mut self, device: String) -> Self {
        self.device = Some(device);
        self
    }
}

struct NamespaceState {
    nodes: Mutex<HashMap<NodeIdentifier, NodeState>>,
}

impl NamespaceState {
    fn new() -> Self {
        NamespaceState {
            nodes: Mutex::new(HashMap::new()),
        }
    }

    fn bdseq_from_payload_metrics(vec: &Vec<Metric>) -> Result<u8, ()> {
        for x in vec {
            match &x.name {
                Some(name) => {
                    if name != BDSEQ {
                        continue;
                    }
                }
                None => continue,
            }
            match &x.value {
                Some(x) => match i64::try_from(MetricValue::from(x.clone())) {
                    Ok(v) => {
                        if v > u8::MAX as i64 || v < 0 {
                            error!("Got invalid bdseq value = {v}");
                            return Err(());
                        }
                        return Ok(v as u8);
                    }
                    Err(_) => {
                        debug!("Could not decode Payload metrics bdseq metric value as i64");
                        return Err(());
                    }
                },
                None => {
                    debug!("Payload metrics bdseq metric did not have a value");
                    return Err(());
                }
            };
        }
        debug!("Payload metrics did not contain a bdseq metric");
        Err(())
    }

    fn handle_node_message(
        &self,
        message: NodeMessage,
        callbacks: &Callbacks,
    ) -> Option<RebirthReasonDetails> {
        let id = NodeIdentifier {
            group: message.group_id,
            node: message.node_id,
        };
        let message_kind = message.message.kind;
        let payload = message.message.payload;

        match message_kind {
            MessageKind::Birth => {
                let seq = match payload.seq {
                    Some(seq) => seq as u8,
                    None => {
                        warn!(
                            "Message did not contain a seq number - discarding. node = {:?}",
                            id
                        );
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };
                let timestamp = match payload.timestamp {
                    Some(ts) => ts,
                    None => {
                        warn!(
                            "Message did not contain a timestamp - discarding. node = {:?}",
                            id
                        );
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };

                let bdseq = match Self::bdseq_from_payload_metrics(&payload.metrics) {
                    Ok(bdseq) => bdseq,
                    Err(_) => {
                        warn!("Birth Message contained an invalid bdseq metric - discarding payload. node = {:?}", id);
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };

                let metric_details =
                    match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                        Ok(details) => details,
                        Err(e) => {
                            warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
                            return Some(RebirthReasonDetails::new(
                                id,
                                RebirthReason::MalformedPayload,
                            ));
                        }
                    };

                let mut nodes = self.nodes.lock().unwrap();
                let tok = match nodes.get_mut(&id) {
                    Some(node) => {
                        if seq != 0 {
                            warn!("Birth payload sequence value is not zero - Discarding. node = {:?}", id);
                            return Some(RebirthReasonDetails::new(
                                id,
                                RebirthReason::MalformedPayload,
                            ));
                        }
                        node.reset_seq();
                        node.bdseq = bdseq;
                        node.birth_token.refresh();
                        node.birth_token.clone()
                    }
                    None => {
                        let node = NodeState::new(seq, bdseq);
                        let tok = node.birth_token.clone();
                        nodes.insert(id.clone(), node);
                        tok
                    }
                };
                if let Some(callback) = &callbacks.nbirth {
                    callback(id, tok, timestamp, metric_details)
                };
            }
            MessageKind::Death => {
                let bdseq = match Self::bdseq_from_payload_metrics(&payload.metrics) {
                    Ok(bdseq) => bdseq,
                    Err(_) => {
                        warn!("Birth Message contained an invalid bdseq metric - discarding payload. node = {:?}", id);
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };
                let mut nodes = self.nodes.lock().unwrap();
                let node = match nodes.get_mut(&id) {
                    Some(node) => node,
                    None => return None,
                };

                if bdseq != node.bdseq {
                    debug!("Death bdseq did not match current known birth bdseq - ignoring. node = {:?}", id);
                    return None;
                }

                /* Duplicate death */
                if node.birth_token.is_dead() {
                    debug!(
                        "Death message already received for bdseq {} - ignoring. node = {:?}",
                        bdseq, id
                    );
                    return None;
                }
                node.birth_token.invalidate();
                for x in node.devices.values() {
                    x.birth_token.invalidate();
                }
                let tok = node.birth_token.clone();
                if let Some(callback) = &callbacks.ndeath {
                    callback(id, tok)
                };
            }
            MessageKind::Data => {
                let seq = match payload.seq {
                    Some(seq) => seq as u8,
                    None => {
                        warn!(
                            "Message did not contain a seq number - discarding. node = {:?}",
                            id
                        );
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };
                let timestamp = match payload.timestamp {
                    Some(ts) => ts,
                    None => {
                        warn!(
                            "Message did not contain a timestamp - discarding. node = {:?}",
                            id
                        );
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };
                let mut nodes = self.nodes.lock().unwrap();
                let node = match nodes.get_mut(&id) {
                    Some(node) => node,
                    None => return Some(RebirthReasonDetails::new(id, RebirthReason::UnknownNode)),
                };

                node.validate_and_update_seq(seq);
                let tok = node.birth_token.clone();
                drop(nodes);

                let details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
                {
                    Ok(details) => details,
                    Err(e) => {
                        warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
                        return Some(RebirthReasonDetails::new(
                            id,
                            RebirthReason::MalformedPayload,
                        ));
                    }
                };

                if let Some(callback) = &callbacks.ndata {
                    let callback = callback.clone();
                    task::spawn(callback(id, tok, timestamp, details));
                };
            }
            _ => (),
        };
        None
    }

    fn handle_device_message(
        &self,
        message: DeviceMessage,
        callbacks: &Callbacks,
    ) -> Option<RebirthReasonDetails> {
        let id = NodeIdentifier {
            group: message.group_id,
            node: message.node_id,
        };
        let device_id = message.device_id;
        let message_kind = message.message.kind;
        let payload = message.message.payload;
        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                warn!(
                    "Message did not contain a seq number - discarding. device = {:?} node = {:?}",
                    device_id, id
                );
                return Some(
                    RebirthReasonDetails::new(id, RebirthReason::MalformedPayload)
                        .with_device(device_id),
                );
            }
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => {
                warn!(
                    "Message did not contain a timestamp - discarding. device = {:?} node = {:?}",
                    device_id, id
                );
                return Some(
                    RebirthReasonDetails::new(id, RebirthReason::MalformedPayload)
                        .with_device(device_id),
                );
            }
        };

        let mut nodes = self.nodes.lock().unwrap();
        let node = match nodes.get_mut(&id) {
            Some(node) => node,
            None => {
                return Some(RebirthReasonDetails::new(id, RebirthReason::UnknownNode));
            }
        };
        node.validate_and_update_seq(seq);

        match message_kind {
            MessageKind::Birth => {
                let metric_details =
                    match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                        Ok(details) => details,
                        Err(e) => {
                            warn!(
                                "Message payload was invalid - {:?}. node = {:?}, device = {}",
                                e, id, device_id
                            );
                            return Some(
                                RebirthReasonDetails::new(id, RebirthReason::MalformedPayload)
                                    .with_device(device_id),
                            );
                        }
                    };

                let tok = match node.get_device(&device_id) {
                    Some(dev) => {
                        dev.birth_token.refresh();
                        dev.birth_token.clone()
                    }
                    None => node.add_device(device_id.clone()),
                };

                if let Some(callback) = &callbacks.dbirth {
                    callback(id, device_id, tok, timestamp, metric_details);
                }
            }
            MessageKind::Death => {
                let tok = match node.get_device(&device_id) {
                    Some(dev) => dev.birth_token.clone(),
                    None => return None,
                };
                if tok.is_dead() {
                    debug!(
                        "Duplicate death for device - ignoring. device = {} node = {:?}",
                        device_id, id
                    );
                    return None;
                }
                tok.invalidate();
                if let Some(callback) = &callbacks.ddeath {
                    callback(id, device_id, tok);
                }
            }
            MessageKind::Data => {
                let tok = match node.get_device(&device_id) {
                    Some(dev) => dev.birth_token.clone(),
                    None => {
                        return Some(
                            RebirthReasonDetails::new(id, RebirthReason::UnknownDevice)
                                .with_device(device_id),
                        )
                    }
                };
                drop(nodes);
                let details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
                {
                    Ok(details) => details,
                    Err(e) => {
                        warn!(
                            "Message payload was invalid - {:?}. node = {:?}, device = {}",
                            e, id, device_id
                        );
                        return Some(
                            RebirthReasonDetails::new(id, RebirthReason::MalformedPayload)
                                .with_device(device_id),
                        );
                    }
                };

                if let Some(callback) = &callbacks.ddata {
                    let callback = callback.clone();
                    task::spawn(callback(id, device_id, tok, timestamp, details));
                };
            }
            _ => (),
        }
        None
    }
}

struct Shutdown;

#[derive(Debug, Clone)]
enum PublishTopicKind {
    NodeTopic(NodeTopic),
    DeviceTopic(DeviceTopic),
}

/// Configuration for a topic to publish a Metric on
#[derive(Debug, Clone)]
pub struct PublishTopic(PublishTopicKind);

impl PublishTopic {
    /// Create a new `PublishTopic` which will publish on a specified device's CMD topic
    pub fn new_device_cmd(group_id: &String, node_id: &String, device_id: &String) -> Self {
        PublishTopic(PublishTopicKind::DeviceTopic(DeviceTopic::new(
            group_id,
            srad_types::topic::DeviceMessage::DCmd,
            node_id,
            device_id,
        )))
    }

    /// Create a new `PublishTopic` which will publish on a specified node's CMD topic
    pub fn new_node_cmd(group_id: &String, node_id: &String) -> Self {
        PublishTopic(PublishTopicKind::NodeTopic(NodeTopic::new(
            group_id,
            srad_types::topic::NodeMessage::NCmd,
            node_id,
        )))
    }
}

/// The Application client to interact with the Application and Sparkplug namespace from.
#[derive(Clone)]
pub struct AppClient(Arc<DynClient>, mpsc::Sender<Shutdown>, Arc<AppState>);

impl AppClient {
    /// Stop all operations, sending a death certificate (application offline message) and disconnect from the broker.
    ///
    /// This will cancel [App::run()]
    pub async fn cancel(&self) {
        info!("App Stopping");
        let topic = StateTopic::new_host(&self.2.host_id);
        match self
            .0
            .try_publish_state_message(
                topic,
                srad_client::StatePayload::Offline {
                    timestamp: timestamp(),
                },
            )
            .await
        {
            Ok(_) => (),
            Err(_) => debug!("Unable to publish state offline on exit"),
        };
        _ = self.1.send(Shutdown).await;
        _ = self.0.disconnect().await;
    }

    /// Issue a node rebirth CMD request
    pub async fn publish_node_rebirth(
        &self,
        group_id: &String,
        node_id: &String,
    ) -> Result<(), ()> {
        let topic = PublishTopic::new_node_cmd(group_id, node_id);
        let rebirth_cmd = PublishMetric::new(MetricId::Name(NODE_CONTROL_REBIRTH.into()), true);
        self.publish_metrics(topic, vec![rebirth_cmd]).await
    }

    fn metrics_to_payload(metrics: Vec<PublishMetric>) -> Payload {
        let mut payload_metrics = Vec::with_capacity(metrics.len());
        for x in metrics.into_iter() {
            payload_metrics.push(x.to_metric());
        }
        Payload {
            timestamp: Some(utils::timestamp()),
            metrics: payload_metrics,
            seq: None,
            uuid: None,
            body: None,
        }
    }

    /// Attempts to publish metrics on a specified topic. Uses the `try_publish` methods from the [srad_client::Client] trait.
    pub async fn try_publish_metrics(
        &self,
        topic: PublishTopic,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), ()> {
        let payload = Self::metrics_to_payload(metrics);
        match topic.0 {
            PublishTopicKind::NodeTopic(topic) => {
                self.0.try_publish_node_message(topic, payload).await
            }
            PublishTopicKind::DeviceTopic(topic) => {
                self.0.try_publish_device_message(topic, payload).await
            }
        }
    }

    /// Publish metrics on a specified topic. Uses the `publish` methods from the [srad_client::Client] trait.
    pub async fn publish_metrics(
        &self,
        topic: PublishTopic,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), ()> {
        let payload = Self::metrics_to_payload(metrics);
        match topic.0 {
            PublishTopicKind::NodeTopic(topic) => self.0.publish_node_message(topic, payload).await,
            PublishTopicKind::DeviceTopic(topic) => {
                self.0.publish_device_message(topic, payload).await
            }
        }
    }
}

struct Callbacks {
    online: Option<Pin<Box<dyn Fn() + Send>>>,
    offline: Option<Pin<Box<dyn Fn() + Send>>>,
    nbirth: Option<
        Pin<
            Box<
                dyn Fn(
                        NodeIdentifier,
                        BirthToken,
                        u64,
                        Vec<(MetricBirthDetails, MetricDetails)>,
                    )
                    + Send,
            >,
        >,
    >,
    ndeath: Option<Pin<Box<dyn Fn(NodeIdentifier, BirthToken) + Send>>>,
    ndata: Option<
        Arc<
            dyn Fn(
                    NodeIdentifier,
                    BirthToken,
                    u64,
                    Vec<(MetricId, MetricDetails)>,
                ) -> Pin<Box<dyn Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    >,
    dbirth: Option<
        Pin<
            Box<
                dyn Fn(
                        NodeIdentifier,
                        String,
                        BirthToken,
                        u64,
                        Vec<(MetricBirthDetails, MetricDetails)>,
                    )
                    + Send,
            >,
        >,
    >,
    ddeath: Option<Pin<Box<dyn Fn(NodeIdentifier, String, BirthToken) + Send>>>,
    ddata: Option<
        Arc<
            dyn Fn(
                    NodeIdentifier,
                    String,
                    BirthToken,
                    u64,
                    Vec<(MetricId, MetricDetails)>,
                ) -> Pin<Box<dyn Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    >,
    evaluate_rebirth_reason: Option<
        Arc<dyn Fn(RebirthReasonDetails) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
    >,
}

struct AppState {
    host_id: String,
    online: AtomicBool,
    published_online_state: AtomicBool,
}

/// Structure that represents a Sparkplug Application instance
pub struct App {
    app_state: Arc<AppState>,
    will_timestamp: u64,
    subscription_config: SubscriptionConfig,
    client: AppClient,
    eventloop: Box<DynEventLoop>,
    state: NamespaceState,
    callbacks: Callbacks,
    shutdown_rx: Receiver<Shutdown>,
}

impl App {
    /// Creates a new instance along with an associated client.
    pub fn new<
        S: Into<String>,
        E: EventLoop + Send + 'static,
        C: Client + Send + Sync + 'static,
    >(
        host_id: S,
        subscription_config: SubscriptionConfig,
        eventloop: E,
        client: C,
    ) -> (Self, AppClient) {
        let callbacks = Callbacks {
            online: None,
            offline: None,
            nbirth: None,
            ndeath: None,
            ndata: None,
            dbirth: None,
            ddeath: None,
            ddata: None,
            evaluate_rebirth_reason: None,
        };
        let (tx, rx) = mpsc::channel(1);

        let host_id: String = host_id.into();
        if let Err(e) = utils::validate_name(&host_id) {
            panic!("Invalid host id: {e}");
        };

        let app_state = Arc::new(AppState {
            host_id,
            online: AtomicBool::new(false),
            published_online_state: AtomicBool::new(false),
        });
        let client = AppClient(Arc::new(client), tx, app_state.clone());
        let app = Self {
            app_state,
            will_timestamp: 0,
            client: client.clone(),
            eventloop: Box::new(eventloop),
            subscription_config,
            state: NamespaceState::new(),
            callbacks,
            shutdown_rx: rx,
        };
        (app, client)
    }

    /// Registers a callback function to be invoked when the application comes online.
    pub fn on_online<F>(&mut self, cb: F) -> &mut Self
    where
        F: Fn() + Send + 'static,
    {
        self.callbacks.online = Some(Box::pin(cb));
        self
    }

    /// Registers a callback function to be invoked when the application goes offline.
    pub fn on_offline<F>(&mut self, cb: F) -> &mut Self
    where
        F: Fn() + Send + 'static,
    {
        self.callbacks.offline = Some(Box::pin(cb));
        self
    }

    /// Registers a callback function to be invoked when a node birth certificate is received.
    pub fn on_nbirth<F>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(NodeIdentifier, BirthToken, u64, Vec<(MetricBirthDetails, MetricDetails)>)
            + Send
            + 'static,
    {
        self.callbacks.nbirth = Some(Box::pin(cb));
        self
    }

    /// Registers a callback function to be invoked when a node death certificate is received.
    ///
    /// When this callback is triggered, all devices that belong to the node should also be considered dead.
    pub fn on_ndeath<F>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(NodeIdentifier, BirthToken) + Send + 'static,
    {
        self.callbacks.ndeath = Some(Box::pin(cb));
        self
    }

    /// Registers an asynchronous callback function to be invoked when node data is received.
    pub fn on_ndata<F, Fut>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(NodeIdentifier, BirthToken, u64, Vec<(MetricId, MetricDetails)>) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let callback = Arc::new(move |id, tok, time, data| {
            Box::pin(cb(id, tok, time, data)) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.callbacks.ndata = Some(callback);
        self
    }

    /// Registers a callback function to be invoked when a device death certificate is received.
    pub fn on_dbirth<F>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(
                NodeIdentifier,
                String,
                BirthToken,
                u64,
                Vec<(MetricBirthDetails, MetricDetails)>,
            )
            + Send
            + 'static,
    {
        self.callbacks.dbirth = Some(Box::pin(cb));
        self
    }

    /// Registers a callback function to be invoked when a device death certificate is received.
    pub fn on_ddeath<F>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(NodeIdentifier, String, BirthToken) + Send + 'static,
    {
        self.callbacks.ddeath = Some(Box::pin(cb));
        self
    }

    /// Registers an asynchronous callback function to be invoked when device data is received.
    pub fn on_ddata<F, Fut>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(NodeIdentifier, String, BirthToken, u64, Vec<(MetricId, MetricDetails)>) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let callback = Arc::new(move |id, device, tok, time, data| {
            Box::pin(cb(id, device, tok, time, data)) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.callbacks.ddata = Some(callback);
        self
    }

    /// Registers an asynchronous callback function to be invoked when the application determines it has encountered a
    /// situation where an application MAY wish to issue a rebirth CMD.
    pub fn register_evaluate_rebirth_reason_fn<F, Fut>(&mut self, cb: F) -> &mut Self
    where
        F: Fn(RebirthReasonDetails) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let callback = Arc::new(move |reason| {
            Box::pin(cb(reason)) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.callbacks.evaluate_rebirth_reason = Some(callback);
        self
    }

    fn update_last_will(&mut self) {
        self.will_timestamp = timestamp();
        self.eventloop.set_last_will(srad_client::LastWill::new_app(
            &self.app_state.host_id,
            self.will_timestamp,
        ));
    }

    fn handle_online(&self) {
        if self
            .app_state
            .online
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        };
        info!("App Online");
        if let Some(callback) = &self.callbacks.online {
            callback()
        };
        let client = self.client.0.clone();
        let state_topic = StateTopic::new_host(&self.app_state.host_id);
        let mut topics: Vec<TopicFilter> = self.subscription_config.clone().into();
        if let SubscriptionConfig::AllGroups = self.subscription_config {
        } else {
            //If subscription config == AllGroups, then we get STATE subscriptions for free
            topics.push(TopicFilter::new_with_qos(
                Topic::State(state_topic.clone()),
                QoS::AtMostOnce,
            ));
        }
        let timestamp = self.will_timestamp;
        let app_state = self.client.2.clone();
        task::spawn(async move {
            _ = client.subscribe_many(topics).await;
            _ = client
                .publish_state_message(state_topic, srad_client::StatePayload::Online { timestamp })
                .await;
            app_state
                .published_online_state
                .store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }

    fn handle_offline(&mut self) {
        if !self
            .app_state
            .online
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        };
        info!("App Offline");
        self.app_state
            .published_online_state
            .store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(callback) = &self.callbacks.offline {
            callback()
        };
        self.update_last_will();
    }

    async fn handle_event(&mut self, event: Event) {
        match event {
            Event::Online => self.handle_online(),
            Event::Offline => self.handle_offline(),
            Event::Node(node_message) => {
                if let Some(reason) = self
                    .state
                    .handle_node_message(node_message, &self.callbacks)
                {
                    if let Some(cb) = &self.callbacks.evaluate_rebirth_reason {
                        let cb = cb.clone();
                        task::spawn(cb(reason));
                    }
                }
            }
            Event::Device(device_message) => {
                if let Some(reason) = self
                    .state
                    .handle_device_message(device_message, &self.callbacks)
                {
                    if let Some(cb) = &self.callbacks.evaluate_rebirth_reason {
                        let cb = cb.clone();
                        task::spawn(cb(reason));
                    }
                }
            }
            Event::State { host_id, payload } => {
                if self
                    .app_state
                    .published_online_state
                    .load(std::sync::atomic::Ordering::SeqCst)
                    && host_id == self.app_state.host_id
                {
                    if let StatePayload::Offline { timestamp: _ } = payload {
                        let topic = StateTopic::new_host(&self.app_state.host_id);
                        let client = self.client.0.clone();
                        let timestamp = self.will_timestamp;
                        task::spawn(async move {
                            _ = client
                                .publish_state_message(
                                    topic,
                                    StatePayload::Online {
                                        timestamp,
                                    },
                                )
                                .await;
                        });
                    }
                }
            }
            Event::InvalidPublish {
                reason: _,
                topic: _,
                payload: _,
            } => (),
        }
    }

    async fn poll_until_offline(&mut self) -> bool {
        while self
            .app_state
            .online
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            if Event::Offline == self.eventloop.poll().await {
                self.handle_offline()
            }
        }
        true
    }

    async fn poll_until_offline_with_timeout(&mut self) {
        _ = timeout(Duration::from_secs(1), self.poll_until_offline()).await;
    }

    /// Run the Application
    ///
    /// Runs the Application until [AppClient::cancel()] is called
    pub async fn run(&mut self) {
        info!("App Started");
        self.update_last_will();
        loop {
            select! {
                event = self.eventloop.poll() => self.handle_event(event).await,
                Some(_) = self.shutdown_rx.recv() => break,
            }
        }
        self.poll_until_offline_with_timeout().await;
        self.handle_offline();
        info!("App Stopped");
    }
}
