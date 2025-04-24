use log::{info, LevelFilter};
use srad::app::{App, SubscriptionConfig};
use srad::client_rumqtt as rumqtt;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
        .init();

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let (mut application, client) =
        App::new("foo", SubscriptionConfig::AllGroups, eventloop, client);

    let shutdown_handle = client.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            info!("Failed to register CTRL-C handler: {e}");
            return;
        }
        shutdown_handle.cancel().await;
    });

    loop {
        match application.poll().await {
            srad::app::AppEvent::Online => info!("App online"),
            srad::app::AppEvent::Offline => info!("App offline"),
            srad::app::AppEvent::NBirth(nbirth) => {
                info!(
                    "Node {:?} born at {} metrics = {:?}",
                    nbirth.id, nbirth.timestamp, nbirth.metrics_details
                )
            }
            srad::app::AppEvent::NDeath(ndeath) => {
                info!("Node {:?} death", ndeath.id);
            }
            srad::app::AppEvent::NData(ndata) => {
                info!(
                    "Node {:?} data at {} metrics = {:?}",
                    ndata.id, ndata.timestamp, ndata.metrics_details
                )
            }
            srad::app::AppEvent::DBirth(dbirth) => {
                info!(
                    "Device {} Node {:?} born at {} metrics = {:?}",
                    dbirth.device_name, dbirth.node_id, dbirth.timestamp, dbirth.metrics_details
                );
            }
            srad::app::AppEvent::DDeath(ddeath) => {
                info!(
                    "Device {} Node {:?} death at {}",
                    ddeath.device_name, ddeath.node_id, ddeath.timestamp
                )
            }
            srad::app::AppEvent::DData(ddata) => {}
            srad::app::AppEvent::RebirthReason(details) => {
                if let srad::app::RebirthReason::MalformedPayload = details.reason {
                    continue;
                }
                info!(
                    "Issuing rebirth request to node {0:?}, reason = {1:?}",
                    details.node_id, details.reason
                );
                let client = client.clone();
                tokio::task::spawn(async move {
                    _ = client
                        .publish_node_rebirth(&details.node_id.group, &details.node_id.node)
                        .await;
                });
            }
            srad::app::AppEvent::Disconnected => break,
        }
    }
}
