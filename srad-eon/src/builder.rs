use std::sync::Arc;

use srad_client::{Client, DynClient, DynEventLoop, EventLoop};

use crate::metric_manager::manager::{DynNodeMetricManager, NoMetricManager, NodeMetricManager};

pub struct EoNBuilder {
  pub(crate) group_id: Option<String>, 
  pub(crate) node_id: Option<String>,
  pub(crate) eventloop_client: (Box<DynEventLoop>, Arc<DynClient>),
  pub(crate) metric_manager: Box<DynNodeMetricManager>
}

impl EoNBuilder {

  pub fn new<E: EventLoop + Send + 'static, C : Client + Send + Sync + 'static>(eventloop: E, client: C) -> Self {
    Self {
      group_id: None,
      node_id: None,
      eventloop_client: (Box::new(eventloop), Arc::new(client)),
      metric_manager: Box::new(NoMetricManager::new())
    }
  }

  pub fn with_group_id<S: Into<String>>(mut self, group_id: S) -> Self {
    self.group_id = Some(group_id.into());
    self
  }

  pub fn with_node_id<S: Into<String>>(mut self, node_id: S) -> Self {
    self.node_id = Some(node_id.into());
    self
  }

  pub fn with_metric_manager<M: NodeMetricManager + Send + Sync + 'static>(mut self, metric_manager: M) -> Self {
    self.metric_manager = Box::new(metric_manager);
    self
  }

}
