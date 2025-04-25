use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use log::{debug, error, info, warn};
use srad_client::{
    Client, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, MessageKind, NodeMessage,
    StatePayload,
};
use srad_types::{
    constants::{BDSEQ, NODE_CONTROL_REBIRTH},
    payload::{self, Metric, Payload},
    topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter},
    utils::{self, timestamp},
    MetricId, MetricValue,
};

use crate::{
    config::SubscriptionConfig,
    events::{self, DData, DDeath, NBirth, NData, NDeath},
    metrics::{
        get_metric_birth_details_from_birth_metrics,
        get_metric_id_and_details_from_payload_metrics, PublishMetric,
    },
    NodeIdentifier,
};

use tokio::{
    select,
    sync::mpsc::{self, Receiver},
    task,
    time::timeout,
};

struct DeviceState {
    birthed: bool,
}

struct NodeState {
    seq: u8,
    bdseq: u8,
    birthed: bool,
    devices: HashMap<String, DeviceState>,
}

impl NodeState {
    fn new(seq: u8, bdseq: u8) -> Self {
        Self {
            seq,
            bdseq,
            birthed: true,
            devices: HashMap::new(),
        }
    }

    fn reset_seq(&mut self) {
        self.seq = 0
    }

    fn get_device(&mut self, device_name: &String) -> Option<&mut DeviceState> {
        self.devices.get_mut(device_name)
    }

    fn add_device(&mut self, name: String) {
        self.devices.insert(name, DeviceState { birthed: true });
    }
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
#[derive(Debug)]
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

/// The client to interact with the [AppEventLoop] and Sparkplug namespace from.
#[derive(Clone)]
pub struct AppClient {
    client: Arc<DynClient>,
    sender: mpsc::Sender<Shutdown>,
    state: Arc<AppState>,
}

impl AppClient {
    /// Stop all operations, sending a death certificate (application offline message) and disconnect from the broker.
    ///
    /// This will produce an [AppEvent::Cancelled] event on the [AppEventLoop] after the application has gracefully disconnected
    pub async fn cancel(&self) {
        info!("App Stopping");
        let topic = StateTopic::new_host(&self.state.host_id);
        match self
            .client
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
        _ = self.sender.send(Shutdown).await;
        _ = self.client.disconnect().await;
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
                self.client.try_publish_node_message(topic, payload).await
            }
            PublishTopicKind::DeviceTopic(topic) => {
                self.client.try_publish_device_message(topic, payload).await
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
            PublishTopicKind::NodeTopic(topic) => {
                self.client.publish_node_message(topic, payload).await
            }
            PublishTopicKind::DeviceTopic(topic) => {
                self.client.publish_device_message(topic, payload).await
            }
        }
    }
}

struct AppState {
    host_id: String,
    published_online_state: AtomicBool,
}

/// A Sparkplug Application EventLoop
pub struct AppEventLoop {
    online: bool,
    state: Arc<AppState>,
    will_timestamp: u64,
    subscription_config: SubscriptionConfig,
    client: AppClient,
    eventloop: Box<DynEventLoop>,
    shutdown_rx: Receiver<Shutdown>,
    nodes: HashMap<NodeIdentifier, NodeState>,
}

