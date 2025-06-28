use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use log::{info, warn};
use srad_client::{DeviceMessage, DynClient, Message, MessageKind};
use srad_types::{payload::Payload, topic::DeviceTopic, utils::timestamp};
use tokio::{select, sync::mpsc, task};

use crate::{
    birth::{BirthInitializer, BirthObjectType},
    error::DeviceRegistrationError,
    metric::{MetricPublisher, PublishError, PublishMetric},
    metric_manager::manager::DynDeviceMetricManager,
    node::EoNState,
    BirthType,
};

#[derive(Clone)]
pub struct DeviceHandle {
    node_state: Arc<EoNState>,
    pub(crate) state: Arc<DeviceState>,
    client: Arc<DynClient>,
    handle_tx: mpsc::UnboundedSender<DeviceHandleRequest>,
}

impl DeviceHandle {
    /// Enabled the device
    ///
    /// Will attempt to birth the device. If the node is not online, the device will be birthed when it is next online.
    pub fn enable(&self) {
        _ = self.handle_tx.send(DeviceHandleRequest::Enable);
    }

    /// Rebirth the device
    ///
    /// Manually trigger a rebirth for the device.
    pub fn rebirth(&self) {
        _ = self.handle_tx.send(DeviceHandleRequest::Rebirth);
    }

    /// Disable the device
    ///
    /// Will produce a death message for the device. The node will no longer attempt to birth the device when it comes online.
    pub fn disable(&self) {
        _ = self.handle_tx.send(DeviceHandleRequest::Disable);
    }

    fn check_publish_state(&self) -> Result<(), PublishError> {
        if !self.node_state.is_online() {
            return Err(PublishError::Offline);
        }
        if !self.node_state.birthed() {
            return Err(PublishError::UnBirthed);
        }
        if !self.state.birthed.load(Ordering::Relaxed) {
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
            seq: Some(self.node_state.get_seq()),
            uuid: None,
            body: None,
        }
    }
}

impl MetricPublisher for DeviceHandle {
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
            .try_publish_device_message(
                self.state.ddata_topic.clone(),
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
            .publish_device_message(
                self.state.ddata_topic.clone(),
                self.publish_metrics_to_payload(metrics),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => Err(PublishError::Offline),
        }
    }
}

pub(crate) struct DeviceState {
    birthed: AtomicBool,
    id: DeviceId,
    pub(crate) name: Arc<String>,
    ddata_topic: DeviceTopic,
}

enum DeviceHandleRequest {
    Enable,
    Disable,
    Rebirth,
}

pub struct Device {
    state: Arc<DeviceState>,
    eon_state: Arc<EoNState>,
    dev_impl: Box<DynDeviceMetricManager>,
    client: Arc<DynClient>,
    enabled: bool,
    handle_request_tx: mpsc::UnboundedSender<DeviceHandleRequest>,
    device_message_rx: mpsc::UnboundedReceiver<Message>,
    node_state_rx: mpsc::UnboundedReceiver<NodeStateMessage>,
    handle_request_rx: mpsc::UnboundedReceiver<DeviceHandleRequest>,
}

impl Device {
    fn generate_birth_payload(&self) -> Payload {
        let mut birth_initializer = BirthInitializer::new(BirthObjectType::Device(self.state.id));
        self.dev_impl.initialise_birth(&mut birth_initializer);
        let timestamp = timestamp();
        let metrics = birth_initializer.finish();

        Payload {
            seq: Some(self.eon_state.get_seq()),
            timestamp: Some(timestamp),
            metrics,
            uuid: None,
            body: None,
        }
    }

    fn generate_death_payload(&self) -> Payload {
        let timestamp = timestamp();
        Payload {
            seq: Some(self.eon_state.get_seq()),
            timestamp: Some(timestamp),
            metrics: Vec::new(),
            uuid: None,
            body: None,
        }
    }

    async fn birth(&self, birth_type: &BirthType) {
        if !self.enabled {
            return;
        }
        if !self.eon_state.birthed() {
            return;
        }
        if *birth_type == BirthType::Birth && self.state.birthed.load(Ordering::SeqCst) {
            return;
        }
        self.state.birthed.store(false, Ordering::SeqCst);
        let payload = self.generate_birth_payload();
        if self
            .client
            .publish_device_message(
                DeviceTopic::new(
                    &self.eon_state.group_id,
                    srad_types::topic::DeviceMessage::DBirth,
                    &self.eon_state.edge_node_id,
                    &self.state.name,
                ),
                payload,
            )
            .await
            .is_ok()
        {
            self.state.birthed.store(true, Ordering::SeqCst);
            info!(
                "Device birthed. Node = {}, Device = {}, Type = {:?}",
                self.eon_state.edge_node_id, self.state.name, birth_type
            );
        }
    }

    async fn death(&self, publish: bool) {
        if !self.state.birthed.load(Ordering::SeqCst) {
            return;
        }
        if publish {
            let payload = self.generate_death_payload();
            _ = self
                .client
                .publish_device_message(
                    DeviceTopic::new(
                        &self.eon_state.group_id,
                        srad_types::topic::DeviceMessage::DDeath,
                        &self.eon_state.edge_node_id,
                        &self.state.name,
                    ),
                    payload,
                )
                .await;
        }
        self.state.birthed.store(false, Ordering::SeqCst);
        info!(
            "Device dead. Node = {}, Device = {}",
            self.eon_state.edge_node_id, self.state.name
        );
    }

