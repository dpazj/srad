use srad::{
    client_rumqtt as rumqtt,
    eon::{EoNBuilder, NoMetricManager, SimpleMetricManager},
};
use std::time::Duration;
use rand::Rng;

use log::LevelFilter;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    const NODE_COUNT: u32 = 10;
    const DEVICE_COUNT: u32 = 1000;
    const PER_DEVICE_METRIC_COUNT: u32 = 100;

    for i in 0..NODE_COUNT {
        let node_name = format!("node-{i}");
        let opts = rumqtt::MqttOptions::new(node_name, "localhost", 1883);
        let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);

        let (mut eon, handle) = EoNBuilder::new(eventloop, client)
            .with_group_id("iotech")
            .with_node_id(format!("node-{i}"))
            .with_metric_manager(NoMetricManager::new())
            .build()
            .unwrap();

        for j in 0..DEVICE_COUNT {
            let device_metrics = SimpleMetricManager::new();
            let mut metrics = Vec::with_capacity(PER_DEVICE_METRIC_COUNT as usize);
            for k in 0..PER_DEVICE_METRIC_COUNT {
                metrics.push(
                    device_metrics
                        .register_metric(format!("metric-{k}"), 0_u64)
                        .unwrap(),
                );
            }
            handle
                .register_device(format!("device-{j}"), device_metrics.clone())
                .await
                .unwrap()
                .enable()
                .await;


            tokio::spawn({
                let mut rng = rand::thread_rng();
                let millis = rng.gen_range(0..=1000);
                async move {
                    tokio::time::sleep(Duration::from_millis(millis)).await;
                    loop {
                        let publish = metrics.iter().map(|x| {
                                x.update(|x| *x = x.wrapping_add(1))
                        }).collect();
                        _ = device_metrics.publish_metrics(publish).await;

                        // for metric in &metrics {
                        //     _ = device_metrics
                        //         .publish_metric(metric.update(|x| *x = x.wrapping_add(1)))
                        //         .await;
                        // }

                        tokio::time::sleep(Duration::from_secs(1)).await
                    }
                }
            });
        }

        tokio::spawn(async move { eon.run().await });
    }

    tokio::signal::ctrl_c().await.unwrap();
}
