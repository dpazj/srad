use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use srad_types::payload::DataType;
use srad_types::{traits, MetricId};
use crate::metric::MetricToken;

use super::error::SpgError;

const OBJECT_ID_NODE: u32 = 0;

pub type DeviceId = u32;

pub struct Registry{
  device_ids: HashMap<DeviceId, Arc<String>>,
}

enum AliasType {
  Node, 
  Device {id: DeviceId}
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

  pub fn remove_device(&mut self, id: DeviceId) {
    self.device_ids.remove(&id);
  }

}

pub enum MetricRegistryInserterType {
  Node, 
  Device {id : DeviceId, name: Arc<String>}
}

pub struct MetricRegistryInserter {
  metric_names: HashSet<String>,
  metric_aliases: HashSet<u64>,
  inserter_type: MetricRegistryInserterType,
}

impl MetricRegistryInserter {

  pub fn new(inserter_type: MetricRegistryInserterType) -> Self {
    Self {
      metric_names: HashSet::new(),
      metric_aliases: HashSet::new(),
      inserter_type: inserter_type,
    }
  }

  fn generate_alias(&mut self, alias_type: AliasType, metric_name: &String) -> u64 {
    let mut hasher = DefaultHasher::new();
    metric_name.hash(&mut hasher);
    let hash= hasher.finish() as u32;
    let id_part = match alias_type {
      AliasType::Node => 0,
      AliasType::Device { id } => id,
    };
    let mut alias = ((id_part as u64) << 32) | (hash as u64);
    while self.metric_aliases.contains(&alias) {
      alias += 1;
    }
    self.metric_aliases.insert(alias.clone());
    alias
  }

  pub fn register_metric<T: Into<String>, M: traits::MetricValue>(&mut self, name: T, use_alias: bool) -> Result<MetricToken<M>, SpgError> {
    let metric = name.into();

    if self.metric_names.contains(&metric) {
      return Err(SpgError::DuplicateMetric);
    }

    let id = match use_alias {
      true => {
        let alias = match &self.inserter_type {
          MetricRegistryInserterType::Node =>  self.generate_alias(AliasType::Node, &metric),
          MetricRegistryInserterType::Device { id , name: _} => self.generate_alias( AliasType::Device { id: id.clone() },&metric),
        };
        MetricId::Alias(alias)
      }
      false => MetricId::Name(metric)
    };

    Ok(MetricToken::new(id))
  }

}
