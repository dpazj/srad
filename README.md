# Srad

>_Scottish Gaelic:_ **srad** /sdrad/ _vb._ (_v. n._ -adh) - **1.** _spark, emit sparks!_ **2.** _sparkle_

`srad` is an implementation of the [Sparkplug](https://sparkplug.eclipse.org/) specification providing a node/application development framework in Rust.

## High Level Features

- Async (tokio)
- No unsafe
- Pluggable MQTT client layer, with a standard implementation backed by [rumqtt](https://github.com/bytebeamio/rumqtt)
- Implements Sparkplug B specification

## Overview

- [srad](./srad/README.md) the general high level crate that most users will use. Mainly just re-exports the other crates under one crate.
- [srad-eon](./srad-eon/README.md) contains an SDK for building Sparkplug Edge Nodes.
- [srad-app](./srad-app/README.md) contains an SDK for building Sparkplug Applications.
- [srad-client](./srad-client/README.md) types and trait definitions for implementing clients to interact with Sparkplug  
- [srad-client-rumqtt](./srad-client-rumqtt/README.md) client implementation using [rumqtt](https://github.com/bytebeamio/rumqtt)
- [srad-types](./srad-types/README.md) contains common and Protobuf generated types, and some utilities

## Examples

### A simple Edge Node

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

### A simple Application

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

See [Examples](./examples) for more.

## License

This project is dual licensed under the [MIT] and [APACHE] licenses.

[MIT]: https://github.com/dpazj/srad/blob/master/LICENSE-MIT
[APACHE]: https://github.com/dpazj/srad/blob/master/LICENSE-APACHE
