//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//! # Overview
//!
//! `srad-app` provides a poll-able EventLoop and Client for implementing Sparkplug Applications.
//!
//! The EventLoop processes Sparkplug messages produced by a specified
//! [`srad-client`](https://crates.io/crates/srad-client) implementation. It provides:
//!
//!  - Payload validation and transformation to ergonomic types
//!  - Message sequence validation and re-sequencing
//!  - Evaluation of conditions where an Application should issue a Rebirth
//!    - **NOTE**: *It does not evaluate if a metric received on a data topic was provided in a birth message*
//!

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
