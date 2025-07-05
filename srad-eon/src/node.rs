use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
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
    sync::{mpsc, oneshot},
    time::timeout,
};

use crate::{
    birth::BirthObjectType, device::DeviceMap, error::DeviceRegistrationError,
    metric_manager::manager::DynNodeMetricManager, BirthInitializer, BirthMetricDetails, BirthType,
    DeviceHandle, DeviceMetricManager, EoNBuilder, MessageMetrics, MetricPublisher, PublishError,
    PublishMetric,
};

pub(crate) struct EoNConfig {
    node_rebirth_request_cooldown: Duration,
}

pub(crate) struct EoNState {
    bdseq: AtomicU8,
    seq: AtomicU8,
    online: AtomicBool,
    running: AtomicBool,
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

    fn generate_death_payload(&self) -> Payload {
        let mut metric = srad_types::payload::Metric::new();
        metric
            .set_name(constants::BDSEQ.to_string())
            .set_value(MetricValue::from(self.bdseq.load(Ordering::SeqCst) as i64).into());
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
            &self.group_id,
            &self.edge_node_id,
            self.generate_death_payload(),
        )
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
    state: Arc<EoNState>,
    client: Arc<DynClient>,
    devices: Arc<Mutex<DeviceMap>>,
    stop_tx: mpsc::Sender<EoNShutdown>,
    rebirth_tx: mpsc::Sender<()>,
}

impl NodeHandle {
    /// Stop all operations, sending a death certificate and disconnect from the broker.
    ///
    /// This will cancel [EoN::run()]
    pub async fn cancel(&self) {
        if !self.state.running.load(Ordering::SeqCst) {
            return;
        }
        info!("Edge node stopping. Node = {}", self.state.edge_node_id);
        let topic = NodeTopic::new(
            &self.state.group_id,
            NodeMessageType::NDeath,
            &self.state.edge_node_id,
        );
        let payload = self.state.generate_death_payload();
        match self.client.try_publish_node_message(topic, payload).await {
            Ok(_) => (),
            Err(_) => debug!("Unable to publish node death certificate on exit"),
        };
        _ = self.stop_tx.send(EoNShutdown).await;
        _ = self.client.disconnect().await;
    }

    /// Manually trigger a rebirth for the node
    pub fn rebirth(&self) {
        //try send, if the channel (size 1) is full then a rebirth will be executed anyways
        _ = self.rebirth_tx.try_send(());
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
        let handle = self.devices.lock().unwrap().add_device(
            &self.state.group_id,
            &self.state.edge_node_id,
            name,
            Box::new(dev_impl),
            self.state.clone(),
            self.client.clone(),
        )?;
        Ok(handle)
    }

    /// Unregister a device using it's handle.
    pub async fn unregister_device(&self, handle: DeviceHandle) {
        self.unregister_device_named(&handle.state.name).await;
    }

    /// Unregister a device using it's name.
    pub async fn unregister_device_named(&self, name: &String) {
        self.devices.lock().unwrap().remove_device(name)
    }

