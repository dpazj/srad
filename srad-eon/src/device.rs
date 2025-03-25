use std::{collections::HashMap, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}};

use futures::future::join_all;
use srad_client::{DeviceMessage, DynClient, MessageKind};
use srad_types::{payload::{Payload, ToMetric}, topic::DeviceTopic, utils::timestamp};

use crate::{error::SpgError, metric::{MetricPublisher, PublishError, PublishMetric}, metric_manager::{birth::BirthInitializer, manager::DynDeviceMetricManager}, node::EoNState, registry::{self, DeviceId}, BirthType};

pub struct DeviceInfo {
  id: DeviceId,
  device_id: Arc<String>,
  ddata_topic: DeviceTopic 
}

#[derive(PartialEq)]
enum BirthState {
  UnBirthed,
  Birthed,
}

#[derive(Clone)]
pub struct DeviceHandle {
  device: Arc<Device>,
}

impl DeviceHandle {

  pub async fn enable(&self) {
    self.device.enabled.store(true, Ordering::SeqCst);
    self.device.birth(&BirthType::Birth).await;
  }

  pub async fn rebirth(&self) { 
    self.device.enabled.store(true, Ordering::SeqCst);
    self.device.birth(&BirthType::Rebirth).await;
  }

  pub async fn disable(&self) {
    if self.device.enabled.swap(false, Ordering::SeqCst) == false { 
      //already disabled
      return 
    };
    self.device.death(true).await
  }
}

impl MetricPublisher for DeviceHandle {
  async fn publish_metrics_unsorted(&self, metrics: Vec<PublishMetric>) -> Result<(), PublishError>{
    if metrics.len() == 0 { return Err(PublishError::NoMetrics) }

    match self.device.birth_state.try_lock() {
      Ok(state) => if BirthState::UnBirthed == *state { return Err(PublishError::UnBirthed)},
      Err(_) => return Err(PublishError::UnBirthed),
    }

    if !self.device.eon_state.is_online() { return Err(PublishError::Offline) }

    let timestamp = timestamp();

    let mut payload_metrics = Vec::with_capacity(metrics.len());
    for x in metrics.into_iter() {
      payload_metrics.push(x.to_metric());
    }

    let payload = Payload { 
      timestamp: Some(timestamp), 
      metrics: payload_metrics, 
      seq: Some(self.device.eon_state.get_seq()), 
      uuid: None, 
      body: None 
    };
    self.device.client.publish_device_message(self.device.info.ddata_topic.clone(), payload).await;
    Ok(())
  }
}

pub struct Device {
  info: DeviceInfo,
  birth_state: tokio::sync::Mutex<BirthState>,
  enabled: AtomicBool,
  eon_state: Arc<EoNState>,
  registry: Arc<Mutex<registry::Registry>>, 
  pub dev_impl: Arc<DynDeviceMetricManager>,
  client: Arc<DynClient>,
} 

impl Device {

  fn generate_birth_payload(&self) -> Payload {
    let mut reg = self.registry.lock().unwrap();
    let mut birth_initializer = BirthInitializer::new( registry::MetricRegistryInserterType::Device { id: self.info.id.clone(), name: self.info.device_id.clone() }, &mut reg);
    self.dev_impl.initialize_birth(&mut birth_initializer);
    let timestamp = timestamp();
    let metrics  = birth_initializer.finish();
  
    Payload {
      seq: Some(self.eon_state.get_seq()),
      timestamp: Some (timestamp),
      metrics: metrics, 
      uuid : None,
      body: None
    }
  }

  fn generate_death_payload(&self) -> Payload {
    let timestamp = timestamp();
    Payload {
      seq: Some(self.eon_state.get_seq()),
      timestamp: Some (timestamp),
      metrics: Vec::new(), 
      uuid : None,
      body: None
    }
  }

