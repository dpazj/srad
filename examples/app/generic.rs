use log::{info, LevelFilter};
use srad::app::{SubscriptionConfig, generic};
use srad::client_rumqtt as rumqtt;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let mut application = generic::Application::new("foo",eventloop, client, SubscriptionConfig::AllGroups);
    application.run().await;
}
