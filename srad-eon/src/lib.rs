//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//! This library defines a framework for implementing Sparkplug Edge of Network Nodes.
//!
//! # Overview
//!
//! `srad-eon` provides a general implementation of a Sparkplug Node.
//!
//! The implementation requires the provision of [MetricManager] implementations to the node or when registering devices. This allows for
//! defining metrics which belong to the node or device as well as the custom handling of CMD messages for those metrics.
//!
//! The node starts a tokio `task` for itself node and each subsequent device. 
//! These tasks are used to process incoming state changes and messages from sparkplug topics such as CMD messages

mod birth;
mod builder;
mod device;
mod error;
mod metric;
mod metric_manager;
mod node;

pub use birth::{BirthInitializer, BirthMetricDetails};
pub use builder::EoNBuilder;
pub use device::DeviceHandle;
pub use metric::*;
pub use metric_manager::manager::{
    DeviceMetricManager, MetricManager, NoMetricManager, NodeMetricManager,
};
pub use metric_manager::simple::SimpleMetricManager;
pub use node::{EoN, NodeHandle};

#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum BirthType {
    Birth,
    Rebirth,
}
