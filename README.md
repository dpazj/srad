# srad

`srad` is an async implementation of the [Sparkplug B](https://sparkplug.eclipse.org/) specification providing an edge node and application development framework in Rust.

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Apache-2.0 licensed][apache-badge]][apache-url]

[crates-badge]: https://img.shields.io/crates/v/srad.svg
[crates-url]: https://crates.io/crates/srad
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/dpazj/srad/blob/master/LICENSE-MIT
[apache-badge]: https://img.shields.io/badge/License-Apache_2.0-blue.svg
[apache-url]: https://github.com/dpazj/srad/blob/master/LICENSE-APACHE

## Overview

Srad aims to make it easy as possible to build reliable, fast, and resource efficient Sparkplug B Edge Nodes and Applications with minimal overhead.

## Getting Started

### Examples

#### A simple Edge Node

```rust no_run
use srad::{client::mqtt_client::rumqtt, eon::{EoN, EoNBuilder, NoMetricManager}};

#[tokio::main(flavor="current_thread")]
async fn main() {

    let opts = rumqtt::MqttOptions::new("foo:bar", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);

    let (mut eon, handle) = EoN::new_from_builder(
        EoNBuilder::new(eventloop, client)
            .with_group_id("foo")
            .with_node_id("bar")
            .with_metric_manager(NoMetricManager::new())
    ).unwrap();

    eon.run().await;
}
```

#### A simple Application

```rust no_run
use srad::app::{App, SubscriptionConfig};
use srad::client::mqtt_client::rumqtt;

#[tokio::main(flavor="current_thread")]
async fn main() {
    let opts = rumqtt::MqttOptions::new("foo", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let (mut application, client) = App::new("foo", SubscriptionConfig::AllGroups, eventloop, client);

    application
        .on_online(||{ println!("App online") })
        .on_offline(||{ println!("App offline") })
        .on_nbirth(|id, _,timestamp, metrics| { println!("Node {id:?} born at {timestamp} metrics = {metrics:?}"); })
        .on_ndeath(|id, _| { println!("Node {id:?} death"); })
        .on_ndata(|id, _, timestamp, metrics| async move { 
          println!("Node {id:?} data timestamp = {timestamp} metrics = {metrics:?}");
        })
        .on_dbirth(|id, dev, _, timestamp, metrics| { println!("Device {dev} Node {id:?} born at {timestamp} metrics = {metrics:?}");})
        .on_ddeath(|id, dev, _| { println!("Device {dev} Node {id:?} death"); })
        .on_ddata(|id, dev, _, timestamp, metrics| async move {
            println!("Device {dev} Node {id:?} timestamp {timestamp} metrics = {metrics:?}");
        });
    application.run().await;
}

```

More examples can be found in [Examples](./examples).

### Dependencies

[`srad-types`](./srad-types) uses `protoc` [Protocol Buffers compiler](https://protobuf.dev/downloads/) to generate types.

## Project Layout

- [`srad`](./README.md): Re-exports the following crates under one package.
- [`srad-eon`](./srad-eon/README.md): SDK for building Sparkplug Edge Nodes.
- [`srad-app`](./srad-app/README.md): SDK for building Sparkplug Applications.
- [`srad-client`](./srad-client/README.md): Trait and type definitions for implementing clients to interact with Sparkplug.  
- [`srad-client-rumqtt`](./srad-client-rumqtt/README.md): Client implementation using [rumqtt](https://github.com/bytebeamio/rumqtt).
- [`srad-types`](./srad-types/README.md): Utility and Protobuf generated types.
- [`examples`](./examples): Example Edge Node and application implementations.

## License

This project is dual licensed under the [MIT] and [APACHE] licenses.

[MIT]: https://github.com/dpazj/srad/blob/master/LICENSE-MIT
[APACHE]: https://github.com/dpazj/srad/blob/master/LICENSE-APACHE
