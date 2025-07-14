pub mod constants;

mod generated {
    pub(crate) mod sparkplug_payload;
}

/// generated types
pub mod payload;

pub mod topic;

mod property_set;
mod template;

pub mod utils;

mod metadata;
mod quality;
mod value;

pub use metadata::*;
pub use property_set::*;
pub use quality::*;
pub use template::*;
pub use value::*;

pub mod traits;

/// Represents a unique identifier of a metric
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MetricId {
    Name(String),
    Alias(u64),
}
