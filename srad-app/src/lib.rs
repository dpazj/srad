//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//!This library defines a framework for implementing Sparkplug Applications.

mod app;
mod config;
mod events;
mod metrics;

pub use app::*;
pub use config::*;
pub use metrics::*;

/// Used to uniquely identify a node
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct NodeIdentifier {
    pub group: String,
    pub node: String,
}
