use async_trait::async_trait;
use log::{debug, warn};
use rumqttc::v5::{mqttbytes::{v5::{ConnectProperties, Filter, Packet}, QoS}, AsyncClient as RuClient, EventLoop as RuEventLoop, MqttOptions as RuMqttOptions};
use srad_types::{payload::{Payload, Message}, topic::{DeviceTopic, TopicFilter}};

use srad_client::{Event, LastWill, topic_and_payload_to_event};

use crate::MqttOptions;

fn qos_to_mqtt_qos(qos: srad_types::topic::QoS) -> QoS {
  match qos {
    srad_types::topic::QoS::AtMostOnce => QoS::AtMostOnce,
    srad_types::topic::QoS::AtLeastOnce => QoS::AtLeastOnce,
    srad_types::topic::QoS::ExactlyOnce => QoS::ExactlyOnce,
  }
}

fn topic_filter_to_mqtt_filter(topic_filter: TopicFilter) -> Filter {
  Filter::new(topic_filter.topic, qos_to_mqtt_qos(topic_filter.qos))
}

#[derive(Clone)]
pub struct Client {
  client: RuClient
}

#[async_trait]
impl srad_client::Client for Client {

  async fn disconnect(&self) {
    self.client.disconnect().await.unwrap()
  }

  async fn publish_node_message(&self, topic: srad_types::topic::NodeTopic, payload: srad_types::payload::Payload) {
    debug!("Publish message: seq = {}, metric count = {}, topic = {}", payload.seq.unwrap_or(0), payload.metrics.len(), topic.topic);
    let (qos, retain) = topic.get_publish_quality_retain();
    self.client.publish(topic.topic, qos_to_mqtt_qos(qos), retain, payload.encode_to_vec()).await.unwrap()
  }

  async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload) {
    debug!("Publish message: seq = {}, metric count = {}, topic = {}", payload.seq.unwrap_or(0), payload.metrics.len(), topic.topic);
    let (qos, retain) = topic.get_publish_quality_retain();
    self.client.publish(topic.topic, qos_to_mqtt_qos(qos), retain, payload.encode_to_vec()).await.unwrap()
  }

  async fn subscribe_many(&self, topics: Vec<TopicFilter>) {
    debug!("Subscribe: topics = {:?}", topics);
    let filters: Vec<Filter> = topics.into_iter().map(|x| {topic_filter_to_mqtt_filter(x)}).collect();
    self.client.subscribe_many(filters).await.unwrap()
  }

}

pub struct EventLoop {
  el: RuEventLoop 
}

impl EventLoop {
  pub fn new(options: MqttOptions) -> (Self, Client) {
    
    let mut connect_properties = ConnectProperties::new();
    connect_properties.session_expiry_interval = Some(0);

    let mut options = RuMqttOptions::new(options.client_id, options.broker_addr, options.port);
    options
      .set_clean_start(true)
      .set_connect_properties(connect_properties);

    let (client, eventloop) = RuClient::new(options, 0);
    (EventLoop{el: eventloop}, Client{client})
  }
}

#[async_trait]
impl srad_client::EventLoop for EventLoop 
{
  async fn poll(&mut self) -> Option<Event> {
    let event = self.el.poll().await;
    match event {
      Ok(event) => {
        return match event {
          rumqttc::v5::Event::Incoming(Packet::ConnAck(_)) => Some(Event::Online),
          rumqttc::v5::Event::Incoming(Packet::Disconnect(_)) => Some(Event::Offline),
          rumqttc::v5::Event::Incoming(Packet::Publish(publish)) => {
            match topic_and_payload_to_event(&publish.topic, &publish.payload) {
              Ok(event) => Some(event),
              Err(e) => {
                warn!("Incoming publish was an invalid sparkplug message: {e:?}");
                Some(Event::InvalidPublish { reason: e, topic: publish.topic.into(), payload: publish.payload.into()})
              },
            }
          },
          _ => None
        }
      },
      Err(_) => {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        None
      },
    }
  }

  fn set_last_will(&mut self, will: LastWill) {
    let qos = qos_to_mqtt_qos(will.qos);
    let mqtt_will = rumqttc::v5::mqttbytes::v5::LastWill::new(will.topic, will.payload, qos, will.retain, None);
    self.el.options.set_last_will(mqtt_will);
  }
}
