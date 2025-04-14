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
    payload::{Metric, Payload},
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

    fn is_dead(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl PartialEq for BirthToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
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
    pub fn new_device_cmd(group_id: &str, node_id: &str, device_id: &str) -> Self {
        PublishTopic(PublishTopicKind::DeviceTopic(DeviceTopic::new(
            group_id,
            srad_types::topic::DeviceMessage::DCmd,
            node_id,
            device_id,
        )))
    }

    /// Create a new `PublishTopic` which will publish on a specified node's CMD topic
    pub fn new_node_cmd(group_id: &str, node_id: &str) -> Self {
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
    pub async fn publish_node_rebirth(&self, group_id: &str, node_id: &str) -> Result<(), ()> {
        let topic = PublishTopic::new_node_cmd(group_id, node_id);
        let rebirth_cmd = PublishMetric::new(MetricId::Name(NODE_CONTROL_REBIRTH.into()), true);
        self.publish_metrics(topic, vec![rebirth_cmd]).await
    }

    fn metrics_to_payload(metrics: Vec<PublishMetric>) -> Payload {
        let mut payload_metrics = Vec::with_capacity(metrics.len());
        for x in metrics.into_iter() {
            payload_metrics.push(x.into());
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

struct AppState {
    host_id: String,
    online: AtomicBool,
    published_online_state: AtomicBool,
}

/// Structure that represents a Sparkplug Application instance
pub struct App {
    online: bool,
    state: Arc<AppState>,
    will_timestamp: u64,
    subscription_config: SubscriptionConfig,
    client: AppClient,
    eventloop: Box<DynEventLoop>,
    shutdown_rx: Receiver<Shutdown>,
    nodes: HashMap<NodeIdentifier, NodeState>
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
            online: false,
            state: app_state,
            will_timestamp: 0,
            client: client.clone(),
            eventloop: Box::new(eventloop),
            subscription_config,
            shutdown_rx: rx,
        };
        (app, client)
    }

    fn update_last_will(&mut self) {
        self.will_timestamp = timestamp();
        self.eventloop.set_last_will(srad_client::LastWill::new_app(
            &self.state.host_id,
            self.will_timestamp,
        ));
    }

    fn handle_online(&mut self) -> Option<AppEvent> {
        if self.online == true { return None }
        info!("App Online");
        self.online = true;
        let client = self.client.0.clone();
        let state_topic = StateTopic::new_host(&self.state.host_id);
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
        Some(AppEvent::Online)
    }

    fn handle_offline(&mut self) -> Option<AppEvent> {
        if !self.online { return None }
        info!("App Offline");
        self.online = false;
        self.state
            .published_online_state
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.update_last_will();
        Some(AppEvent::Offline)
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

    fn handle_node_birth(&mut self, id: NodeIdentifier, payload: Payload) -> AppEvent
    {
        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                warn!(
                    "Message did not contain a seq number - discarding. node = {:?}",
                    id
                );
                return AppEvent::RebirthReason(RebirthReasonDetails::new(
                    id,
                    RebirthReason::MalformedPayload,
                ))
            }
        };

        if seq != 0 {
            debug!("Birth payload sequence value is not zero - Discarding. node = {:?}", id);
            return AppEvent::RebirthReason(RebirthReasonDetails::new(
                id,
                RebirthReason::MalformedPayload,
            ))
        }

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => {
                warn!(
                    "Message did not contain a timestamp - discarding. node = {:?}",
                    id
                );
                return AppEvent::RebirthReason(RebirthReasonDetails::new(
                    id,
                    RebirthReason::MalformedPayload,
                ))
            }
        };

        let bdseq = match Self::bdseq_from_payload_metrics(&payload.metrics) {
            Ok(bdseq) => bdseq,
            Err(_) => {
                warn!("Birth Message contained an invalid bdseq metric - discarding payload. node = {:?}", id);
                return AppEvent::RebirthReason(RebirthReasonDetails::new(
                    id,
                    RebirthReason::MalformedPayload,
                ));
            }
        };

        match self.nodes.get_mut(&id) {
            Some(node) => {
                node.reset_seq();
                node.bdseq = bdseq;
            },
            None => {
                let node = NodeState::new(seq, bdseq);
                self.nodes.insert(id.clone(), node);
            },
        };

        let metric_details =
            match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                Ok(details) => details,
                Err(e) => {
                    warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
                    return AppEvent::RebirthReason(RebirthReasonDetails::new(
                        id,
                        RebirthReason::MalformedPayload,
                    ));
                }
            };
        
        AppEvent::NBirth
    }

    fn handle_node_message(&mut self, message: NodeMessage) -> Option<AppEvent> {

        match message.message.kind {
            MessageKind::Birth => todo!(),
            MessageKind::Death => todo!(),
            MessageKind::Cmd => None,  
            MessageKind::Data => todo!(),
            MessageKind::Other(_) => None,
        }
    }

    fn handle_event(&mut self, event: Event) -> Option<AppEvent>
    {
        match event {
            Event::Offline => self.handle_offline(),
            Event::Online => self.handle_online(),
            Event::Node(node_message) => todo!(),
            Event::Device(device_message) => todo!(),
            Event::State { host_id, payload } => todo!(),
            Event::InvalidPublish { reason, topic, payload } => todo!(),
        }
    }

    pub async fn poll(&mut self) -> AppEvent {

        loop {

            select! {
                event = self.eventloop.poll() => {
                    if let Some(app_event) = self.handle_event(event) {
                        return app_event;
                    }
                }

            }

        }
        
    }
  
}

pub enum AppEvent {
    Online,
    Offline,
    NBirth,
    NDeath,
    NData,
    RebirthReason(RebirthReasonDetails),
    Disconnected
}

