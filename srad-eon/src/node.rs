use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use log::{debug, error, info, warn};
use srad_client::{DeviceMessage, DynClient, DynEventLoop, Event, LastWill, Message, MessageKind};
use srad_types::{
    constants::{self, NODE_CONTROL_REBIRTH},
    payload::{metric::Value, Payload},
    topic::{
        DeviceMessage as DeviceMessageType, DeviceTopic, NodeMessage as NodeMessageType, NodeTopic,
        QoS, StateTopic, Topic, TopicFilter,
    },
    utils::timestamp,
    MetricValue,
};
use tokio::{
    select,
    sync::{
        mpsc::{self, Sender, UnboundedSender},
        oneshot,
    },
    task,
    time::timeout,
};

use crate::{
    birth::BirthObjectType, device::DeviceMap, error::DeviceRegistrationError,
    metric_manager::manager::DynNodeMetricManager, BirthInitializer, BirthMetricDetails, BirthType,
    DeviceHandle, DeviceMetricManager, EoNBuilder, MessageMetrics, MetricPublisher, PublishError,
    PublishMetric,
};

pub(crate) struct EoNConfig
{
    node_rebirth_request_cooldown: Duration
}

pub(crate) struct EoNState {
    bdseq: AtomicU8,
    seq: AtomicU8,
    online: AtomicBool,
    birthed: AtomicBool,
    pub group_id: String,
    pub edge_node_id: String,
    pub ndata_topic: NodeTopic,
}

impl EoNState {
    pub(crate) fn get_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) as u64
    }

    pub(crate) fn is_online(&self) -> bool {
        self.online.load(Ordering::SeqCst)
    }

    pub(crate) fn birthed(&self) -> bool {
        self.birthed.load(Ordering::SeqCst)
    }

    fn birth_topic(&self) -> NodeTopic {
        NodeTopic::new(&self.group_id, NodeMessageType::NBirth, &self.edge_node_id)
    }

    fn sub_topics(&self) -> Vec<TopicFilter> {
        vec![
            TopicFilter::new_with_qos(
                Topic::NodeTopic(NodeTopic::new(
                    &self.group_id,
                    NodeMessageType::NCmd,
                    &self.edge_node_id,
                )),
                QoS::AtLeastOnce,
            ),
            TopicFilter::new_with_qos(
                Topic::DeviceTopic(DeviceTopic::new(
                    &self.group_id,
                    DeviceMessageType::DCmd,
                    &self.edge_node_id,
                    "+",
                )),
                QoS::AtLeastOnce,
            ),
            TopicFilter::new_with_qos(Topic::State(StateTopic::new()), QoS::AtLeastOnce),
        ]
    }
}

#[derive(Debug)]
struct EoNShutdown;

/// A handle for interacting with the Edge Node.
///
/// `NodeHandle` provides an interface for interacting with an edge node,
/// including device management, node lifecycle operations, and metric publishing.
#[derive(Clone)]
pub struct NodeHandle {
    node: Arc<Node>,
}

impl NodeHandle {
    /// Stop all operations, sending a death certificate and disconnect from the broker.
    ///
    /// This will cancel [EoN::run()]
    pub async fn cancel(&self) {
        info!("Edge node stopping. Node = {}", self.node.state.edge_node_id);
        let topic = NodeTopic::new(
            &self.node.state.group_id,
            NodeMessageType::NDeath,
            &self.node.state.edge_node_id,
        );
        let payload = self.node.generate_death_payload();
        match self
            .node
            .client
            .try_publish_node_message(topic, payload)
            .await
        {
            Ok(_) => (),
            Err(_) => debug!("Unable to publish node death certificate on exit"),
        };
        _ = self.node.stop_tx.send(EoNShutdown).await;
        _ = self.node.client.disconnect().await;
    }

    /// Manually trigger a rebirth for the node
    pub async fn rebirth(&self) {
        self.node.birth(BirthType::Rebirth).await;
    }

    /// Registers a new device with the node.
    ///
    /// Returns an error if:
    ///   - A device with the same name is already registered
    ///   - The device name is invalid
    pub fn register_device<S, M>(
        &self,
        name: S,
        dev_impl: M,
    ) -> Result<DeviceHandle, DeviceRegistrationError>
    where
        S: Into<String>,
        M: DeviceMetricManager + Send + Sync + 'static,
    {
        let name = name.into();
        if let Err(e) = srad_types::utils::validate_name(&name) {
            return Err(DeviceRegistrationError::InvalidName(e));
        }
        let handle = self.node.devices.lock().unwrap().add_device(
            &self.node.state.group_id,
            &self.node.state.edge_node_id,
            name,
            Box::new(dev_impl),
            self.node.state.clone(),
            self.node.client.clone(),
        )?;
        Ok(handle)
    }