    fn check_publish_state(&self) -> Result<(), PublishError> {
        if !self.state.is_online() {
            return Err(PublishError::Offline);
        }
        if !self.state.birthed() {
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
            seq: Some(self.state.get_seq()),
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
            .client
            .try_publish_node_message(
                self.state.ndata_topic.clone(),
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
            .client
            .publish_node_message(
                self.state.ndata_topic.clone(),
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
    devices: Arc<Mutex<DeviceMap>>,
    state: Arc<EoNState>,
    config: Arc<EoNConfig>,
    stop_tx: mpsc::Sender<EoNShutdown>,
    last_node_rebirth_request: Duration,

    rebirth_request_tx: mpsc::Sender<()>,

    node_message_rx: mpsc::UnboundedReceiver<Message>,
    client_state_rx: mpsc::Receiver<ClientStateMessage>,
    rebirth_request_rx: mpsc::Receiver<()>,
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

    async fn birth(&self) {
        info!("Birthing Node. Node = {}", self.state.edge_node_id);
        self.node_birth().await;
        self.devices.lock().unwrap().birth_devices(BirthType::Birth);
    }

    async fn rebirth(&self) {
        if !self.state.birthed.load(Ordering::SeqCst) {
            return;
        }
        info!("Re-Birthing Node. Node = {}", self.state.edge_node_id);
        self.node_birth().await;
        self.devices
            .lock()
            .unwrap()
            .birth_devices(BirthType::Rebirth);
    }

    fn death(&self) {
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
            self.birth().await
        };
    }

    fn on_offline(&self, will_sender: oneshot::Sender<LastWill>) {
        if !self.state.online.swap(false, Ordering::SeqCst) {
            return;
        }
        info!("Edge node offline. Node = {}", self.state.edge_node_id);
        self.death();
        let new_lastwill = self.state.create_last_will();
        _ = will_sender.send(new_lastwill);
    }

    async fn on_sparkplug_message(&mut self, message: Message, handle: NodeHandle) {
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
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                let time_since_last = now - self.last_node_rebirth_request;
                if time_since_last < self.config.node_rebirth_request_cooldown {
                    info!("Got Rebirth CMD but cooldown time not expired. Ignoring");
                    return;
                }
                info!("Got Rebirth CMD - Rebirthing Node");
                self.rebirth().await;
                self.last_node_rebirth_request = now;
            }
        }
    }

    fn create_node_handle(&self) -> NodeHandle {
        NodeHandle {
            state: self.state.clone(),
            client: self.client.clone(),
            devices: self.devices.clone(),
            stop_tx: self.stop_tx.clone(),
            rebirth_tx: self.rebirth_request_tx.clone(),
        }
    }

    async fn run(mut self) {
        loop {
            select! {
                biased;
                maybe_state_update = self.client_state_rx.recv() => match maybe_state_update {
                    Some (state_update) => match state_update {
                        ClientStateMessage::Online => self.on_online().await,
                        ClientStateMessage::Offline(sender) => self.on_offline(sender),
                        ClientStateMessage::Stopped => break
                    },
                    None => break, //EoN has been dropped
                },
                Some(_) = self.rebirth_request_rx.recv() => self.rebirth().await,
                maybe_message = self.node_message_rx.recv() => match maybe_message {
                    Some(message) => self.on_sparkplug_message(message, self.create_node_handle()).await,
                    None => break, //EoN has been dropped
                },
            }
        }
    }
}

enum ClientStateMessage {
    Stopped,
    Online,
    Offline(oneshot::Sender<LastWill>),
}

/// Structure that represents a Sparkplug Edge Node instance.
///
/// See [EoNBuilder] on how to create an [EoN] instance.
pub struct EoN {
    eventloop: Box<DynEventLoop>,
    stop_rx: mpsc::Receiver<EoNShutdown>,
    node_message_tx: mpsc::UnboundedSender<Message>,
    client_state_tx: mpsc::Sender<ClientStateMessage>,
    state: Arc<EoNState>,
    devices: Arc<Mutex<DeviceMap>>,
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
            running: AtomicBool::new(false),
            online: AtomicBool::new(false),
            birthed: AtomicBool::new(false),
            ndata_topic: NodeTopic::new(&group_id, NodeMessageType::NData, &node_id),
            group_id,
            edge_node_id: node_id,
        });

        let devices = Arc::new(Mutex::new(DeviceMap::new()));

        let (node_message_tx, node_message_rx) = mpsc::unbounded_channel();
        let (rebirth_request_tx, rebirth_request_rx) = mpsc::channel(1);
        let (client_state_tx, client_state_rx) = mpsc::channel(1);

        let node = Node {
            metric_manager,
            client: client.clone(),
            state: state.clone(),
            devices: devices.clone(),
            stop_tx,
            config: Arc::new(EoNConfig {
                node_rebirth_request_cooldown: builder.node_rebirth_request_cooldown,
            }),
            last_node_rebirth_request: Duration::new(0, 0),
            node_message_rx,
            rebirth_request_rx,
            rebirth_request_tx,
            client_state_rx,
        };

        let eon = Self {
            eventloop,
            stop_rx,
            node_message_tx,
            client_state_tx,
            state,
            devices,
        };

        let handle = node.create_node_handle();

        node.metric_manager.init(&handle);

        tokio::spawn(async move { node.run().await });

        Ok((eon, handle))
    }

    fn update_last_will(&mut self, lastwill: LastWill) {
        self.eventloop.set_last_will(lastwill);
    }

    async fn on_online(&mut self) {
        _ = self.client_state_tx.send(ClientStateMessage::Online).await;
    }

    async fn on_offline(&mut self) {
        let (lastwill_tx, lastwill_rx) = oneshot::channel();
        _ = self
            .client_state_tx
            .send(ClientStateMessage::Offline(lastwill_tx))
            .await;
        if let Ok(will) = lastwill_rx.await {
            self.update_last_will(will)
        }
    }

    fn on_node_message(&mut self, message: Message) {
        _ = self.node_message_tx.send(message)
    }

    fn on_device_message(&mut self, message: DeviceMessage) {
        self.devices.lock().unwrap().handle_device_message(message);
    }

    async fn handle_event(&mut self, event: Event) {
        match event {
            Event::Online => self.on_online().await,
            Event::Offline => self.on_offline().await,
            Event::Node(node_message) => self.on_node_message(node_message.message),
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

    async fn poll_until_offline(&mut self) -> bool {
        while self.state.is_online() {
            if Event::Offline == self.eventloop.poll().await {
                self.on_offline().await;
                break;
            }
        }
        true
    }

    /// Run the Edge Node
    ///
    /// Runs the Edge Node until [NodeHandle::cancel()] is called
    pub async fn run(mut self) {
        info!("Edge node running. Node = {}", self.state.edge_node_id);
        self.state.running.store(true, Ordering::SeqCst);

        self.update_last_will(self.state.create_last_will());

        loop {
            select! {
              event = self.eventloop.poll() => self.handle_event(event).await,
              Some(_) = self.stop_rx.recv() => break,
            }
        }

        if timeout(Duration::from_secs(1), self.poll_until_offline())
            .await
            .is_err()
        {
            self.on_offline().await;
        }

        _ = self.client_state_tx.send(ClientStateMessage::Stopped).await;
        info!("Edge node stopped. Node = {}", self.state.edge_node_id);
        self.state.running.store(false, Ordering::SeqCst);
    }
}
