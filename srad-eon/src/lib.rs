//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//! This library defines a framework for implementing Sparkplug Edge of Network Nodes.

mod birth;
mod builder;
mod device;
mod error;
mod metric;
mod metric_manager;
mod node;
mod registry;

pub use birth::{BirthInitializer, BirthMetricDetails};
pub use builder::EoNBuilder;
pub use device::DeviceHandle;
pub use metric::*;
pub use metric_manager::manager::{
    DeviceMetricManager, MetricManager, NoMetricManager, NodeMetricManager,
};
pub use metric_manager::simple::SimpleMetricManager;
pub use node::{EoN, NodeHandle};

#[derive(Debug, PartialEq)]
pub(crate) enum BirthType {
    Birth,
    Rebirth,
}
