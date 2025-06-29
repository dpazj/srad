use srad::{
    client_rumqtt as rumqtt,
    eon::{EoNBuilder, NoMetricManager, SimpleMetricBuilder, SimpleMetricManager},
};
use std::time::Duration;

use log::LevelFilter;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    const NODE_COUNT: u32 = 5;
    const DEVICE_COUNT: u32 = 15;
    const PER_DEVICE_METRIC_COUNT: u32 = 25;

    let mut node_handles = Vec::with_capacity(NODE_COUNT as usize);

    for i in 0..NODE_COUNT {
        let node_name = format!("node-{i}");
        let opts = rumqtt::MqttOptions::new(node_name.clone(), "localhost", 1883);
        let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);

        let (eon, handle) = EoNBuilder::new(eventloop, client)
            .with_group_id("foo")
            .with_node_id(node_name)
            .with_metric_manager(NoMetricManager::new())
            .build()
            .unwrap();

        for j in 0..DEVICE_COUNT {
            let device_metrics = SimpleMetricManager::new();
            let mut metrics = Vec::with_capacity(PER_DEVICE_METRIC_COUNT as usize);
            for k in 0..PER_DEVICE_METRIC_COUNT {
                metrics.push(
                    device_metrics
                        .register_metric(SimpleMetricBuilder::new(format!("metric-{k}"), 0_u64))
                        .unwrap(),
                );
            }
            handle
                .register_device(format!("device-{j}"), device_metrics.clone())
                .unwrap()
                .enable();

            tokio::spawn({
                async move {
                    loop {
                        for metric in &metrics {
                            _ = device_metrics
                                .publish_metric(metric.update(|x| *x = x.wrapping_add(1)))
                                .await;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await
                    }
                }
            });
        }
        let run_handle = tokio::spawn(async move { eon.run().await });
        node_handles.push((handle.clone(), run_handle));
    }

    tokio::signal::ctrl_c().await.unwrap();

    for (node_handle, run_handle) in node_handles {
        node_handle.cancel().await;
        run_handle.await.unwrap();
    }
}
