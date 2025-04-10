use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use futures::future::join_all;
use log::{debug, info, warn};
use srad_client::{DeviceMessage, DynClient, MessageKind};
use srad_types::{
    payload::{Payload, ToMetric},
    topic::DeviceTopic,
    utils::timestamp,
};

use crate::{
    birth::{BirthInitializer, BirthObjectType},
    error::DeviceRegistrationError,
    metric::{MetricPublisher, PublishError, PublishMetric},
    metric_manager::manager::DynDeviceMetricManager,
    node::EoNState,
    registry::{self, DeviceId},
    BirthType,
};

pub(crate) struct DeviceInfo {
    id: DeviceId,
    pub(crate) name: Arc<String>,
    ddata_topic: DeviceTopic,
}

/// A handle for interacting with an Edge Device
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
        if self.device.enabled.swap(false, Ordering::SeqCst) == false {
            //already disabled
            return;
        };
        self.device.death(true).await
    }

    fn check_publish_state(&self) -> Result<(), PublishError> {
        if !self.device.eon_state.is_online() {
            return Err(PublishError::Offline);
        }
        if self.device.birthed.load(Ordering::Relaxed) == false {
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
        if metrics.len() == 0 {
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
        if metrics.len() == 0 {
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

pub struct Device {
    pub(crate) info: DeviceInfo,
    birthed: AtomicBool,
    birth_lock: tokio::sync::Mutex<()>,
    enabled: AtomicBool,
    eon_state: Arc<EoNState>,
    dev_impl: Arc<DynDeviceMetricManager>,
    client: Arc<DynClient>,
}

impl Device {
    fn generate_birth_payload(&self) -> Payload {
        let mut birth_initializer =
            BirthInitializer::new(BirthObjectType::Device(self.info.id.clone()));
        self.dev_impl.initialise_birth(&mut birth_initializer);
        let timestamp = timestamp();
        let metrics = birth_initializer.finish();

        Payload {
            seq: Some(self.eon_state.get_seq()),
            timestamp: Some(timestamp),
            metrics: metrics,
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

    pub async fn birth(&self, birth_type: &BirthType) {
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let guard = self.birth_lock.lock().await;
        if !self.eon_state.birthed() {
            return;
        }
        if *birth_type == BirthType::Birth && self.birthed.load(Ordering::SeqCst) == true {
            return;
        }
        debug!("Device {} birthing. Type: {:?}", self.info.name, birth_type);
        let payload = self.generate_birth_payload();
        match self
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
        {
            Ok(_) => self.birthed.store(true, Ordering::SeqCst),
            Err(_) => (),
        };
        drop(guard)
    }

    pub async fn death(&self, publish: bool) {
        let guard = self.birth_lock.lock().await;
        if self.birthed.load(Ordering::SeqCst) == false {
            return;
        }
        if publish {
            let payload = self.generate_death_payload();
            match self
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
                .await
            {
                Ok(_) => (),
                Err(_) => (),
            };
        }
        self.birthed.store(false, Ordering::SeqCst);
        debug!("Device {} dead", self.info.name);
        drop(guard)
    }
}

pub struct DeviceMapInner {
    devices: HashMap<Arc<String>, Arc<Device>>,
}

pub struct DeviceMap {
    client: Arc<DynClient>,
    state: tokio::sync::Mutex<DeviceMapInner>,
    eon_state: Arc<EoNState>,
    registry: Arc<Mutex<registry::Registry>>,
}

impl DeviceMap {
    pub fn new(
        eon_state: Arc<EoNState>,
        registry: Arc<Mutex<registry::Registry>>,
        client: Arc<DynClient>,
    ) -> Self {
        Self {
            eon_state,
            registry,
            client,
            state: tokio::sync::Mutex::new(DeviceMapInner {
                devices: HashMap::new(),
            }),
        }
    }

    pub async fn add_device(
        &self,
        group_id: &String,
        node_id: &String,
        name: String,
        dev_impl: Arc<DynDeviceMetricManager>,
    ) -> Result<DeviceHandle, DeviceRegistrationError> {
        let mut state = self.state.lock().await;
        if let Some(_) = state.devices.get_key_value(&name) {
            return Err(DeviceRegistrationError::DuplicateDevice);
        }

        let name = Arc::new(name);
        let mut registry = self.registry.lock().unwrap();
        let id = registry.generate_device_id(name.clone());
        drop(registry);

        let ddata_topic = DeviceTopic::new(
            &group_id,
            srad_types::topic::DeviceMessage::DData,
            &node_id,
            &name,
        );

        let device = Arc::new(Device {
            info: DeviceInfo {
                id,
                name: name.clone(),
                ddata_topic: ddata_topic,
            },
            birth_lock: tokio::sync::Mutex::new(()),
            birthed: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            eon_state: self.eon_state.clone(),
            dev_impl,
            client: self.client.clone(),
        });
        let handle = DeviceHandle {
            device: device.clone(),
        };
        device.dev_impl.init(&handle);
        state.devices.insert(name, device);
        drop(state);
        Ok(handle)
    }

    pub async fn remove_device(&self, device: &String) {
        let mut state = self.state.lock().await;
        let dev = match state.devices.remove(device) {
            Some(dev) => dev,
            None => return,
        };

        let mut registry = self.registry.lock().unwrap();
        registry.remove_device_id(dev.info.id);
        drop(registry);

        dev.death(true).await;
    }

    pub async fn birth_devices(&self, birth_type: BirthType) {
        info!("Birthing Devices. Type: {:?}", birth_type);
        let device_map = self.state.lock().await;
        let futures: Vec<_> = device_map
            .devices
            .values()
            .map(|x| x.birth(&birth_type))
            .collect();
        join_all(futures).await;
    }

    pub async fn on_offline(&self) {
        let device_map = self.state.lock().await;
        let futures: Vec<_> = device_map
            .devices
            .values()
            .map(|x| x.death(false))
            .collect();
        join_all(futures).await;
    }

    pub async fn handle_device_message(&self, message: DeviceMessage) {
        let dev = {
            let state = self.state.lock().await;
            let dev = state.devices.get(&message.device_id);
            match dev {
                Some(dev) => dev.clone(),
                None => {
                    warn!("Got message for unknown device '{}'", message.device_id);
                    return;
                }
            }
        };

        let payload = message.message.payload;
        let message_kind = message.message.kind;
        match message_kind {
            MessageKind::Cmd => {
                let message_metrics = match payload.try_into() {
                    Ok(metrics) => metrics,
                    Err(_) => {
                        warn!(
                            "Got invalid CMD payload for device '{}' - ignoring",
                            message.device_id
                        );
                        return;
                    }
                };
                dev.dev_impl
                    .on_dcmd(
                        DeviceHandle {
                            device: dev.clone(),
                        },
                        message_metrics,
                    )
                    .await
            }
            _ => (),
        }
    }
}
