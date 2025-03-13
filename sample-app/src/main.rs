use srad::app::{App, SubscriptionConfig};
use srad::client::mqtt_client::rumqtt;

#[tokio::main(flavor="current_thread")]
async fn main() {

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts);
    let mut application = App::new("foo", SubscriptionConfig::AllGroups, eventloop, client);
    application
        .on_online(||{ println!("App online") })
        .on_offline(||{ println!("App offline") })
        .on_nbirth(|id, timestamp, metrics| {
            println!("Node {id:?} born at {timestamp}");
            for (birth, details) in metrics {
                println!("Metric: {:?}, Details: {:?}", birth, details);
            }
        })
        .on_ndeath(|id, timestamp| {
            println!("Node {id:?} death at {timestamp}");
        })
        .on_ndata(|id, timestamp, metrics| async move {
            println!("Node {id:?} data at {timestamp}");
            for (id, details) in metrics {
                println!("Metric: {:?}, Details: {:?}", id, details);
            }
        });

    application.run().await;
}
