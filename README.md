# srad

`srad` is a [Sparkplug](https://sparkplug.eclipse.org/) edge node and application development framework in Rust.

[![CI](https://github.com/dpazj/srad/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/dpazj/srad/actions/workflows/ci.yml?query=branch%3Amaster)
[![Documentation](https://docs.rs/srad/badge.svg)][docs]
[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Apache-2.0 licensed][apache-badge]][apache-url]

[crates-badge]: https://img.shields.io/crates/v/srad.svg
[crates-url]: https://crates.io/crates/srad
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/dpazj/srad/blob/master/LICENSE-MIT
[apache-badge]: https://img.shields.io/badge/License-Apache_2.0-blue.svg
[apache-url]: https://github.com/dpazj/srad/blob/master/LICENSE-APACHE

Additional information for this crate can be found in the [docs][docs].

## Overview

`srad` aims to make it easy as possible to build reliable, fast, and resource efficient Sparkplug B Edge Nodes and Applications with minimal overhead.

## Getting Started

### Examples

#### A simple Edge Node

```rust no_run
use srad::{client_rumqtt, eon::{EoN, EoNBuilder, NoMetricManager}};

#[tokio::main]
async fn main() {

    let opts = client_rumqtt::MqttOptions::new("foo:bar", "localhost", 1883);
    let (eventloop, client) = client_rumqtt::EventLoop::new(opts, 0);

    let (mut eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("foo")
        .with_node_id("bar")
        .with_metric_manager(NoMetricManager::new())
        .build().unwrap();

    eon.run().await;
}
```

#### A simple Application

```rust no_run
use srad::app::{SubscriptionConfig, generic_app::ApplicationBuilder};
use srad::client_rumqtt;

#[tokio::main]
async fn main() {
    let opts = client_rumqtt::MqttOptions::new("foo", "localhost", 1883);
    let (eventloop, client) = client_rumqtt::EventLoop::new(opts, 0);
    let (mut application, client) = ApplicationBuilder::new("foo", eventloop, client, SubscriptionConfig::AllGroups).build();
    application.run().await
}
```

More examples can be found in the [examples](./examples) and in the [docs][docs].

### Dependencies

[`codegen`](./codegen) uses `protoc` [Protocol Buffers compiler](https://protobuf.dev/downloads/) to generate types.

## Project Layout

- [`srad`](./README.md): Re-exports the `srad-*` crates under one package.
- [`srad-eon`](./srad-eon/README.md): SDK for building Sparkplug Edge Nodes.
- [`srad-app`](./srad-app/README.md): SDK for building Sparkplug Applications.
- [`srad-client`](./srad-client/README.md): Trait and type definitions for implementing clients to interact with Sparkplug.  
- [`srad-client-rumqtt`](./srad-client-rumqtt/README.md): Client implementation using [rumqtt](https://github.com/bytebeamio/rumqtt).
- [`srad-types`](./srad-types/README.md): Utility and Protobuf generated types.
- [`codegen`](./codegen): Generates types from protobuf files in [`protos`](./protos).
- [`examples`](./examples): Example Edge Node and application implementations.

## License

This project is dual licensed under the [MIT] and [APACHE] licenses.

[MIT]: https://github.com/dpazj/srad/blob/master/LICENSE-MIT
[APACHE]: https://github.com/dpazj/srad/blob/master/LICENSE-APACHE

[docs]: https://docs.rs/srad