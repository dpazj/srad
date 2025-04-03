# Srad

A general purpose [Sparkplug](https://sparkplug.eclipse.org/) library in rust.

This library defines a framework for implementing Sparkplug Applications.

## Features

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

See [Examples](https://github.com/dpazj/srad/blob/master/examples) for more.