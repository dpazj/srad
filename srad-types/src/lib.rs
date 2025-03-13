pub mod constants;
pub mod payload;
pub mod quality;
pub mod topic;
pub mod traits;
pub mod property_set;
mod value;

pub use value::*;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MetricId {
  Name(String),
  Alias(u64)  
}

