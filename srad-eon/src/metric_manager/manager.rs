use async_trait::async_trait;

use crate::{birth::BirthInitializer, device::DeviceHandle, metric::MessageMetrics, NodeHandle};

pub type DynNodeMetricManager = dyn NodeMetricManager + Send + Sync + 'static;
pub type DynDeviceMetricManager = dyn DeviceMetricManager + Send + Sync + 'static;

pub trait MetricManager {
  fn initialize_birth(&self, bi: &mut BirthInitializer);
}

#[async_trait]
pub trait NodeMetricManager : MetricManager {
  fn init(&self, _handle: &NodeHandle) {}
  async fn on_ncmd(&self, _node: NodeHandle, _metrics: MessageMetrics) {}
}

#[async_trait]
pub trait DeviceMetricManager : MetricManager {
  fn init(&self, _handle: &DeviceHandle) {}
  async fn on_dcmd(&self, _device: DeviceHandle, _metrics: MessageMetrics) {}
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
