use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use srad_types::payload::DataType;
use srad_types::{traits, MetricId};
use crate::metric::MetricToken;

use super::error::SpgError;

const OBJECT_ID_NODE: u32 = 0;

#[derive(Clone)]
pub struct MetricValidToken(Arc<AtomicBool>);

#[derive(PartialEq, Debug)]
pub struct MetricValidTokenPtr(usize);

impl MetricValidToken {

  pub(crate) fn new() -> Self {
    MetricValidToken(Arc::new(AtomicBool::new(true))) 
  }

  fn invalidate(&self) {
    self.0.store(false, std::sync::atomic::Ordering::SeqCst);
  }

  pub(crate) fn is_valid(&self) -> bool {
    self.0.load( std::sync::atomic::Ordering::SeqCst)
  }

  pub(crate) fn token_ptr(&self) -> MetricValidTokenPtr {
    MetricValidTokenPtr(Arc::as_ptr(&self.0) as usize)
  }
 
}

pub type DeviceId = u32;

pub struct MetricRegistry{
  device_ids: HashMap<DeviceId, Arc<String>>,
  node_metrics_token: MetricValidToken, 
  device_metrics_tokens : HashMap<Arc<String>, MetricValidToken>
}

enum AliasType {
  Node, 
  Device {id: DeviceId}
}

impl MetricRegistry{

  pub fn new() -> Self {
    Self {
      device_ids: HashMap::new(),
      node_metrics_token: MetricValidToken::new(),
      device_metrics_tokens: HashMap::new(),
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

  fn remove_device_metrics(&mut self) {
    self.device_metrics_tokens.iter().for_each(|(_, token)| {
      token.invalidate();
    });
    self.device_metrics_tokens.clear();
  }

  fn remove_node_metrics(&mut self) {
    self.node_metrics_token.invalidate();
  }

  pub fn clear(&mut self){
    self.remove_device_metrics();
    self.remove_node_metrics();
  }

  fn remove_device(&mut self, id: DeviceId) {
    let name = match self.device_ids.remove(&id) {
      Some(v) => v,
      None => return
    };
    let device_metrics = match self.device_metrics_tokens.remove(&name) {
      Some(metrics) => metrics,
      None => return,
    };
    device_metrics.invalidate();
  }

}

pub enum MetricRegistryInserterType {
  Node, 
  Device {id : DeviceId, name: Arc<String>}
}

pub struct MetricRegistryInserter<'a> {
  metric_names: HashSet<String>,
  metric_aliases: HashSet<u64>,
  registry: &'a mut MetricRegistry,
  inserter_type: MetricRegistryInserterType,
  metrics_valid_token: MetricValidToken 
}

impl<'a> MetricRegistryInserter<'a> {

  pub fn new(inserter_type: MetricRegistryInserterType, registry: &'a mut MetricRegistry) -> Self {
    Self {
      metric_names: HashSet::new(),
      metric_aliases: HashSet::new(),
      registry,
      inserter_type: inserter_type,
      metrics_valid_token: MetricValidToken::new() 
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

  pub fn register_metric<T: Into<String>, M: traits::MetricValue>(&mut self, name: T, datatype: DataType, use_alias: bool) -> Result<MetricToken<M>, SpgError> {
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

    Ok(MetricToken::new(id, self.metrics_valid_token.clone()))
  }

  pub fn finish(self) -> MetricValidToken {
    let token = self.metrics_valid_token.clone();
    match self.inserter_type {
      MetricRegistryInserterType::Node => self.registry.node_metrics_token = self.metrics_valid_token, 
      MetricRegistryInserterType::Device { id: _ , name } => {
        self.registry.device_metrics_tokens.insert(name.clone(), self.metrics_valid_token);
      } 
    }
    token
  }

}

#[cfg(test)]
mod tests {
  mod metric_valid_token {
    use crate::registry::MetricValidToken;

    #[test]
    fn test_token_ptr() {
      let tok1 = MetricValidToken::new();
      let tok1_cpy = tok1.clone();

      assert_eq!(tok1.token_ptr(), tok1_cpy.token_ptr());

      let tok2 = MetricValidToken::new();
      assert_ne!(tok1.token_ptr(), tok2.token_ptr());
    }

    #[test]
    fn test_invalidate() {
      let tok1 = MetricValidToken::new();
      let tok1_cpy = tok1.clone();
      assert!(tok1.is_valid());
      assert!(tok1_cpy.is_valid());

      tok1.invalidate();
      assert!(tok1.is_valid() == false);
      assert!(tok1_cpy.is_valid() == false);
    }
  }
}

