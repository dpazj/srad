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
use tokio::{
    sync::{
        mpsc::{self, UnboundedSender},
        Mutex,
    },
    task,
};

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
    pub(crate) device: Arc<Device>,
}

impl DeviceHandle {
    /// Enabled the device
    ///
    /// Will attempt to birth the device. If the node is not online, the device will be birthed when it is next online.
    pub async fn enable(&self) {
        self.device.enabled.store(true, Ordering::SeqCst);
        self.device.birth(&BirthType::Birth).await;
    }

    /// Rebirth the device
    ///
    /// Manually trigger a rebirth for the device.
    pub async fn rebirth(&self) {
        self.device.enabled.store(true, Ordering::SeqCst);
        self.device.birth(&BirthType::Rebirth).await;
    }

    /// Disable the device
    ///
    /// Will produce a death message for the device. The node will no longer attempt to birth the device when it comes online.
    pub async fn disable(&self) {
        if !self.device.enabled.swap(false, Ordering::SeqCst) {
            return;
        }
        self.device.death(true).await
    }

    fn check_publish_state(&self) -> Result<(), PublishError> {
        if !self.device.eon_state.is_online() {
            return Err(PublishError::Offline);
        }
        if !self.device.birthed.load(Ordering::Relaxed) {
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
            seq: Some(self.device.eon_state.get_seq()),
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
            .device
            .client
            .try_publish_device_message(
                self.device.info.ddata_topic.clone(),
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
            .device
            .client
            .publish_device_message(
                self.device.info.ddata_topic.clone(),
                self.publish_metrics_to_payload(metrics),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => Err(PublishError::Offline),
        }
    }
}

pub(crate) struct DeviceInfo {
    id: DeviceId,
    pub(crate) name: Arc<String>,
    ddata_topic: DeviceTopic,
}

pub struct Device {
    pub(crate) info: DeviceInfo,
    birthed: AtomicBool,
    enabled: AtomicBool,
    eon_state: Arc<EoNState>,
    dev_impl: Box<DynDeviceMetricManager>,
    client: Arc<DynClient>,

    // Because we allow the user to directly call birth and death messages
    // through enable/disable/rebirth we need to protect against scenarios
    // where the Node is performing a birth/death sequence and the user does
    // the same or opposite action which could cause state inconsistency issues.
    birth_guard: Mutex<()>,
}

impl Device {
    fn generate_birth_payload(&self) -> Payload {
        let mut birth_initializer = BirthInitializer::new(BirthObjectType::Device(self.info.id));
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
        let guard = self.birth_guard.lock().await;

        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        if !self.eon_state.birthed() {
            return;
        }
        if *birth_type == BirthType::Birth && self.birthed.load(Ordering::SeqCst) {
            return;
        }

        let payload = self.generate_birth_payload();
        if self
            .client
            .publish_device_message(
                DeviceTopic::new(
                    &self.eon_state.group_id,
                    srad_types::topic::DeviceMessage::DBirth,
                    &self.eon_state.edge_node_id,
                    &self.info.name,
                ),
                payload,
            )
            .await
            .is_ok()
        {
            self.birthed.store(true, Ordering::SeqCst);
            info!("Device birthed. Node = {}, Device = {}, Type = {:?}", self.eon_state.edge_node_id, self.info.name, birth_type);
        }

        drop(guard)
    }

    async fn death(&self, publish: bool) {
        let guard = self.birth_guard.lock().await;

        if !self.birthed.load(Ordering::SeqCst) {
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
                        &self.info.name,
                    ),
                    payload,
                )
                .await;
        }
        self.birthed.store(false, Ordering::SeqCst);
        info!("Device dead. Node = {}, Device = {}", self.eon_state.edge_node_id, self.info.name);
        drop(guard);
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
                        self.info.name
                    );
                    return;
                }
            };
            self.dev_impl.on_dcmd(handle, message_metrics).await
        }
    }
}

enum ChannelMessage {
    Birth(BirthType),
    Death,
    Removed,
    SparkplugMessage(Message),
}

pub struct DeviceMap {
    device_ids: HashSet<DeviceId>,
    devices: HashMap<Arc<String>, (UnboundedSender<ChannelMessage>, DeviceId)>,
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

        let device = Arc::new(Device {
            info: DeviceInfo {
                id,
                name: name.clone(),
                ddata_topic: DeviceTopic::new(
                    group_id,
                    srad_types::topic::DeviceMessage::DData,
                    node_id,
                    &name,
                ),
            },
            birthed: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            eon_state,
            dev_impl,
            client,
            birth_guard: Mutex::new(()),
        });
        let handle = DeviceHandle {
            device: device.clone(),
        };
        device.dev_impl.init(&handle);

        let (device_tx, mut device_rx) = mpsc::unbounded_channel();
        _ = device_tx.send(ChannelMessage::Birth(BirthType::Birth));
        task::spawn(async move {
            while let Some(msg) = device_rx.recv().await {
                match msg {
                    ChannelMessage::Birth(birth_type) => device.birth(&birth_type).await,
                    ChannelMessage::Death => device.death(false).await,
                    ChannelMessage::SparkplugMessage(message) => {
                        let handle = DeviceHandle {
                            device: device.clone(),
                        };
                        device.handle_sparkplug_message(message, handle).await
                    }
                    ChannelMessage::Removed => {
                        device.death(true).await;
                        break;
                    }
                }
            }
        });

        self.devices.insert(name, (device_tx, id));
        Ok(handle)
    }

    pub(crate) fn remove_device(&mut self, device: &String) {
        let tx = {
            let (tx, id) = match self.devices.remove(device) {
                Some(entry) => entry,
                None => return,
            };

            self.remove_device_id(id);
            tx
        };
        _ = tx.send(ChannelMessage::Removed);
    }

    pub(crate) fn birth_devices(&self, birth_type: BirthType) {
        info!("Birthing Devices. Type = {:?}", birth_type);
        for (tx, _) in self.devices.values() {
            _ = tx.send(ChannelMessage::Birth(birth_type));
        }
    }

    pub(crate) fn on_death(&self) {
        for (tx, _) in self.devices.values() {
            _ = tx.send(ChannelMessage::Death);
        }
    }

    pub(crate) fn handle_device_message(&self, message: DeviceMessage) {
        let tx = {
            match self.devices.get(&message.device_id) {
                Some((tx, _)) => tx,
                None => {
                    warn!("Got message for unknown device. Device = '{}'", message.device_id);
                    return;
                }
            }
        };
        _ = tx.send(ChannelMessage::SparkplugMessage(message.message));
    }
}
