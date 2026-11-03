use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use log::{debug, error, info, warn};
use srad_client::{
    ClientError, DeviceMessage, DynClient, DynEventLoop, Event, LastWill, Message, MessageKind,
    StatePayload,
};
use srad_types::{
    constants::{self, BDSEQ, NODE_CONTROL_REBIRTH},
    payload::{self, metric::Value, DataType, Payload},
    topic::{
        DeviceMessage as DeviceMessageType, DeviceTopic, NodeMessage as NodeMessageType, NodeTopic,
        QoS, StateTopic, Topic, TopicFilter,
    },
    utils::timestamp,
    MetricValue, Template, TemplateDefinition,
};
use thiserror::Error;
use tokio::{
    select,
    sync::{mpsc, oneshot},
    time::{sleep_until, timeout, Instant as TokioInstant},
};

use crate::{
    birth::{AliasConfig, BirthObjectType},
    device::{DeviceMap, DeviceRegistrationError},
    metric_manager::manager::DynNodeMetricManager,
    BirthInitializer, BirthMetricDetails, BirthType, DeviceHandle, DeviceMetricManager, EoNBuilder,
    MessageMetrics, MetricPublisher, PublishError, PublishMetric, StateError,
};

pub(crate) struct EoNConfig {
    node_rebirth_request_cooldown: Duration,
    retry_delay: Duration,
}

struct EoNStateInner {
    seq: u8,
    online: bool,
    birthed: bool,
    primary_host_online: bool,
    last_primary_host_timestamp: Option<u64>,
}

pub(crate) struct EoNState {
    running: AtomicBool,
    bdseq: AtomicU8,
    inner: Mutex<EoNStateInner>,
    pub group_id: String,
    pub edge_node_id: String,
    pub primary_host_id: Option<String>,
    pub ndata_topic: NodeTopic,
}

impl EoNState {
    pub(crate) fn get_next_seq(&self) -> Result<u64, StateError> {
        let mut state = self.inner.lock().unwrap();
        if !state.online {
            return Err(StateError::Offline);
        }
        if !state.birthed {
            return Err(StateError::UnBirthed);
        }
        state.seq = state.seq.wrapping_add(1);
        Ok(state.seq as u64)
    }

    fn online_swap(&self, online: bool) -> bool {
        let mut state = self.inner.lock().unwrap();
        let old_online_state = state.online;
        state.online = online;
        old_online_state
    }

    fn is_online(&self) -> bool {
        self.inner.lock().unwrap().online
    }

    fn is_primary_host_online(&self) -> bool {
        self.inner.lock().unwrap().primary_host_online
    }

    /// Whether the node is allowed to publish its birth message.
    ///
    /// Nodes without a primary host may birth as soon as they are online. Nodes
    /// configured with one must wait until that host is online.
    fn ready_to_birth(&self) -> bool {
        self.primary_host_id.is_none() || self.is_primary_host_online()
    }

    fn set_primary_host_online(&self, online: bool) {
        self.inner.lock().unwrap().primary_host_online = online;
    }

    /// Require the primary host to be verified again before the next birth.
    fn reset_primary_host(&self) {
        self.inner.lock().unwrap().primary_host_online = false;
    }

    /// Record the timestamp of a STATE message.
    ///
    /// Returns `false` if the timestamp is older than the last accepted STATE.
    /// Sparkplug requires the timestamp to be greater than or equal to the
    /// previous STATE message's timestamp.
    fn update_primary_host_timestamp(&self, timestamp: u64) -> bool {
        let mut state = self.inner.lock().unwrap();
        if let Some(last) = state.last_primary_host_timestamp {
            if timestamp < last {
                return false;
            }
        }
        state.last_primary_host_timestamp = Some(timestamp);
        true
    }

    fn set_dead(&self) {
        let mut state = self.inner.lock().unwrap();
        state.birthed = false;
    }

    fn birthed(&self) -> bool {
        self.inner.lock().unwrap().birthed
    }

    fn start_birth(&self) {
        let mut state = self.inner.lock().unwrap();
        state.birthed = false;
        state.seq = 0;
    }

    fn birth_completed(&self) {
        self.inner.lock().unwrap().birthed = true
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
        let state_topic = match &self.primary_host_id {
            Some(host_id) => StateTopic::new_host(host_id),
            None => StateTopic::new(),
        };

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
            TopicFilter::new_with_qos(Topic::State(state_topic), QoS::AtLeastOnce),
        ]
    }
}

