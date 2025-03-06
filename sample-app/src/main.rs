use srad::client::mqtt_client::rumqtt;
use srad::client::{Event, Client, EventLoop};
use srad::topic::{TopicFilter, Topic, QoS};
use tokio::task;

#[tokio::main(flavor="current_thread")]
async fn main() {

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (mut eventloop, client) = rumqtt::EventLoop::new(opts);

    loop {
        match eventloop.poll().await {
           Some(event) => {
                println!("Event = {event:?}");
                match event {
                    Event::Online => {
                        let c = client.clone();
                        task::spawn(async move {
                            c.subscribe(TopicFilter { topic: Topic::Namespace, qos: QoS::AtMostOnce}).await
                        });
                    },
                    _ => ()
                }
           },
           None => (), 
        }
    }


}