impl AppEventLoop {
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
            published_online_state: AtomicBool::new(false),
        });
        let client = AppClient {
            client: Arc::new(client),
            sender: tx,
            state: app_state.clone(),
        };
        let mut app = Self {
            online: false,
            state: app_state,
            will_timestamp: 0,
            client: client.clone(),
            eventloop: Box::new(eventloop),
            subscription_config,
            shutdown_rx: rx,
            nodes: HashMap::new(),
        };
        app.update_last_will();
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
        if self.online {
            return None;
        }
        info!("App Online");
        self.online = true;
        let client = self.client.client.clone();
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
        let app_state = self.client.state.clone();
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
        if !self.online {
            return None;
        }
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

    fn handle_node_birth(&mut self, id: NodeIdentifier, payload: Payload) -> AppEvent {
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
                ));
            }
        };

        if seq != 0 {
            debug!(
                "Birth payload sequence value is not zero - Discarding. node = {:?}",
                id
            );
            return AppEvent::RebirthReason(RebirthReasonDetails::new(
                id,
                RebirthReason::MalformedPayload,
            ));
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
                ));
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

        let metrics_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
            Ok(details) => details,
            Err(e) => {
                warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
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
                node.birthed = true;
            }
            None => {
                let node = NodeState::new(seq, bdseq);
                self.nodes.insert(id.clone(), node);
            }
        };

        AppEvent::NBirth(NBirth {
            id,
            timestamp,
            metrics_details,
        })
    }

    fn handle_node_death(&mut self, id: NodeIdentifier, payload: Payload) -> Option<AppEvent> {
        let bdseq = match Self::bdseq_from_payload_metrics(&payload.metrics) {
            Ok(bdseq) => bdseq,
            Err(_) => {
                warn!("Birth Message contained an invalid bdseq metric - discarding payload. node = {:?}", id);
                return Some(AppEvent::RebirthReason(RebirthReasonDetails::new(
                    id,
                    RebirthReason::MalformedPayload,
                )));
            }
        };

        let node = self.nodes.get_mut(&id)?;
        /* Duplicate death */
        if !node.birthed {
            debug!(
                "Death message already received for bdseq {} - ignoring. node = {:?}",
                bdseq, id
            );
            return None;
        }

        if bdseq != node.bdseq {
            debug!(
                "Death bdseq did not match current known birth bdseq - ignoring. node = {:?}",
                id
            );
            return None;
        }
        node.birthed = false;

        Some(AppEvent::NDeath(NDeath { id }))
    }

    fn handle_sequenced_message(
        &mut self,
        node_id: NodeIdentifier,
        payload: &Payload,
    ) -> Result<(&mut NodeState, NodeIdentifier, u64), RebirthReasonDetails> {
        let _ = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                warn!(
                    "Message from node did not contain a seq number - discarding. node = {:?}",
                    node_id
                );
                return Err(RebirthReasonDetails::new(
                    node_id,
                    RebirthReason::MalformedPayload,
                ));
            }
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => {
                warn!(
                    "Message did not contain a timestamp - discarding. node = {:?}",
                    node_id
                );
                return Err(RebirthReasonDetails::new(
                    node_id,
                    RebirthReason::MalformedPayload,
                ));
            }
        };

        let node = match self.nodes.get_mut(&node_id) {
            Some(node) => node,
            None => {
                return Err(RebirthReasonDetails::new(
                    node_id,
                    RebirthReason::UnknownNode,
                ))
            }
        };

        Ok((node, node_id, timestamp))
    }

    fn handle_node_data(&mut self, id: NodeIdentifier, payload: Payload) -> AppEvent {
        let (_, id, timestamp) = match self.handle_sequenced_message(id, &payload) {
            Ok(val) => val,
            Err(rr) => return AppEvent::RebirthReason(rr),
        };

        let metrics_details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
        {
            Ok(details) => details,
            Err(e) => {
                warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
                return AppEvent::RebirthReason(RebirthReasonDetails::new(
                    id,
                    RebirthReason::MalformedPayload,
                ));
            }
        };

        AppEvent::NData(NData {
            id,
            timestamp,
            metrics_details,
        })
    }

    fn handle_node_message(&mut self, message: NodeMessage) -> Option<AppEvent> {
        let id = NodeIdentifier {
            group: message.group_id,
            node: message.node_id,
        };
        match message.message.kind {
            MessageKind::Birth => Some(self.handle_node_birth(id, message.message.payload)),
            MessageKind::Death => self.handle_node_death(id, message.message.payload),
            MessageKind::Cmd => None,
            MessageKind::Data => Some(self.handle_node_data(id, message.message.payload)),
            MessageKind::Other(_) => None,
        }
    }

    fn handle_device_birth(
        node: &mut NodeState,
        node_id: NodeIdentifier,
        device_name: String,
        timestamp: u64,
        metrics: Vec<payload::Metric>,
    ) -> Option<AppEvent> {
        let metrics_details = match get_metric_birth_details_from_birth_metrics(metrics) {
            Ok(details) => details,
            Err(e) => {
                warn!(
                    "Message payload was invalid - {:?}. node = {:?}, device = {}",
                    e, node_id, device_name
                );
                return Some(AppEvent::RebirthReason(
                    RebirthReasonDetails::new(node_id, RebirthReason::MalformedPayload)
                        .with_device(device_name),
                ));
            }
        };

        match node.get_device(&device_name) {
            Some(dev) => {
                if dev.birthed {
                    return None;
                }
                dev.birthed = true;
            }
            None => node.add_device(device_name.clone()),
        };

        Some(AppEvent::DBirth(events::DBirth {
            node_id,
            device_name,
            timestamp,
            metrics_details,
        }))
    }

    fn handle_device_death(
        node: &mut NodeState,
        node_id: NodeIdentifier,
        device_name: String,
        timestamp: u64,
    ) -> Option<AppEvent> {
        let device = node.get_device(&device_name)?;
        if !device.birthed {
            return None;
        }
        device.birthed = true;
        Some(AppEvent::DDeath(DDeath {
            node_id,
            device_name,
            timestamp,
        }))
    }

    fn handle_device_data(
        node: &mut NodeState,
        node_id: NodeIdentifier,
        device_name: String,
        timestamp: u64,
        metrics: Vec<payload::Metric>,
    ) -> Option<AppEvent> {
        match node.get_device(&device_name) {
            Some(dev) => {
                if dev.birthed {
                    return None;
                }
                dev.birthed = true;
            }
            None => node.add_device(device_name.clone()),
        };

        let metrics_details = match get_metric_id_and_details_from_payload_metrics(metrics) {
            Ok(details) => details,
            Err(e) => {
                warn!(
                    "Message payload was invalid - {:?}. node = {:?}, device = {}",
                    e, node_id, device_name
                );
                return Some(AppEvent::RebirthReason(
                    RebirthReasonDetails::new(node_id, RebirthReason::MalformedPayload)
                        .with_device(device_name),
                ));
            }
        };

        Some(AppEvent::DData(DData {
            node_id,
            device_name,
            timestamp,
            metrics_details,
        }))
    }

    fn handle_device_message(&mut self, message: DeviceMessage) -> Option<AppEvent> {
        let device_name = message.device_id;
        let id = NodeIdentifier {
            group: message.group_id,
            node: message.node_id,
        };
        let message_kind = message.message.kind;
        let payload = message.message.payload;

        let (node, id, timestamp) = match self.handle_sequenced_message(id, &payload) {
            Ok(out) => out,
            Err(rr) => return Some(AppEvent::RebirthReason(rr.with_device(device_name))),
        };

        match message_kind {
            MessageKind::Birth => {
                Self::handle_device_birth(node, id, device_name, timestamp, payload.metrics)
            }
            MessageKind::Death => Self::handle_device_death(node, id, device_name, timestamp),
            MessageKind::Data => {
                Self::handle_device_data(node, id, device_name, timestamp, payload.metrics)
            }
            MessageKind::Cmd => None,
            MessageKind::Other(_) => None,
        }
    }

    fn handle_event(&mut self, event: Event) -> Option<AppEvent> {
        match event {
            Event::Offline => self.handle_offline(),
            Event::Online => self.handle_online(),
            Event::Node(message) => self.handle_node_message(message),
            Event::Device(message) => self.handle_device_message(message),
            Event::State { host_id, payload } => {
                if self
                    .client
                    .state
                    .published_online_state
                    .load(std::sync::atomic::Ordering::SeqCst)
                    && host_id == self.client.state.host_id
                {
                    if let StatePayload::Offline { timestamp: _ } = payload {
                        let topic = StateTopic::new_host(&self.client.state.host_id);
                        let client = self.client.client.clone();
                        let timestamp = self.will_timestamp;
                        task::spawn(async move {
                            _ = client
                                .publish_state_message(topic, StatePayload::Online { timestamp })
                                .await;
                        });
                    }
                }
                None
            }
            _ => None,
        }
    }

    async fn poll_until_offline(&mut self) {
        while self.online {
            if Event::Offline == self.eventloop.poll().await {
                self.handle_offline();
            }
        }
    }

    async fn poll_until_offline_with_timeout(&mut self) {
        _ = timeout(Duration::from_secs(1), self.poll_until_offline()).await;
    }

    /// Progress the App. Continuing to poll will reconnect if the application if there is a disconnection.
    /// **NOTE** Don't block this while iterating. Additionally, using the `AppClient` directly inside the loop may block progress.
    pub async fn poll(&mut self) -> AppEvent {
        loop {
            select! {
                event = self.eventloop.poll() => {
                    if let Some(app_event) = self.handle_event(event) {
                        return app_event
                    }
                }
                Some(_) = self.shutdown_rx.recv() => {
                    self.poll_until_offline_with_timeout().await;
                    return AppEvent::Cancelled
                },
            }
        }
    }
}

/// An event produced by the [AppEventLoop]
#[derive(Debug)]
pub enum AppEvent {
    /// Application is online and connected to the broker
    Online,
    /// Application is offline and has disconnected from the broker
    Offline,
    NBirth(events::NBirth),
    NDeath(events::NDeath),
    NData(events::NData),
    DBirth(events::DBirth),
    DDeath(events::DDeath),
    DData(events::DData),
    /// Application has detected a state where the application may wish to issue a Rebirth request
    RebirthReason(RebirthReasonDetails),
    Cancelled,
}
