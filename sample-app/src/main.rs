use log::{info, LevelFilter};
use env_logger;
use srad::app::{App, SubscriptionConfig};
use srad::client::mqtt_client::rumqtt;

#[tokio::main(flavor="current_thread")]
async fn main() {

    env_logger::Builder::new()
    .filter_level(LevelFilter::Trace)
    .init();

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let (mut application, client) = App::new("foo", SubscriptionConfig::AllGroups, eventloop, client);

    let shutdown_handle = client.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            info!("Failed to register CTRL-C handler: {e}");
            return;
        }
        shutdown_handle.cancel().await;
    });

    application
        .on_online(||{ info!("App online") })
        .on_offline(||{ info!("App offline") })
        .on_nbirth(|id, timestamp, metrics| {
            info!("Node {id:?} born at {timestamp} metrics = {metrics:?}");
        })
        .on_ndeath(|id, timestamp| {
            info!("Node {id:?} death at {timestamp}");
        })
        .on_ndata(|id, timestamp, metrics| async move {
            info!("Node {id:?} data timestamp = {timestamp} metrics = {metrics:?}");
        })
        .on_dbirth(|id, dev, timestamp, metrics| {
            info!("Device {dev} Node {id:?} born at {timestamp} metrics = {metrics:?}");
        })
        .on_ddeath(|id, dev, timestamp| {
            info!("Device {dev} Node {id:?} death at {timestamp}");
        })
        .on_ddata(|id, dev, timestamp, metrics| async move {
            info!("Device {dev} Node {id:?} timestamp {timestamp} metrics = {metrics:?}");
        })
        .register_evaluate_rebirth_reason_fn(move |reason| {
            let client= client.clone();
            async move {
                info!("Issuing rebirth request to node {0:?}, reason = {1:?}", reason.node_id, reason.reason);
                _ = client.publish_node_rebirth(&reason.node_id.group, &reason.node_id.node).await;
            }
        });
    application.run().await;
}
