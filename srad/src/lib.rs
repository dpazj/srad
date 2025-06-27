//! `srad` is a [Sparkplug](https://sparkplug.eclipse.org/) edge node and application development framework in Rust.
//!
//! # Overview
//!
//! `srad` aims to make it easy as possible to build reliable, fast, and resource efficient Sparkplug B Edge Nodes and Applications with minimal overhead.  
//!
//! This is a convenient crate that re-exports the other `srad` crates. See those crates for docs etc.
//!
//! # Example
//!  
//! Some super simple "Hello, World!" examples:
//!
//! ## Edge Node
//!
//! ```rust no_run
//! use srad::{client_rumqtt, eon::{EoN, EoNBuilder, NoMetricManager}};
//!
//! #[tokio::main]
//! async fn main() {
//!
//!     let opts = client_rumqtt::MqttOptions::new("foo:bar", "localhost", 1883);
//!     let (eventloop, client) = client_rumqtt::EventLoop::new(opts, 0);
//!
//!     let (mut eon, handle) = EoNBuilder::new(eventloop, client)
//!         .with_group_id("foo")
//!         .with_node_id("bar")
//!         .with_metric_manager(NoMetricManager::new())
//!         .build().unwrap();
//!
//!     eon.run().await;
//! }
//! ```
//!
//! ## Application
//!
//! ```rust no_run
//! use srad::app::{SubscriptionConfig, generic_app::ApplicationBuilder};
//! use srad::client_rumqtt;
//!
//! #[tokio::main]
//! async fn main() {
//!     let opts = client_rumqtt::MqttOptions::new("foo", "localhost", 1883);
//!     let (eventloop, client) = client_rumqtt::EventLoop::new(opts, 0);
//!     let (mut application, client) = ApplicationBuilder::new("foo", eventloop, client, SubscriptionConfig::AllGroups).build();
//!     application.run().await
//! }
//! ```
//!
//! # Examples
//!
//! The `srad` repo contains [a number of examples](https://github.com/dpazj/srad/tree/master/examples) that can help get you started.
//!
//! # Feature Flags
//!
//! - `eon`: Enables the Edge Node SDK. Enabled by default.
//! - `app`: Enables the Application SDK. Enabled by default.
//! - `rumqtt-client`: Enables the Rumqtt client implementation. Enabled by default.
//!

#[cfg(feature = "eon")]
pub use srad_eon as eon;

#[cfg(feature = "app")]
pub use srad_app as app;

pub use srad_client as client;

#[cfg(feature = "rumqtt-client")]
pub use srad_client_rumqtt as client_rumqtt;

pub use srad_types as types;
