//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//! 
//! This library defines a framework for implementing Sparkplug Edge of Network Nodes.

mod node;
mod builder;
mod device;
mod birth;
mod metric;
mod registry;
mod error;
mod metric_manager;

pub use node::{EoN, NodeHandle};
pub use device::DeviceHandle;
pub use builder::EoNBuilder;
pub use metric_manager::simple::SimpleMetricManager;
pub use metric_manager::manager::{MetricManager, DeviceMetricManager, NodeMetricManager, NoMetricManager};
pub use birth::{BirthInitializer, BirthMetricDetails};
pub use metric::*;


#[derive(Debug, PartialEq)]
pub(crate) enum BirthType {
  Birth, 
  Rebirth, 
}
