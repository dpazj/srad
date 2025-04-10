use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, error, info, warn};
use srad_client::{DeviceMessage, DynClient, DynEventLoop, MessageKind};
use srad_client::{Event, NodeMessage};

use srad_types::constants::NODE_CONTROL_REBIRTH;
use srad_types::payload::metric::Value;
use srad_types::payload::ToMetric;
use srad_types::topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter};
use srad_types::utils::timestamp;
use srad_types::MetricValue;
use srad_types::{
    constants,
    payload::Payload,
    topic::{DeviceMessage as DeviceMessageType, NodeMessage as NodeMessageType},
};
use tokio::time::timeout;

use crate::birth::{BirthInitializer, BirthMetricDetails, BirthObjectType};
use crate::builder::EoNBuilder;
use crate::device::{DeviceHandle, DeviceMap};
use crate::error::DeviceRegistrationError;
use crate::metric::{MessageMetrics, MetricPublisher, PublishError, PublishMetric};
use crate::metric_manager::manager::{DeviceMetricManager, DynNodeMetricManager};
use crate::registry::Registry;
use crate::BirthType;

use tokio::{select, sync::mpsc, task};

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
        info!("Edge node stopping");
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
    pub async fn register_device<S, M>(
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
        let handle = self
            .node
            .devices
            .add_device(
                &self.node.state.group_id,
                &self.node.state.edge_node_id,
                name,
                Arc::new(dev_impl),
            )
            .await?;
        Ok(handle)
    }

    /// Unregister a device using it's handle.
    pub async fn unregister_device(&self, handle: DeviceHandle) {
        self.unregister_device_named(&handle.device.info.name).await;
    }

    /// Unregister a device using it's name.
    pub async fn unregister_device_named(&self, name: &String) {
        self.node.devices.remove_device(name).await
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
            payload_metrics.push(x.to_metric());
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

pub struct Node {
    state: Arc<EoNState>,
    metric_manager: Box<DynNodeMetricManager>,
    devices: DeviceMap,
    client: Arc<DynClient>,
    stop_tx: mpsc::Sender<EoNShutdown>,
}

impl Node {
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
            metrics: metrics,
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
        info!("Birthing Node. Type: {:?}", birth_type);
        self.node_birth().await;
        self.devices.birth_devices(birth_type).await;
    }
}

/// Structure that represents a Sparkplug Edge Node instance
pub struct EoN {
    node: Arc<Node>,
    eventloop: Box<DynEventLoop>,
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
            edge_node_id,
        });

        let registry = Arc::new(Mutex::new(Registry::new()));

        let node = Arc::new(Node {
            metric_manager,
            client: client.clone(),
            devices: DeviceMap::new(state.clone(), registry.clone(), client),
            state,
            stop_tx,
        });

        let mut eon = Self {
            node,
            eventloop,
            stop_rx,
        };
        let handle = NodeHandle {
            node: eon.node.clone(),
        };
        eon.node.metric_manager.init(&handle);
        eon.update_last_will();
        Ok((eon, handle))
    }

    fn update_last_will(&mut self) {
        self.eventloop
            .set_last_will(srad_client::LastWill::new_node(
                &self.node.state.group_id,
                &self.node.state.edge_node_id,
                self.node.generate_death_payload(),
            ));
    }

    fn on_online(&self) {
        if self.node.state.online.swap(true, Ordering::SeqCst) {
            return;
        }
        info!("Edge node online");
        let sub_topics = self.node.state.sub_topics();
        let node = self.node.clone();

        tokio::spawn(async move {
            if node.client.subscribe_many(sub_topics).await.is_ok() {
                node.birth(BirthType::Birth).await
            };
        });
    }

    async fn on_offline(&mut self) {
        if !self.node.state.online.swap(false, Ordering::SeqCst) {
            return;
        }
        info!("Edge node offline");
        self.node.devices.on_offline().await;
        self.node.state.bdseq.fetch_add(1, Ordering::SeqCst);
        self.update_last_will();
    }

    fn on_node_message(&self, message: NodeMessage) {
        let payload = message.message.payload;
        let message_kind = message.message.kind;
        
        if message_kind == MessageKind::Cmd {
            let mut rebirth = false;
            for x in &payload.metrics {
                if x.alias.is_some() { continue; }

                let metric_name = match &x.name {
                    Some(name) => name,
                    None => continue,
                };

                if metric_name != NODE_CONTROL_REBIRTH { continue; }
                
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

            let node = self.node.clone();
            task::spawn(async move {
                node.metric_manager
                    .on_ncmd(NodeHandle { node: node.clone() }, message_metrics)
                    .await;
                if rebirth {
                    info!("Got Rebirth CMD - Rebirthing Node");
                    node.birth(BirthType::Rebirth).await
                }
            });
        }
    }

    fn on_device_message(&self, message: DeviceMessage) {
        let node = self.node.clone();
        task::spawn(async move {
            node.devices.handle_device_message(message).await;
        });
    }

    async fn handle_event(&mut self, event: Event) {
        match event {
            Event::Online => self.on_online(),
            Event::Offline => self.on_offline().await,
            Event::Node(node_message) => self.on_node_message(node_message),
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
        while self.node.state.is_online() {
            if Event::Offline == self.eventloop.poll().await {
                self.on_offline().await
            }
        }
        true
    }

    async fn poll_until_offline_with_timeout(&mut self) {
        _ = timeout(Duration::from_secs(1), self.poll_until_offline()).await;
    }

    /// Run the Edge Node
    ///
    /// Runs the Edge Node until [NodeHandle::cancel()] is called
    pub async fn run(&mut self) {
        info!("Edge node running");
        self.update_last_will();
        loop {
            select! {
              event = self.eventloop.poll() => self.handle_event(event).await,
              Some(_) = self.stop_rx.recv() => break,
            }
        }
        self.poll_until_offline_with_timeout().await;
        self.on_offline().await;
        info!("Edge node stopped");
    }
}
