//! Part of [srad](https://crates.io/crates/srad), a general purpose [Sparkplug](https://sparkplug.eclipse.org/) development library.
//! 
//! This library defines traits and types used to implement Sparkplug clients.
//! 
//! # Feature Flags
//! 
//! - `channel-client`: Enables the channel based [EventLoop] and [Client] implementation. Disabled by default.
//! 
 
mod traits;
mod types;
mod utils;

pub use utils::topic_and_payload_to_event;
pub use traits::{Client, DynClient, EventLoop, DynEventLoop};
pub use types::*;

/// A basic [EventLoop] and [Client] implementation based on channels
/// 
/// Useful for writing tests where it is not appropriate to be running a real MQTT client and broker setup
#[cfg(any(feature = "channel-client", doc))]
pub mod channel;