    /// Unregister a device using it's handle.
    pub async fn unregister_device(&self, handle: DeviceHandle) {
        self.unregister_device_named(&handle.device.info.name).await;
    }

    /// Unregister a device using it's name.
    pub async fn unregister_device_named(&self, name: &String) {
        self.node.devices.lock().unwrap().remove_device(name)
    }

    fn check_publish_state(&self) -> Result<(), PublishError> {
        if !self.node.state.is_online() {
            return Err(PublishError::Offline);
        }
        if !self.node.state.birthed() {
            return Err(PublishError::UnBirthed);
        }
        Ok(())
    }

    fn publish_metrics_to_payload(&self, metrics: Vec<PublishMetric>) -> Payload {
        let timestamp = timestamp();
        let mut payload_metrics = Vec::with_capacity(metrics.len());
        for x in metrics.into_iter() {
            payload_metrics.push(x.into());
        }
        Payload {
            timestamp: Some(timestamp),
            metrics: payload_metrics,
            seq: Some(self.node.state.get_seq()),
            uuid: None,
            body: None,
        }
    }
}

impl MetricPublisher for NodeHandle {
    async fn try_publish_metrics_unsorted(
        &self,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), PublishError> {
        if metrics.is_empty() {
            return Err(PublishError::NoMetrics);
        }
        self.check_publish_state()?;
        match self
            .node
            .client
            .try_publish_node_message(
                self.node.state.ndata_topic.clone(),
                self.publish_metrics_to_payload(metrics),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => Err(PublishError::Offline),
        }
    }

    async fn publish_metrics_unsorted(
        &self,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), PublishError> {
        if metrics.is_empty() {
            return Err(PublishError::NoMetrics);
        }
        self.check_publish_state()?;
        match self
            .node
            .client
            .publish_node_message(
                self.node.state.ndata_topic.clone(),
                self.publish_metrics_to_payload(metrics),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => Err(PublishError::Offline),
        }
    }
}

struct Node {
    metric_manager: Box<DynNodeMetricManager>,
    client: Arc<DynClient>,
    devices: Mutex<DeviceMap>,
    state: Arc<EoNState>,
    config: Arc<EoNConfig>,
    stop_tx: Sender<EoNShutdown>,
    last_node_rebirth_request: AtomicU64,
    // The birth guard is to stop a user calling birth from the NodeHandle
    // while a death or birth is in progress due to an event from the Eventloop
    birth_guard: tokio::sync::Mutex<()>,
}

impl Node {
    fn generate_birth_payload(&self, bdseq: i64, seq: u64) -> Payload {
        let timestamp = timestamp();
        let mut birth_initializer = BirthInitializer::new(BirthObjectType::Node);
        birth_initializer
            .register_metric(
                BirthMetricDetails::new_with_initial_value(constants::BDSEQ, bdseq)
                    .use_alias(false),
            )
            .unwrap();
        birth_initializer
            .register_metric(
                BirthMetricDetails::new_with_initial_value(constants::NODE_CONTROL_REBIRTH, false)
                    .use_alias(false),
            )
            .unwrap();

        self.metric_manager.initialise_birth(&mut birth_initializer);
        let metrics = birth_initializer.finish();

        Payload {
            seq: Some(seq),
            timestamp: Some(timestamp),
            metrics,
            uuid: None,
            body: None,
        }
    }

    async fn node_birth(&self) {
        /* [tck-id-topics-nbirth-seq-num] The NBIRTH MUST include a sequence number in the payload and it MUST have a value of 0. */
        self.state.birthed.store(false, Ordering::SeqCst);
        self.state.seq.store(0, Ordering::SeqCst);
        let bdseq = self.state.bdseq.load(Ordering::SeqCst) as i64;

        let payload = self.generate_birth_payload(bdseq, 0);
        let topic = self.state.birth_topic();
        self.state.seq.store(1, Ordering::SeqCst);
        match self.client.publish_node_message(topic, payload).await {
            Ok(_) => self.state.birthed.store(true, Ordering::SeqCst),
            Err(_) => error!("Publishing birth message failed"),
        }
    }