#[derive(Debug)]
struct EoNShutdown;

/// Errors that prevent an edge node from establishing its session.
#[derive(Debug, Error)]
enum SessionError {
    /// The client failed to subscribe to the topics required by the node.
    #[error("failed to subscribe to required topics: {0}")]
    Subscribe(#[source] ClientError),
}

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
        info!("Edge node stopping. node={}", self.state.edge_node_id);
        let topic = NodeTopic::new(
            &self.state.group_id,
            NodeMessageType::NDeath,
            &self.state.edge_node_id,
        );
        let payload = self.state.generate_death_payload();
        match self.client.try_publish_node_message(topic, payload).await {
            Ok(_) => (),
            Err(_) => debug!(
                "Unable to publish node death certificate on exit. node={}",
                self.state.edge_node_id
            ),
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

    fn publish_metrics_to_payload(&self, seq: u64, metrics: Vec<PublishMetric>) -> Payload {
        let timestamp = timestamp();
        let mut payload_metrics = Vec::with_capacity(metrics.len());
        for x in metrics.into_iter() {
            payload_metrics.push(x.into());
        }
        Payload {
            timestamp: Some(timestamp),
            metrics: payload_metrics,
            seq: Some(seq),
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
        self.client
            .try_publish_node_message(
                self.state.ndata_topic.clone(),
                self.publish_metrics_to_payload(self.state.get_next_seq()?, metrics),
            )
            .await?;
        Ok(())
    }

    async fn publish_metrics_unsorted(
        &self,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), PublishError> {
        if metrics.is_empty() {
            return Err(PublishError::NoMetrics);
        }
        self.client
            .publish_node_message(
                self.state.ndata_topic.clone(),
                self.publish_metrics_to_payload(self.state.get_next_seq()?, metrics),
            )
            .await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum TemplateRegistryError {
    #[error("Invalid template name")]
    InvalidName,
    #[error("Duplicate template. A template with that name is already registered")]
    Duplicate,
    #[error("The Templates Definition is invalid")]
    InvalidDefinition,
    #[error("The Templates Definition contained a template that has not been registered")]
    UnregisteredMetric,
}

/// A struct representing a collection of Template Definitions for a Node
///
/// Used to manage the template definitions for the node. A definition must be included in the registry
/// in order to register a metric that has a template datatype with a [BirthInitializer].
///
/// When a Node generates a birth message, a template definition for each templates registered with this structure
/// will be included in the Nodes birth message. [srad_types::TemplateMetadata::template_definition_metric_name]
/// for the type is used to define the name of the metric that represents the template definition. [srad_types::Template::template_definition()]
/// is used to generate the definition used for the metrics value.
///
/// For templates which use another template as one of it's metrics, the template used as a field must be registered first.
///
/// This registry is checked any time a metric that uses a template as the type for its value is registered for birth.
#[derive(Debug, Clone)]
pub struct TemplateRegistry {
    templates: HashMap<String, TemplateDefinition>,
}

impl TemplateRegistry {
    pub(crate) fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Empty the registry of all template definitions
    pub fn clear(&mut self) {
        self.templates.clear();
    }

    /// Remove a template
    pub fn deregister(&mut self, name: &str) {
        self.templates.remove(name);
    }

    // Recurse through all template metrics in the metric list and ensure they have been registered.
    fn check_template_metrics(
        &self,
        metrics: &Vec<payload::Metric>,
    ) -> Result<(), TemplateRegistryError> {
        for x in metrics {
            let datatype = match &x.datatype {
                Some(t) => match DataType::try_from(*t) {
                    Ok(datatype) => datatype,
                    Err(_) => return Err(TemplateRegistryError::InvalidDefinition),
                },
                None => return Err(TemplateRegistryError::InvalidDefinition),
            };
            if datatype != DataType::Template {
                continue;
            }

            if let Value::TemplateValue(template) = x
                .value
                .as_ref()
                .ok_or(TemplateRegistryError::InvalidDefinition)?
            {
                let ref_name = template
                    .template_ref
                    .as_ref()
                    .ok_or(TemplateRegistryError::InvalidDefinition)?;
                if self.templates.contains_key(ref_name) {
                    return Ok(());
                }
                self.check_template_metrics(&template.metrics)?;
            } else {
                return Err(TemplateRegistryError::InvalidDefinition);
            }
        }
        Ok(())
    }

    /// Add a template definition
    pub fn register<T: Template>(&mut self) -> Result<(), TemplateRegistryError> {
        let name = T::template_definition_metric_name();
        if name == NODE_CONTROL_REBIRTH || name == BDSEQ {
            return Err(TemplateRegistryError::InvalidName);
        }

        if self.templates.contains_key(&name) {
            return Err(TemplateRegistryError::Duplicate);
        }

        let definition = T::template_definition();
        self.check_template_metrics(&definition.metrics)?;

        self.templates.insert(name, definition);
        Ok(())
    }

    /// Check the registry contains a template with the given name
    pub fn contains(&self, template_definition_metric_name: &str) -> bool {
        self.templates.contains_key(template_definition_metric_name)
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

    template_registry: Arc<TemplateRegistry>,

    rebirth_request_tx: mpsc::Sender<()>,

    node_message_rx: mpsc::UnboundedReceiver<Message>,
    client_state_rx: mpsc::Receiver<ClientStateMessage>,
    rebirth_request_rx: mpsc::Receiver<()>,

    /// Whether the underlying client currently reports an established connection.
    client_online: bool,
    /// When the next attempt to establish the node session may be made.
    retry_at: Option<Instant>,
}

impl Node {
    fn generate_birth_payload(&self, bdseq: i64, seq: u64) -> Payload {
        let timestamp = timestamp();
        let mut birth_initializer =
            BirthInitializer::new(BirthObjectType::Node, self.template_registry.clone());

        birth_initializer
            .register_metric(
                BirthMetricDetails::new_with_initial_value(constants::BDSEQ, bdseq)
                    .with_alias_config(AliasConfig::None),
            )
            .unwrap();
        birth_initializer
            .register_metric(
                BirthMetricDetails::new_with_initial_value(constants::NODE_CONTROL_REBIRTH, false)
                    .with_alias_config(AliasConfig::None),
            )
            .unwrap();

        for (name, template_definition) in &self.template_registry.templates {
            birth_initializer
                .register_template_definition(name.clone(), template_definition.clone())
                .unwrap();
        }

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

    async fn node_birth(&mut self) -> Result<(), PublishError> {
        /* [tck-id-topics-nbirth-seq-num] The NBIRTH MUST include a sequence number in the payload and it MUST have a value of 0. */
        self.state.start_birth();

        let bdseq = self.state.bdseq.load(Ordering::SeqCst) as i64;

        //TODO any way we can avoid this clone? could use Ref counting in the TemplateRegistry but it might not be worth it
        let mut updatable_template_registry = self.template_registry.as_ref().clone();
        self.metric_manager
            .birth_update_template_registry(&mut updatable_template_registry);
        self.template_registry = Arc::new(updatable_template_registry);

        let payload = self.generate_birth_payload(bdseq, 0);
        let topic = self.state.birth_topic();
        self.client.publish_node_message(topic, payload).await?;
        self.state.birth_completed();
        Ok(())
    }

    async fn birth(&mut self, birth_type: BirthType) -> Result<(), PublishError> {
        info!(
            "Birthing Node. node={} type={birth_type:?}",
            self.state.edge_node_id
        );
        self.node_birth().await?;
        self.devices
            .lock()
            .unwrap()
            .birth_devices(birth_type, &self.template_registry);
        Ok(())
    }

    async fn rebirth(&mut self) {
        if !self.state.birthed() {
            return;
        }
        if let Err(e) = self.birth(BirthType::Rebirth).await {
            error!(
                "Publishing node rebirth message failed. node={}: {e}",
                self.state.edge_node_id
            );
        }
    }

    fn death(&self) {
        self.state.set_dead();
        self.state.bdseq.fetch_add(1, Ordering::SeqCst);
        self.devices.lock().unwrap().on_death();
    }

    async fn on_online(&mut self) -> Result<(), SessionError> {
        if self.state.is_online() {
            return Ok(());
        }

        info!("Edge node online. node={}", self.state.edge_node_id);

        // Subscribe before publishing the birth message so that the node can receive
        // commands as soon as it is birthed. When a primary host is configured the
        // birth itself is deferred until that host publishes an online STATE.
        self.client
            .subscribe_many(self.state.sub_topics())
            .await
            .map_err(SessionError::Subscribe)?;
        self.state.online_swap(true);
        Ok(())
    }

    /// Attempt to establish the node session, retrying on a delay if it fails.
    ///
    /// Failures are treated like a lost connection: the client is disconnected and
    /// the node tries again once the configured retry delay has elapsed.
    async fn try_online(&mut self) {
        if !self.client_online || self.retry_at.is_some() {
            return;
        }

        if !self.state.is_online() {
            if let Err(e) = self.on_online().await {
                warn!(
                    "Failed to establish edge node session. node={}: {e}",
                    self.state.edge_node_id
                );
                self.retry_at = Some(Instant::now() + self.config.retry_delay);
                _ = self.client.disconnect().await;
                return;
            }
        }

        if self.state.birthed() {
            return;
        }

        if !self.state.ready_to_birth() {
            debug!(
                "Waiting for primary host before birthing node. node={} host={}",
                self.state.edge_node_id,
                self.state.primary_host_id.as_deref().unwrap_or_default()
            );
            return;
        }

        if let Err(e) = self.birth(BirthType::Birth).await {
            // The session was never established, so do not publish a death message.
            warn!(
                "Failed to publish node birth message. node={}: {e}",
                self.state.edge_node_id
            );
            self.state.online_swap(false);
            self.retry_at = Some(Instant::now() + self.config.retry_delay);
            _ = self.client.disconnect().await;
        }
    }

    fn on_offline(&self, will_sender: oneshot::Sender<LastWill>) {
        // A new MQTT session must verify the primary host again before birthing.
        self.state.reset_primary_host();

        if !self.state.online_swap(false) {
            return;
        }

        info!("Edge node offline. node={}", self.state.edge_node_id);
        self.death();
        let new_lastwill = self.state.create_last_will();
        _ = will_sender.send(new_lastwill);
    }

    /// Handle a STATE message from the primary host application.
    async fn on_primary_host_state(&mut self, host_id: String, payload: StatePayload) {
        let Some(expected_host_id) = self.state.primary_host_id.clone() else {
            return;
        };

        if host_id != expected_host_id {
            warn!(
                "Ignoring STATE message for an unexpected host. node={} host={} expected={}",
                self.state.edge_node_id, host_id, expected_host_id
            );
            return;
        }

        let timestamp = match payload {
            StatePayload::Online { timestamp } | StatePayload::Offline { timestamp } => timestamp,
        };

        if !self.state.update_primary_host_timestamp(timestamp) {
            warn!(
                "Ignoring STATE message with an older timestamp. node={} host={} timestamp={}",
                self.state.edge_node_id, host_id, timestamp
            );
            return;
        }

        match payload {
            StatePayload::Online { .. } => {
                info!(
                    "Primary host online. node={} host={}",
                    self.state.edge_node_id, host_id
                );
                self.state.set_primary_host_online(true);
                self.try_online().await;
            }
            StatePayload::Offline { .. } => {
                info!(
                    "Primary host offline. node={} host={}",
                    self.state.edge_node_id, host_id
                );
                self.state.set_primary_host_online(false);
                self.on_primary_host_offline().await;
            }
        }
    }

    /// Terminate the Sparkplug session after the primary host goes offline.
    ///
    /// The NDEATH message uses the `bdSeq` of the current session. Disconnecting
    /// lets the client reconnect with a new session and an incremented `bdSeq`;
    /// the node then waits for the primary host to come online again before
    /// publishing its next birth.
    async fn on_primary_host_offline(&mut self) {
        if !self.state.birthed() {
            return;
        }

        let topic = NodeTopic::new(
            &self.state.group_id,
            NodeMessageType::NDeath,
            &self.state.edge_node_id,
        );
        if let Err(e) = self
            .client
            .publish_node_message(topic, self.state.generate_death_payload())
            .await
        {
            error!(
                "Failed to publish node death certificate. node={}: {e}",
                self.state.edge_node_id
            );
        }

        // Stop publishing data immediately. `on_offline` increments `bdSeq` once
        // the client reports the new session and updates the last will.
        self.state.set_dead();
        self.devices.lock().unwrap().on_death();

        _ = self.client.disconnect().await;
    }

    async fn on_sparkplug_message(&mut self, message: Message, handle: NodeHandle) {
        let message_kind = message.kind;

        if message_kind == MessageKind::Cmd {
            let payload = match message.payload.decode() {
                Ok(payload) => payload,
                Err(e) => {
                    warn!(
                        "Received invalid CMD payload - ignoring. node={} error={e}",
                        self.state.edge_node_id
                    );
                    return;
                }
            };
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
                    warn!(
                        "Received invalid NCMD Rebirth metric - ignoring request. node={}",
                        self.state.edge_node_id
                    )
                }
            }

            let message_metrics: MessageMetrics = match payload.try_into() {
                Ok(metrics) => metrics,
                Err(_) => {
                    warn!(
                        "Received invalid CMD payload - ignoring request. node={}",
                        self.state.edge_node_id
                    );
                    return;
                }
            };

            self.metric_manager.on_ncmd(handle, message_metrics).await;
            if rebirth {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                let time_since_last = now - self.last_node_rebirth_request;
                if time_since_last < self.config.node_rebirth_request_cooldown {
                    info!(
                        "Got Rebirth CMD but cooldown time not expired. Ignoring. node={}",
                        self.state.edge_node_id
                    );
                    return;
                }
                info!(
                    "Got Rebirth CMD - Rebirthing Node. node={}",
                    self.state.edge_node_id
                );
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
            let retry_at = TokioInstant::from_std(self.retry_at.unwrap_or_else(Instant::now));
            select! {
                biased;
                maybe_state_update = self.client_state_rx.recv() => match maybe_state_update {
                    Some (state_update) => match state_update {
                        ClientStateMessage::Online => {
                            self.client_online = true;
                            self.try_online().await;
                        },
                        ClientStateMessage::Offline(sender) => {
                            self.client_online = false;
                            self.on_offline(sender);
                        },
                        ClientStateMessage::State { host_id, payload } => {
                            self.on_primary_host_state(host_id, payload).await;
                        },
                        ClientStateMessage::Stopped => break
                    },
                    None => break, //EoN has been dropped
                },
                _ = sleep_until(retry_at), if self.retry_at.is_some() => {
                    self.retry_at = None;
                    self.try_online().await;
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
    State {
        host_id: String,
        payload: StatePayload,
    },
}

/// struct that represents a Sparkplug Edge Node instance.
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
        if let Some(primary_host_id) = &builder.primary_host_id {
            srad_types::utils::validate_name(primary_host_id)?;
        }

        let metric_manager = builder.metric_manager;
        let (eventloop, client) = builder.eventloop_client;
        let primary_host_id = builder.primary_host_id;
        let (stop_tx, stop_rx) = mpsc::channel(1);

        let state = Arc::new(EoNState {
            running: AtomicBool::new(false),
            bdseq: AtomicU8::new(0),
            inner: Mutex::new(EoNStateInner {
                seq: 0,
                online: false,
                birthed: false,
                primary_host_online: false,
                last_primary_host_timestamp: None,
            }),
            ndata_topic: NodeTopic::new(&group_id, NodeMessageType::NData, &node_id),
            group_id,
            edge_node_id: node_id,
            primary_host_id,
        });

        let template_registry = Arc::new(builder.templates);

        let devices = Arc::new(Mutex::new(DeviceMap::new(template_registry.clone())));

        let (node_message_tx, node_message_rx) = mpsc::unbounded_channel();
        let (rebirth_request_tx, rebirth_request_rx) = mpsc::channel(1);
        let (client_state_tx, client_state_rx) = mpsc::channel(1);

        let node = Node {
            metric_manager,
            template_registry,
            client: client.clone(),
            state: state.clone(),
            devices: devices.clone(),
            stop_tx,
            config: Arc::new(EoNConfig {
                node_rebirth_request_cooldown: builder.node_rebirth_request_cooldown,
                retry_delay: builder.retry_delay,
            }),
            last_node_rebirth_request: Duration::new(0, 0),
            node_message_rx,
            rebirth_request_rx,
            rebirth_request_tx,
            client_state_rx,
            client_online: false,
            retry_at: None,
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

    async fn on_state(&mut self, host_id: String, payload: StatePayload) {
        if self.state.primary_host_id.is_none() {
            return;
        }
        _ = self
            .client_state_tx
            .send(ClientStateMessage::State { host_id, payload })
            .await;
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
            Event::State { host_id, payload } => self.on_state(host_id, payload).await,
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
    /// Runs the Edge Node until [NodeHandle::cancel()] is called.
    ///
    /// If the node cannot establish its session, for example because subscribing to
    /// the command topics or publishing the birth message fails, it disconnects and
    /// retries after a delay rather than stopping.
    pub async fn run(mut self) {
        info!("Edge node running. node={}", self.state.edge_node_id);
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
        info!("Edge node stopped. node={}", self.state.edge_node_id);
        self.state.running.store(false, Ordering::SeqCst);
    }
}