  pub async fn death(&self, publish: bool) {
    let mut state = self.birth_state.lock().await;
    if publish {
      let payload = self.generate_death_payload();
      self.client.publish_device_message(
        DeviceTopic::new(&self.eon_state.group_id, srad_types::topic::DeviceMessage::DDeath, &self.eon_state.edge_node_id, &self.info.device_id),
        payload 
      ).await;
    }
    *state = BirthState::UnBirthed;
  }

  pub async fn birth(&self, birth_type: &BirthType) {
    if !self.enabled.load(Ordering::SeqCst) { return }
    let mut state = self.birth_state.lock().await;
    if !self.eon_state.birthed() { return }
    if *birth_type == BirthType::Birth && *state == BirthState::Birthed { return }
    let payload = self.generate_birth_payload();
    self.client.publish_device_message(
      DeviceTopic::new(&self.eon_state.group_id, srad_types::topic::DeviceMessage::DBirth, &self.eon_state.edge_node_id, &self.info.device_id),
      payload 
    ).await;
    *state = BirthState::Birthed
  }
}

pub struct DeviceMapInner {
  devices: HashMap<Arc<String>, Arc<Device>>
}

pub struct DeviceMap {
  client: Arc<DynClient>,
  state: tokio::sync::Mutex<DeviceMapInner>,
  eon_state: Arc<EoNState>,
  registry: Arc<Mutex<registry::Registry>>, 
}

impl DeviceMap {

  pub fn new(eon_state: Arc<EoNState>, registry: Arc<Mutex<registry::Registry>>, client: Arc<DynClient>) -> Self {
    Self {
      eon_state,
      registry,
      client,
      state: tokio::sync::Mutex::new(DeviceMapInner{devices: HashMap::new()})
    }
  }

  pub async fn add_device(&self, group_id: &String, node_id: &String, name: String, dev_impl: Arc<DynDeviceMetricManager>) -> Result<DeviceHandle, SpgError>{
    
    let mut state= self.state.lock().await;
    if let Some(_) = state.devices.get_key_value(&name) {
      return Err(SpgError::DuplicateDevice);
    }

    let name = Arc::new(name);
    let mut registry= self.registry.lock().unwrap();
    let id = registry.generate_device_id(name.clone());
    drop(registry);

    let ddata_topic = DeviceTopic::new(&group_id, srad_types::topic::DeviceMessage::DData, &node_id, &name);

    let device = Arc::new(Device {
      info: DeviceInfo { id, device_id: name.clone(), ddata_topic: ddata_topic },
      birth_state: tokio::sync::Mutex::new(BirthState::UnBirthed),
      enabled: AtomicBool::new(false),
      eon_state: self.eon_state.clone(), 
      registry: self.registry.clone(),
      dev_impl,
      client: self.client.clone()
    });

    let handle = DeviceHandle { device: device.clone()};
    state.devices.insert(name, device);
    drop(state);

    Ok(handle)
  }

  pub async fn birth_devices(&self, birth_type: BirthType) {
    let device_map = self.state.lock().await;
    let futures: Vec<_> = device_map.devices.values()
      .map(|x| {
        x.birth(&birth_type)
      })
      .collect();
    join_all(futures).await;
  }

  pub async fn on_offline(&self) {
    let device_map = self.state.lock().await;
    let futures: Vec<_> = device_map.devices.values()
      .map(|x| {
        x.death(false) 
      })
      .collect();
    join_all(futures).await;
  }

  pub async fn handle_device_message(&self, message: DeviceMessage) {
    let dev = {
      let state = match self.state.try_lock() {
        Ok(state) => state,
        Err(_) => return,
      };

      let dev = state.devices.get(&message.device_id);
      match dev {
        Some(dev) => dev.clone(),
        None => return,
      }
    };

    let payload = message.message.payload;
    let message_kind = message.message.kind;
    match message_kind {
      MessageKind::Cmd => {
        let message_metrics= match payload.try_into() {
          Ok(metrics) => metrics,
          Err(_) => todo!(),
        };
        dev.dev_impl.on_dcmd(DeviceHandle { device: dev.clone() } ,message_metrics).await
      }
      _ => ()
    }
  }

}

