use std::{
    collections::{HashMap, HashSet}, hash::{DefaultHasher, Hash, Hasher}, sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    }
};

use futures::future::join_all;
use log::{debug, info, warn};
use srad_client::{DeviceMessage, DynClient, MessageKind};
use srad_types::{payload::Payload, topic::DeviceTopic, utils::timestamp};

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
}

impl Device {
 
}



pub struct DeviceMap {
    device_ids: HashSet<DeviceId>,
    devices: HashMap<Arc<String>, Arc<()>>,
}

const OBJECT_ID_NODE: u32 = 0;
pub type DeviceId = u32;

impl DeviceMap {

    pub fn new(
    ) -> Self {
        Self {
            device_ids: HashSet::new(),
            devices: HashMap::new(),
        }
    }

    pub fn generate_device_id(&mut self, name: Arc<String>) -> DeviceId {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let mut id = hasher.finish() as DeviceId;
        while id == OBJECT_ID_NODE || self.device_ids.contains(&id) {
            id += 1;
        }
        self.device_ids.insert(id);
        id
    }

    pub fn remove_device_id(&mut self, id: DeviceId) {
        self.device_ids.remove(&id);
    }

    pub async fn add_device(
        &mut self,
        group_id: &str,
        node_id: &str,
        name: String,
        dev_impl: Box<DynDeviceMetricManager>,
        eon_state: Arc<EoNState>,
        client: Arc<DynClient>
    ) -> Result<DeviceHandle, DeviceRegistrationError> {

        if self.devices.get_key_value(&name).is_some() {
            return Err(DeviceRegistrationError::DuplicateDevice);
        }

        let name = Arc::new(name);
        let id = self.generate_device_id(name.clone());

        let ddata_topic = DeviceTopic::new(
            group_id,
            srad_types::topic::DeviceMessage::DData,
            node_id,
            &name,
        );

        let device = Arc::new(Device {
            info: DeviceInfo {
                id,
                name: name.clone(),
                ddata_topic,
            },
            birthed: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            eon_state,
            dev_impl,
            client
        });
        let handle = DeviceHandle {
            device: device.clone(),
        };
        device.dev_impl.init(&handle);
        self.devices.insert(name, device);
        Ok(handle)
    }

    pub async fn remove_device(&self, device: &String) {
        let dev = {
            let mut state = self.state.lock().await;
            let dev = match state.devices.remove(device) {
                Some(dev) => dev,
                None => return,
            };

            {
                let mut registry = self.registry.lock().unwrap();
                registry.remove_device_id(dev.info.id);
            }
            dev
        };
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
        if MessageKind::Cmd == message_kind {
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
    }
}
