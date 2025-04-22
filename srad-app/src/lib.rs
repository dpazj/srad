//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//!This library defines a framework for implementing Sparkplug Applications.

//mod app;
mod app2;
mod events;
mod config;
mod metrics;

pub use app2::*;
pub use config::*;
pub use metrics::*;

/// Used to uniquely identify a node
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct NodeIdentifier {
    pub group: String,
    pub node: String,
}

