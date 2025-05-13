//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//! # Overview
//!
//! `srad-app` provides a poll-able EventLoop and Client to be used as the base for implementing Sparkplug Applications.
//!
//! The EventLoop processes Sparkplug messages produced by a specified
//! [`srad-client`](https://crates.io/crates/srad-client) implementation. It provides:
//!  
//!  - Application topic subscription management
//!  - Application online state management
//!  - Payload validation and transformation to ergonomic types
//!

mod app;
mod config;
mod events;
mod metrics;
mod resequencer;

mod generic;

pub use app::*;
pub use config::*;
pub use metrics::*;
pub use resequencer::*;

/// Used to uniquely identify a node
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct NodeIdentifier {
    pub group: String,
    pub node: String,
}
