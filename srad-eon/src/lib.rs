mod node;
mod builder;
mod device;
mod birth;
mod metric;
mod registry;
mod error;
mod metric_manager;

pub use node::{EoN, NodeHandle};
pub use builder::EoNBuilder;
pub use metric_manager::simple::SimpleMetricManager;
pub use metric_manager::manager::NoMetricManager;


#[derive(Debug, PartialEq)]
pub(crate) enum BirthType {
  Birth, 
  Rebirth, 
}
