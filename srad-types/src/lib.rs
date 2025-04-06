pub mod constants;
pub mod payload;
pub mod topic;

mod property_set;

pub mod utils;

mod value;
mod quality;
mod metadata;

pub use metadata::*;
pub use property_set::*;
pub use quality::*;
pub use value::*;

pub mod traits;

/// Represents a unique identifier of a metric 
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MetricId {
  Name(String),
  Alias(u64)  
}

