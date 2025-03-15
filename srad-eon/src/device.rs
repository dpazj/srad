use std::{collections::HashMap, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}};

use futures::future::join_all;
use srad_client::{DeviceMessage, DynClient, MessageKind};
use srad_types::{payload::{Payload, ToMetric}, topic::DeviceTopic};

use crate::{error::SpgError, metric::{MetricPublisher, PublishMetric}, metric_manager::{birth::BirthInitializer, manager::DynDeviceMetricManager}, node::EoNState, registry::{self, DeviceId, MetricValidToken}, utils::{self, timestamp, BirthType}};

pub struct DeviceInfo {
  id: DeviceId,
  device_id: Arc<String>,
  ddata_topic: DeviceTopic 
}

#[derive(Clone)]
pub struct DeviceHandle {
  device: Arc<Device>,
}

impl DeviceHandle {

  pub async fn birth(&self) {
    self.device.can_birth.store(true, Ordering::SeqCst);
    self.device.birth(&BirthType::Birth).await;
  }

  pub async fn rebirth(&self) { 
    self.device.can_birth.store(true, Ordering::SeqCst);
    self.device.birth(&BirthType::Rebirth).await;
  }

  pub async fn death(&self) {
    self.device.can_birth.store(false, Ordering::SeqCst);
    self.device.death().await
  }
}

impl MetricPublisher for DeviceHandle {
  async fn publish_metrics_unsorted(&self, metrics: Vec<PublishMetric>) -> Result<(), ()>{
    if metrics.len() == 0 { return Err(()) }

    let timestamp = timestamp();
    let metric_ptr = {
      let metrics_valid_tok = self.device.metrics_valid_token.lock().unwrap();
      if !metrics_valid_tok.is_valid() {
        return Err(())
      }
      metrics_valid_tok.token_ptr()
    };

    let mut payload_metrics = Vec::with_capacity(metrics.len());
    for x in metrics.into_iter() {
      if *x.get_token_ptr() != metric_ptr { 
        return Err(()) 
      }
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
  birthed: AtomicBool,
  can_birth: AtomicBool,
  metrics_valid_token: Mutex<MetricValidToken>,
  eon_state: Arc<EoNState>,
  registry: Arc<Mutex<registry::MetricRegistry>>, 
  pub dev_impl: Arc<DynDeviceMetricManager>,
  client: Arc<DynClient>,
} 

impl Device {

  fn generate_birth_payload(&self) -> Payload {
    let mut reg = self.registry.lock().unwrap();
    let mut birth_initializer = BirthInitializer::new( registry::MetricRegistryInserterType::Device { id: self.info.id.clone(), name: self.info.device_id.clone() }, &mut reg);
    self.dev_impl.initialize_birth(&mut birth_initializer);
    let timestamp = utils::timestamp();
    let (metrics, metric_valid_token) = birth_initializer.finish();
    {
      let mut valid_token = self.metrics_valid_token.lock().unwrap();
      *valid_token = metric_valid_token 
    }
    Payload {
      seq: Some(self.eon_state.get_seq()),
      timestamp: Some (timestamp),
      metrics: metrics, 
      uuid : None,
      body: None
    }
  }

  fn generate_death_payload(&self) -> Payload {
    let timestamp = utils::timestamp();
    Payload {
      seq: Some(self.eon_state.get_seq()),
      timestamp: Some (timestamp),
      metrics: Vec::new(), 
      uuid : None,
      body: None
    }
  }

  pub async fn death(&self) {
    if self.birthed.swap(false, Ordering::SeqCst) == false { return }
    let payload = self.generate_death_payload();
    self.client.publish_device_message(
      DeviceTopic::new(&self.eon_state.group_id, srad_types::topic::DeviceMessage::DDeath, &self.eon_state.edge_node_id, &self.info.device_id),
      payload 
    ).await;
  }

  pub async fn birth(&self, birth_type: &BirthType) {
    if !self.can_birth.load(Ordering::SeqCst) { return }
    if !self.eon_state.is_online() { return }
    if *birth_type == BirthType::Birth && self.birthed.swap(true, Ordering::SeqCst) { return }
    let payload = self.generate_birth_payload();
    self.client.publish_device_message(
      DeviceTopic::new(&self.eon_state.group_id, srad_types::topic::DeviceMessage::DBirth, &self.eon_state.edge_node_id, &self.info.device_id),
      payload 
    ).await;
  }
}

pub struct DeviceMapInner {
  devices: HashMap<Arc<String>, Arc<Device>>
}

pub struct DeviceMap {
  client: Arc<DynClient>,
  state: Mutex<DeviceMapInner>,
  eon_state: Arc<EoNState>,
  registry: Arc<Mutex<registry::MetricRegistry>>, 
}

impl DeviceMap {

  pub fn new(eon_state: Arc<EoNState>, registry: Arc<Mutex<registry::MetricRegistry>>, client: Arc<DynClient>) -> Self {
    Self {
      eon_state,
      registry,
      client,
      state: Mutex::new(DeviceMapInner{devices: HashMap::new()})
    }
  }

  pub fn add_device(&self, group_id: &String, node_id: &String, name: String, dev_impl: Arc<DynDeviceMetricManager>) -> Result<DeviceHandle, SpgError>{
    
    let mut state= self.state.lock().unwrap();
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
      birthed: AtomicBool::new(false),
      can_birth: AtomicBool::new(false),
      metrics_valid_token: Mutex::new(MetricValidToken::new()),
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

  pub async fn birth_devices(&self, birth: BirthType) {
    let devices: Vec<_> = {
      let device_map = self.state.lock().unwrap();
      device_map.devices.iter().map(|(_, val)| { val.clone() }).collect()
    };

    let futures: Vec<_> = devices.iter()
      .map(|x| {
        x.birth(&birth)
      })
      .collect();
    join_all(futures).await;
  }

  pub fn death_devices(&self) {
    let device_map = self.state.lock().unwrap();
    device_map.devices.iter().for_each(|(_, x)| { x.birthed.store(false, Ordering::SeqCst); });
  }

  pub async fn handle_device_message(&self, message: DeviceMessage) {
    let dev = {
      let state= self.state.lock().unwrap();
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

