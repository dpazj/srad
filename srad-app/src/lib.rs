//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library in rust.
//!
//! # Overview
//!
//! `srad-app` provides a generic [Application](generic_app::Application) implementation which supports:
//!
//!  - Node and Device state management
//!  - Message sequence reordering
//!  - Detecting situations where an Application may require a node to issue a rebirth message
//!
//! The generic application should be able to support the majority of use cases required from a Sparkplug application.
//!
//! For more specific custom implementations `srad-app` also provides a poll-able [AppEventLoop] and [AppClient] to be used as the base for implementing Sparkplug Applications.
//!
//! The EventLoop processes Sparkplug messages produced by a specified
//! [`srad-client`](https://crates.io/crates/srad-client) implementation. It provides:
//!  
//!  - Application topic subscription management
//!  - Application online state management
//!  - Payload validation and transformation to ergonomic types

mod config;
mod eventloop;
mod events;
mod metrics;

/// A module containing a resequencer utility used for reordering out of order messages
pub mod resequencer;

/// A module containing the generic Application implementation which users can register callbacks with and provide [generic_app::MetricStore] implementations to allow for custom app behaviour.
pub mod generic_app;

pub use config::*;
pub use eventloop::*;
pub use metrics::*;

pub mod generic;

/// Struct used to uniquely identify a node within a Sparkplug namespace
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct NodeIdentifier {
    pub group: String,
    pub node: String,
}
