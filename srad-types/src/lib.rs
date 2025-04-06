pub mod constants;
pub mod payload;
pub mod quality;
pub mod topic;
pub mod traits;
pub mod property_set;
pub mod metadata;
pub mod utils;
mod value;

pub use value::*;

/// Represents a unique identifier of a metric 
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MetricId {
  Name(String),
  Alias(u64)  
}

