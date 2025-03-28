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
    let (eventloop, client) = rumqtt::EventLoop::new(opts);
    let (mut application, client) = App::new("foo", SubscriptionConfig::AllGroups, eventloop, client);

    application
        .on_online(||{ println!("App online") })
        .on_offline(||{ println!("App offline") })
        .on_nbirth(|id, timestamp, metrics| {
            info!("Node {id:?} born at {timestamp}");
            for (birth, details) in metrics {
                info!("Metric: {:?}, Details: {:?}", birth, details);
            }
        })
        .on_ndeath(|id, timestamp| {
            info!("Node {id:?} death at {timestamp}");
        })
        .on_ndata(|id, timestamp, metrics| async move {
            info!("Node {id:?} timestamp {timestamp} metrics = {metrics:?}");
        })
        .on_dbirth(|id, dev, timestamp, metrics| {
            info!("Device {dev} Node {id:?} born at {timestamp}");
            for (birth, details) in metrics {
                info!("Metric: {:?}, Details: {:?}", birth, details);
            }
        })
        .on_ddeath(|id, dev, timestamp| {
            info!("Device {dev} Node {id:?} death at {timestamp}");
        })
        .on_ddata(|id, dev, timestamp, metrics| async move {
            info!("Device {dev} Node {id:?} timestamp {timestamp} metrics = {metrics:?}");
        })
        .register_evaluate_rebirth_reason_fn(move |rebirth_reason| {
            let client= client.clone();
            async move {
                let id = match rebirth_reason {
                    srad::app::RebirthReason::UnknownNode(node_identifier) => node_identifier,
                    srad::app::RebirthReason::UnknownDevice { node_id, device_id : _} => node_id,
                };
                info!("Issuing rebirth request to node {id:?}");
                _ = client.publish_node_rebirth(&id.group, &id.node_id).await;
            }
        });
    application.run().await;
}
