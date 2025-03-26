use async_trait::async_trait;

use crate::{device::DeviceHandle, metric::MessageMetrics, NodeHandle};

use super::birth::BirthInitializer;

pub type DynNodeMetricManager = dyn NodeMetricManager + Send + Sync + 'static;
pub type DynDeviceMetricManager = dyn DeviceMetricManager + Send + Sync + 'static;

pub trait MetricManager {
  fn initialize_birth(&self, bi: &mut BirthInitializer);
}

#[async_trait]
pub trait NodeMetricManager : MetricManager {
  fn init(&self, handle: &NodeHandle) {}
  async fn on_ncmd(&self, node: NodeHandle, metrics: MessageMetrics) {}
}

#[async_trait]
pub trait DeviceMetricManager : MetricManager {
  fn init(&self, handle: &DeviceHandle) {}
  async fn on_dcmd(&self, device: DeviceHandle, metrics: MessageMetrics) {}
}

pub struct NoMetricManager {}

impl NoMetricManager {
  pub fn new() -> Self {Self {}}
}

impl MetricManager for NoMetricManager {
  fn initialize_birth(&self, _: &mut BirthInitializer) {}
}

impl NodeMetricManager for NoMetricManager {

}

impl DeviceMetricManager for NoMetricManager {

}
