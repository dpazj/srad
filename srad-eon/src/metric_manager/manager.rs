use async_trait::async_trait;

use crate::{birth::BirthInitializer, device::DeviceHandle, metric::MessageMetrics, NodeHandle};

/// A type alias for a trait object implementing [`NodeMetricManager`].
pub type DynNodeMetricManager = dyn NodeMetricManager + Send + Sync + 'static;
/// A type alias for a trait object implementing [`DeviceMetricManager`].
pub type DynDeviceMetricManager = dyn DeviceMetricManager + Send + Sync + 'static;

/// A trait for implementing a type that defines a collection of metrics.
///
/// This trait acts as an interface the library uses to query a type about
/// what metrics it has so that birth messages can be created.
///
/// # Examples
///
/// ```
/// use srad_eon::{MetricManager, BirthInitializer, BirthMetricDetails};
///
/// struct MyMetricManager {
///   counter: i32
/// };
///
/// impl MetricManager for MyMetricManager {
///     fn initialise_birth(&self, bi: &mut BirthInitializer) {
///         // Register metrics
///         bi.register_metric(BirthMetricDetails::new_with_initial_value("my_counter",  self.counter).use_alias(true));
///     }
/// }
pub trait MetricManager {
    /// Initialises the set of metrics to be included in a birth or rebirth message.
    ///
    /// This method is called whenever the library needs to generate a birth message payload for the node or device  
    /// that uses the type which implements MetricManager
    fn initialise_birth(&self, bi: &mut BirthInitializer);
}

///A trait for implementing a type that defines node-specific metrics
#[async_trait]
pub trait NodeMetricManager: MetricManager {
    /// Initialise the struct.
    ///
    /// Called when the Node is created
    fn init(&self, _handle: &NodeHandle) {}

    /// Processes NCMD metrics.
    ///
    /// This async method is called when a NCMD message for the node is received, allowing
    /// the implementation to handle command metrics as it sees fit. The default implementation does nothing.
    async fn on_ncmd(&self, _node: NodeHandle, _metrics: MessageMetrics) {}
}

///A trait for implementing a type that defines device-specific metrics
#[async_trait]
pub trait DeviceMetricManager: MetricManager {
    /// Initialise the struct.
    ///
    /// Called when the Device is created
    fn init(&self, _handle: &DeviceHandle) {}

    /// Processes DCMD metrics.
    ///
    /// This async method is called when a DCMD message for the device is received, allowing
    /// the implementation to handle command metrics as it sees fit. The default implementation does nothing.
    async fn on_dcmd(&self, _device: DeviceHandle, _metrics: MessageMetrics) {}
}

/// A no-op implementation [MetricManager] which will provide no metrics on Birth
/// and will do nothing when a CMD message is received.
pub struct NoMetricManager {}

impl NoMetricManager {
    /// Creates a new instance of `NoMetricManager`.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for NoMetricManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricManager for NoMetricManager {
    fn initialise_birth(&self, _: &mut BirthInitializer) {}
}
impl NodeMetricManager for NoMetricManager {}
impl DeviceMetricManager for NoMetricManager {}