    async fn birth(&self, birth_type: BirthType) {
        let guard = self.birth_guard.lock().await;
        info!("Birthing Node. Node = {}, Type = {:?}", self.state.edge_node_id, birth_type);
        self.node_birth().await;
        self.devices.lock().unwrap().birth_devices(birth_type);
        drop(guard)
    }

    async fn death(&self) {
        self.state.birthed.store(false, Ordering::SeqCst);
        self.state.bdseq.fetch_add(1, Ordering::SeqCst);
        self.devices.lock().unwrap().on_death();
    }

    async fn on_online(&self) {
        if self.state.online.swap(true, Ordering::SeqCst) {
            return;
        }
        info!("Edge node online. Node = {}", self.state.edge_node_id);
        let sub_topics = self.state.sub_topics();

        if self.client.subscribe_many(sub_topics).await.is_ok() {
            self.birth(BirthType::Birth).await
        };
    }

    async fn on_offline(&self, will_sender: oneshot::Sender<LastWill>) {
        if !self.state.online.swap(false, Ordering::SeqCst) {
            return;
        }
        info!("Edge node offline. Node = {}", self.state.edge_node_id);
        self.death().await;
        let new_lastwill = self.create_last_will();
        _ = will_sender.send(new_lastwill);
    }

    async fn on_sparkplug_message(&self, message: Message, handle: NodeHandle) {
        let payload = message.payload;
        let message_kind = message.kind;

        if message_kind == MessageKind::Cmd {
            let mut rebirth = false;
            for x in &payload.metrics {
                if x.alias.is_some() {
                    continue;
                }

                let metric_name = match &x.name {
                    Some(name) => name,
                    None => continue,
                };

                if metric_name != NODE_CONTROL_REBIRTH {
                    continue;
                }

                rebirth = match &x.value {
                    Some(Value::BooleanValue(val)) => *val,
                    _ => false,
                };

                if !rebirth {
                    warn!("Received invalid CMD Rebirth metric - ignoring request")
                }
            }

            let message_metrics: MessageMetrics = match payload.try_into() {
                Ok(metrics) => metrics,
                Err(_) => {
                    warn!("Received invalid CMD payload - ignoring request");
                    return;
                }
            };

            self.metric_manager.on_ncmd(handle, message_metrics).await;
            if rebirth {
                let now = timestamp(); 
                let time_since_last = now - self.last_node_rebirth_request.load(Ordering::Relaxed);
                if time_since_last < self.config.node_rebirth_request_cooldown.as_millis() as u64 {
                    info!("Got Rebirth CMD but cooldown time not expired. Ignoring");
                    return;
                } 
                info!("Got Rebirth CMD - Rebirthing Node");
                self.birth(BirthType::Rebirth).await;
                self.last_node_rebirth_request.store(now, Ordering::Relaxed);
            }
        }
    }

    fn generate_death_payload(&self) -> Payload {
        let mut metric = srad_types::payload::Metric::new();
        metric
            .set_name(constants::BDSEQ.to_string())
            .set_value(MetricValue::from(self.state.bdseq.load(Ordering::SeqCst) as i64).into());
        Payload {
            seq: None,
            metrics: vec![metric],
            uuid: None,
            timestamp: None,
            body: None,
        }
    }

    fn create_last_will(&self) -> LastWill {
        LastWill::new_node(
            &self.state.group_id,
            &self.state.edge_node_id,
            self.generate_death_payload(),
        )
    }
}

enum EoNNodeMessage {
    Stopped,
    Online,
    SparkplugMessage(Message),
    Offline(oneshot::Sender<LastWill>),
}

/// Structure that represents a Sparkplug Edge Node instance.
///
/// See [EoNBuilder] on how to create an [EoN] instance.
pub struct EoN {
    eventloop: Box<DynEventLoop>,
    node: Arc<Node>,
    stop_rx: mpsc::Receiver<EoNShutdown>,
}