    async fn handle_sparkplug_message(&self, message: Message, handle: DeviceHandle) {
        let payload = message.payload;
        let message_kind = message.kind;
        if MessageKind::Cmd == message_kind {
            let message_metrics = match payload.try_into() {
                Ok(metrics) => metrics,
                Err(_) => {
                    warn!(
                        "Got invalid CMD payload for device - ignoring. Device = {}",
                        self.state.name
                    );
                    return;
                }
            };
            self.dev_impl.on_dcmd(handle, message_metrics).await
        }
    }

    fn create_handle(&self) -> DeviceHandle {
        DeviceHandle {
            node_state: self.eon_state.clone(),
            state: self.state.clone(),
            client: self.client.clone(),
            handle_tx: self.handle_request_tx.clone(),
        }
    }

    async fn enable(&mut self) {
        self.enabled = true;
        self.birth(&BirthType::Birth).await
    }

    async fn disable(&mut self) {
        self.enabled = false;
        self.death(true).await
    }

    async fn run(mut self) {
        loop {
            select! {
                biased;
                Some(state_update) = self.node_state_rx.recv() => {
                    match state_update {
                        NodeStateMessage::Birth(birth_type) => self.birth(&birth_type).await,
                        NodeStateMessage::Death => self.death(false).await,
                        NodeStateMessage::Removed => {
                            self.death(true).await;
                            break;
                        },
                    }
                },
                Some(request) = self.handle_request_rx.recv() => {
                    match request {
                        DeviceHandleRequest::Enable => self.enable().await,
                        DeviceHandleRequest::Disable => self.disable().await,
                        DeviceHandleRequest::Rebirth => self.birth(&BirthType::Rebirth).await,
                    }
                },
                Some(message) = self.device_message_rx.recv() => self.handle_sparkplug_message(message, self.create_handle()).await,
            }
        }
    }
}

enum NodeStateMessage {
    Birth(BirthType),
    Death,
    Removed,
}

struct DeviceMapEntry {
    id: DeviceId,
    device_message_tx: mpsc::UnboundedSender<Message>,
    node_state_tx: mpsc::UnboundedSender<NodeStateMessage>,
}

pub struct DeviceMap {
    device_ids: HashSet<DeviceId>,
    devices: HashMap<Arc<String>, DeviceMapEntry>,
}

const OBJECT_ID_NODE: u32 = 0;
pub type DeviceId = u32;

impl DeviceMap {
    pub(crate) fn new() -> Self {
        Self {
            device_ids: HashSet::new(),
            devices: HashMap::new(),
        }
    }

    fn generate_device_id(&mut self, name: &String) -> DeviceId {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let mut id = hasher.finish() as DeviceId;
        while id == OBJECT_ID_NODE || self.device_ids.contains(&id) {
            id += 1;
        }
        self.device_ids.insert(id);
        id
    }

    fn remove_device_id(&mut self, id: DeviceId) {
        self.device_ids.remove(&id);
    }

    pub(crate) fn add_device(
        &mut self,
        group_id: &str,
        node_id: &str,
        name: String,
        dev_impl: Box<DynDeviceMetricManager>,
        eon_state: Arc<EoNState>,
        client: Arc<DynClient>,
    ) -> Result<DeviceHandle, DeviceRegistrationError> {
        if self.devices.get_key_value(&name).is_some() {
            return Err(DeviceRegistrationError::DuplicateDevice);
        }

        let name = Arc::new(name);
        let id = self.generate_device_id(&name);

        let (device_message_tx, device_message_rx) = mpsc::unbounded_channel();
        let (handle_request_tx, handle_request_rx) = mpsc::unbounded_channel();
        let (node_state_tx, node_state_rx) = mpsc::unbounded_channel();

        let device_map_entry = DeviceMapEntry {
            id,
            device_message_tx,
            node_state_tx,
        };

        let device = Device {
            state: Arc::new(DeviceState {
                id,
                name: name.clone(),
                ddata_topic: DeviceTopic::new(
                    group_id,
                    srad_types::topic::DeviceMessage::DData,
                    node_id,
                    &name,
                ),
                birthed: AtomicBool::new(false),
            }),
            enabled: false,
            eon_state,
            dev_impl,
            client,
            handle_request_tx,
            device_message_rx,
            node_state_rx,
            handle_request_rx,
        };

        let handle = device.create_handle();
        device.dev_impl.init(&handle);

        task::spawn(async move { device.run().await });
        self.devices.insert(name, device_map_entry);
        Ok(handle)
    }

    pub(crate) fn remove_device(&mut self, device: &String) {
        let entry = {
            let entry = match self.devices.remove(device) {
                Some(entry) => entry,
                None => return,
            };

            self.remove_device_id(entry.id);
            entry
        };
        _ = entry.node_state_tx.send(NodeStateMessage::Removed);
    }

    pub(crate) fn birth_devices(&self, birth_type: BirthType) {
        info!("Birthing Devices. Type = {:?}", birth_type);
        for entry in self.devices.values() {
            _ = entry
                .node_state_tx
                .send(NodeStateMessage::Birth(birth_type));
        }
    }

    pub(crate) fn on_death(&self) {
        for entry in self.devices.values() {
            _ = entry.node_state_tx.send(NodeStateMessage::Death);
        }
    }

    pub(crate) fn handle_device_message(&self, message: DeviceMessage) {
        let entry = {
            match self.devices.get(&message.device_id) {
                Some(entry) => entry,
                None => {
                    warn!(
                        "Got message for unknown device. Device = '{}'",
                        message.device_id
                    );
                    return;
                }
            }
        };
        _ = entry.device_message_tx.send(message.message);
    }
}
