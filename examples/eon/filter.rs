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

    const NODE_COUNT:u32 = 5;
    const DEVICE_COUNT:u32 = 5;
    const METRIC_COUNT:u32 = 5;

    for i in 0..NODE_COUNT {
        let node_name = format!("node-{i}");
        let opts = rumqtt::MqttOptions::new(node_name, "localhost", 1883);
        let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
        
        let node_metrics = SimpleMetricManager::new();
        let mut metric = None;
        for j in 0..METRIC_COUNT {
            metric = Some(node_metrics.register_metric(format!("metric-{j}"), 0_u64).unwrap());
        }

        let (mut eon, handle) = EoNBuilder::new(eventloop, client)
            .with_group_id("iotech")
            .with_node_id(format!("node-{i}"))
            .with_metric_manager(node_metrics.clone())
            .build()
            .unwrap();

        tokio::spawn({
            async move {
                let metric = metric.unwrap();
                loop {
                    _ = node_metrics.publish_metric(metric.update(|x| *x = x.wrapping_add(1))).await;
                    tokio::time::sleep(Duration::from_secs(1)).await
                }
            }
        });

        for j in 0..DEVICE_COUNT {

            let device_metrics = SimpleMetricManager::new();
            for k in 0..METRIC_COUNT {
                device_metrics.register_metric(format!("metric-{k}"), 0_u64).unwrap();
            }
            handle.register_device(format!("device-{j}"), device_metrics).await.unwrap().enable().await;
        }

        tokio::spawn(async move {
            eon.run().await
        });
    }

    tokio::signal::ctrl_c().await.unwrap(); 
}
