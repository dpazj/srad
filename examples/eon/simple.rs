use srad::{
    client_rumqtt as rumqtt,
    eon::{EoNBuilder, SimpleMetricManager},
};
use std::time::Duration;

use tokio::time;

use env_logger;
use log::LevelFilter;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
        .init();

    let opts = rumqtt::MqttOptions::new("node", "localhost", 1883);

    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let node_metrics = SimpleMetricManager::new();
    let counter_metric = node_metrics.register_metric("Counter", 0 as u64).unwrap();
    node_metrics.register_metric("A", 0 as u64).unwrap();

    let (mut eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("foo")
        .with_node_id("bar")
        .with_metric_manager(node_metrics.clone())
        .build()
        .unwrap();

    let dev1_metrics = SimpleMetricManager::new();
    let device1 = handle
        .register_device("dev1", dev1_metrics.clone())
        .await
        .unwrap();
    let dev1_counter = dev1_metrics
        .register_metric("Device Counter", 0 as i8)
        .unwrap();
    dev1_metrics
        .register_metric_with_cmd_handler("B", 0 as u64, |manager, metric, value| async move {
            _ = manager
                .publish_metric(metric.update(|x| *x = value.unwrap_or(0)))
                .await;
        })
        .unwrap();
    dev1_metrics
        .register_metric("U32Array", vec![1 as u32, 2, 3, 999999999, 43])
        .unwrap();
    dev1_metrics
        .register_metric(
            "BoolArray",
            vec![
                true, false, true, true, false, false, false, false, true, false, true,
            ],
        )
        .unwrap();
    dev1_metrics
        .register_metric(
            "StringArray",
            vec![
                "ABC".to_string(),
                "Easy".to_string(),
                "as".to_string(),
                "123".to_string(),
            ],
        )
        .unwrap();
    device1.enable().await;

    let node_manager = node_metrics.clone();
    let dev_manager = dev1_metrics.clone();
    tokio::spawn(async move {
        loop {
            _ = node_manager
                .publish_metric(counter_metric.update(|x| *x = x.wrapping_add(1)))
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
