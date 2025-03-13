use srad::app::{App, SubscriptionConfig};
use srad::client::mqtt_client::rumqtt;

#[tokio::main(flavor="current_thread")]
async fn main() {

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts);
    let mut application = App::new("foo", SubscriptionConfig::AllGroups, eventloop, client);
    application
        .on_online(||{})
        .on_offline(||{})
        .on_nbirth(|id, timestamp, metrics| {})
        .on_ndeath(|id, timestamp| {})
        .on_ndata(|id, timestamp, metrics| async move {});

    application.run().await;
}
