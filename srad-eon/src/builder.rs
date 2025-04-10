use std::sync::Arc;

use srad_client::{Client, DynClient, DynEventLoop, EventLoop};

use crate::{
    metric_manager::manager::{DynNodeMetricManager, NoMetricManager, NodeMetricManager},
    EoN, NodeHandle,
};

/// A builder for creating and configuring Edge of Network (EoN) instances.
pub struct EoNBuilder {
    pub(crate) group_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) eventloop_client: (Box<DynEventLoop>, Arc<DynClient>),
    pub(crate) metric_manager: Box<DynNodeMetricManager>,
}

impl EoNBuilder {
    /// Creates a new builder with the specified event loop and client.
    ///
    /// Initializes a builder with default values and a no-op metric manager.
    pub fn new<E: EventLoop + Send + 'static, C: Client + Send + Sync + 'static>(
        eventloop: E,
        client: C,
    ) -> Self {
        Self {
            group_id: None,
            node_id: None,
            eventloop_client: (Box::new(eventloop), Arc::new(client)),
            metric_manager: Box::new(NoMetricManager::new()),
        }
    }

    /// Sets the group ID for the EoN instance.
    ///
    /// The group ID identifies the group to which this node belongs.
    pub fn with_group_id<S: Into<String>>(mut self, group_id: S) -> Self {
        self.group_id = Some(group_id.into());
        self
    }

    /// Sets the node ID for the EoN instance.
    ///
    /// The node ID uniquely identifies this node within its group.
    pub fn with_node_id<S: Into<String>>(mut self, node_id: S) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Sets a custom metric manager for the EoN instance.
    ///
    /// Replaces the default no-op metric manager with the provided implementation.
    pub fn with_metric_manager<M: NodeMetricManager + Send + Sync + 'static>(
        mut self,
        metric_manager: M,
    ) -> Self {
        self.metric_manager = Box::new(metric_manager);
        self
    }

    /// Builds the EoN instance with the configured settings.
    ///
    /// Creates and returns a new EoN instance and its associated NodeHandle.
    /// This method will return an error if required configuration is missing
    /// or if there are other issues with the configuration.
    pub fn build(self) -> Result<(EoN, NodeHandle), String> {
        EoN::new_from_builder(self)
    }
}
