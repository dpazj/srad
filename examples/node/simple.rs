use srad::{
    client_rumqtt as rumqtt,
    eon::{EoNBuilder, SimpleMetricManager},
};
use std::time::Duration;

use tokio::time;

use log::LevelFilter;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
        .init();

    let opts = rumqtt::MqttOptions::new("node", "localhost", 1883);

    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let node_metrics = SimpleMetricManager::new();
    let node_counter = node_metrics.register_metric("Node Counter", 0_u64).unwrap();

    let (eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("foo")
        .with_node_id("bar")
        .with_metric_manager(node_metrics.clone())
        .build()
        .unwrap();

    let dev1_metrics = SimpleMetricManager::new();
    let device1 = handle
        .register_device("dev1", dev1_metrics.clone())
        .unwrap();
    let dev1_counter = dev1_metrics
        .register_metric("Device Counter", 0_i8)
        .unwrap();
    dev1_metrics
        .register_metric_with_cmd_handler(
            "Writeable UInt64",
            0_u64,
            |manager, metric, value| async move {
                _ = manager
                    .publish_metric(metric.update(|x| *x = value.unwrap_or(0)))
                    .await;
            },
        )
        .unwrap();

    device1.enable();

    let node_manager = node_metrics.clone();
    let dev_manager = dev1_metrics.clone();
    tokio::spawn(async move {
        loop {
            _ = node_manager
                .publish_metric(node_counter.update(|x| *x = x.wrapping_add(1)))
                .await;
            _ = dev_manager
                .publish_metric(dev1_counter.update(|x| *x = x.wrapping_sub(1)))
                .await;
            time::sleep(Duration::from_secs(1)).await;
        }
    });

    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            println!("Failed to register CTRL-C handler: {e}");
            return;
        }
        handle.cancel().await;
    });

    eon.run().await;
}
