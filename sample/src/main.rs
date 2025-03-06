use std::time::Duration;
use srad::{client::mqtt_client::rumqtt, eon::{self, EoNBuilder, SimpleMetricManager}};

use tokio::time;

#[tokio::main(flavor="current_thread")]
async fn main() {
    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts);
    let node_metrics = SimpleMetricManager::new();
    let counter_metric = node_metrics.register_metric("Counter", 0 as u64).unwrap();
    node_metrics.register_metric("A", 0 as u64).unwrap();

    let (mut eon, handle)  = eon::EoN::new_from_builder(
        EoNBuilder::new(eventloop, client)
            .with_group_id("foo")
            .with_node_id("bar")
            .with_metric_manager(node_metrics.clone())
    ).unwrap();

    let dev1_metrics = SimpleMetricManager::new();
    let dev1_counter = dev1_metrics.register_metric("Device Counter", 0 as i8).unwrap();
    dev1_metrics.register_metric_with_cmd_handler("B", 0 as u64, |handle, metric, value| async move {
        println! ("Got CMD for B with value {value:?}");
        metric.update_and_publish(|x|{ *x = value.unwrap_or(0)}, &handle).await;
    }).unwrap();
    dev1_metrics.register_metric("U32Array", vec![1 as u32, 2, 3, 999999999, 43]).unwrap();
    dev1_metrics.register_metric("BoolArray", vec![true, false, true, true, false, false, false, false, true, false, true]).unwrap();
    dev1_metrics.register_metric("StringArray", vec!["ABC".to_string(), "Easy".to_string(), "as".to_string(), "123".to_string()]).unwrap();
    let device1 = handle.register_device("dev1", dev1_metrics).await.unwrap();


    let node= handle.clone();
    tokio::spawn(async move {
        loop {
            counter_metric.update_and_publish(|x| { *x = x.wrapping_add(1) }, &node).await;
            dev1_counter.update_and_publish(|x| { *x = x.wrapping_sub(1) }, &device1).await;
            time::sleep(Duration::from_secs(1)).await;
        }
    });

    let handle_c = handle.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            println!("Failed to register CTRL-C handler: {e}");
            return;
        }
        handle_c.cancel().await;
    });

    eon.run().await;
}
