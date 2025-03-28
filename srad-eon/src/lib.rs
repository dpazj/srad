mod node;
mod builder;
mod device;
mod birth;
mod metric;
mod registry;
mod error;
mod metric_manager;

use std::sync::atomic::AtomicU8;

pub use node::{EoN, NodeHandle};
pub use builder::EoNBuilder;
pub use metric_manager::simple::SimpleMetricManager;
pub use metric_manager::manager::NoMetricManager;


#[derive(PartialEq)]
pub(crate) enum BirthType {
  Birth, 
  Rebirth, 
}
