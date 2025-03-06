# Srad

**WIP**

>_Scottish Gaelic:_ **srad** /sdrad/ _vb._ (_v. n._ -adh) - **1.** _spark, emit sparks!_ **2.** _sparkle_

`srad` is an implementation of the [Sparkplug](https://sparkplug.eclipse.org/) specification providing a node/application development framework in Rust.

## High Level Features

- Async API
- No unsafe
- Pluggable MQTT client layer, with a standard implementation backed by [rumqtt](https://github.com/bytebeamio/rumqtt)

## Overview

- **srad** the general high level crate that most users will use. Mainly just re-exports the other crates under one crate.
- **srad-eon** contains an SDK for building Sparkplug Edge Nodes.
- **srad-app** contains an SDK for building Sparkplug Applications.
- **srad-client** types and trait definitions for implementing clients to interact with Sparkplug  
- **srad-client-rumqtt** client implementation using [rumqtt](https://github.com/bytebeamio/rumqtt)
- **srad-types** contains common and Protobuf generated types, and some utilities

## Examples

## License