impl EoN {
    pub(crate) fn new_from_builder(builder: EoNBuilder) -> Result<(Self, NodeHandle), String> {
        let group_id = builder
            .group_id
            .ok_or("group id must be provided".to_string())?;
        let node_id = builder
            .node_id
            .ok_or("node id must be provided".to_string())?;
        srad_types::utils::validate_name(&group_id)?;
        srad_types::utils::validate_name(&node_id)?;

        let metric_manager = builder.metric_manager;
        let (eventloop, client) = builder.eventloop_client;
        let (stop_tx, stop_rx) = mpsc::channel(1);

        let state = Arc::new(EoNState {
            seq: AtomicU8::new(0),
            bdseq: AtomicU8::new(0),
            online: AtomicBool::new(false),
            birthed: AtomicBool::new(false),
            ndata_topic: NodeTopic::new(&group_id, NodeMessageType::NData, &node_id),
            group_id,
            edge_node_id: node_id,
        });

        let node = Arc::new(Node {
            metric_manager,
            client: client.clone(),
            devices: Mutex::new(DeviceMap::new()),
            state,
            stop_tx,
            birth_guard: tokio::sync::Mutex::new(()),
            config: Arc::new(
                EoNConfig { node_rebirth_request_cooldown: builder.node_rebirth_request_cooldown }
            ),
            last_node_rebirth_request: AtomicU64::new(0)
        });

        let eon = Self {
            node,
            eventloop,
            stop_rx,
        };
        let handle = NodeHandle {
            node: eon.node.clone(),
        };
        eon.node.metric_manager.init(&handle);

        Ok((eon, handle))
    }

    fn update_last_will(&mut self, lastwill: LastWill) {
        self.eventloop.set_last_will(lastwill);
    }

    fn on_online(&mut self, node_tx: &UnboundedSender<EoNNodeMessage>) {
        _ = node_tx.send(EoNNodeMessage::Online);
    }

    async fn on_offline(&mut self, node_tx: &UnboundedSender<EoNNodeMessage>) {
        let (lastwill_tx, lastwill_rx) = oneshot::channel();
        _ = node_tx.send(EoNNodeMessage::Offline(lastwill_tx));
        if let Ok(will) = lastwill_rx.await {
            self.update_last_will(will)
        }
    }

    fn on_node_message(&mut self, message: Message, node_tx: &UnboundedSender<EoNNodeMessage>) {
        _ = node_tx
            .send(EoNNodeMessage::SparkplugMessage(message))
    }

    fn on_device_message(&mut self, message: DeviceMessage) {
        let devices = self.node.devices.lock().unwrap();
        devices.handle_device_message(message);
    }

    async fn handle_event(&mut self, event: Event, node_tx: &UnboundedSender<EoNNodeMessage>) {
        match event {
            Event::Online => self.on_online(node_tx),
            Event::Offline => self.on_offline(node_tx).await,
            Event::Node(node_message) => self.on_node_message(node_message.message, node_tx),
            Event::Device(device_message) => self.on_device_message(device_message),
            Event::State {
                host_id: _,
                payload: _,
            } => (),
            Event::InvalidPublish {
                reason: _,
                topic: _,
                payload: _,
            } => (),
        }
    }

    async fn poll_until_offline(&mut self, node_tx: &UnboundedSender<EoNNodeMessage>) -> bool {
        while self.node.state.is_online() {
            if Event::Offline == self.eventloop.poll().await {
                self.on_offline(node_tx).await;
                break;
            }
        }
        true
    }

    /// Run the Edge Node
    ///
    /// Runs the Edge Node until [NodeHandle::cancel()] is called
    pub async fn run(&mut self) {
        info!("Edge node running. Node = {}", self.node.state.edge_node_id);

        let (node_tx, mut node_rx) = mpsc::unbounded_channel();

        self.update_last_will(self.node.create_last_will());

        let node = self.node.clone();
        task::spawn(async move {


            while let Some(msg) = node_rx.recv().await {
                match msg {
                    EoNNodeMessage::Online => node.on_online().await,
                    EoNNodeMessage::Offline(sender) => node.on_offline(sender).await,
                    EoNNodeMessage::SparkplugMessage(message) => {
                        let handle = NodeHandle { node: node.clone() };
                        node.on_sparkplug_message(message, handle).await
                    }
                    EoNNodeMessage::Stopped => break,
                }
            }
        });

        loop {
            select! {
              event = self.eventloop.poll() => self.handle_event(event, &node_tx).await,
              Some(_) = self.stop_rx.recv() => break,
            }
        }

        if timeout(Duration::from_secs(1), self.poll_until_offline(&node_tx)).await.is_err() {
            self.on_offline(&node_tx).await;
        }

        _ = node_tx.send(EoNNodeMessage::Stopped);
        info!("Edge node stopped. Node = {}", self.node.state.edge_node_id);
    }
}
