use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

const OBJECT_ID_NODE: u32 = 0;

pub type DeviceId = u32;

pub struct Registry{
  device_ids: HashMap<DeviceId, Arc<String>>,
}

impl Registry{

  pub fn new() -> Self {
    Self {
      device_ids: HashMap::new()
    }
  }

  pub fn generate_device_id(&mut self, name: Arc<String>) -> DeviceId {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let mut id = hasher.finish() as DeviceId;
    while id == OBJECT_ID_NODE || self.device_ids.contains_key(&id) {
      id += 1;
    }
    self.device_ids.insert(id.clone(), name);
    id
  }

  pub fn remove_device_id(&mut self, id: DeviceId) {
    self.device_ids.remove(&id);
  }

}

