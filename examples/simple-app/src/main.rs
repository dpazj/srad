use log::{info, LevelFilter};
use env_logger;
use srad::app::{App, SubscriptionConfig};
use srad::client_rumqtt as rumqtt;

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
        .on_nbirth(|id, _,timestamp, metrics| {
            info!("Node {id:?} born at {timestamp} metrics = {metrics:?}");
        })
        .on_ndeath(|id, _| {
            info!("Node {id:?} death");
        })
        .on_ndata(|id, _, timestamp, metrics| async move {
            info!("Node {id:?} data timestamp = {timestamp} metrics = {metrics:?}");
        })
        .on_dbirth(|id, dev, _, timestamp, metrics| {
            info!("Device {dev} Node {id:?} born at {timestamp} metrics = {metrics:?}");
        })
        .on_ddeath(|id, dev, _| {
            info!("Device {dev} Node {id:?} death");
        })
        .on_ddata(|id, dev, _, timestamp, metrics| async move {
            info!("Device {dev} Node {id:?} timestamp {timestamp} metrics = {metrics:?}");
        })
        .register_evaluate_rebirth_reason_fn(move |details| {
            let client= client.clone();
            async move {
                match details.reason {
                    srad::app::RebirthReason::MalformedPayload => return,
                    _ => ()
                }
                info!("Issuing rebirth request to node {0:?}, reason = {1:?}", details.node_id, details.reason);
                _ = client.publish_node_rebirth(&details.node_id.group, &details.node_id.node).await;
            }
        });
    application.run().await;
}
