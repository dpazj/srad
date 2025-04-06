use async_trait::async_trait;
use log::{error, trace};
use rumqttc::{v5::{mqttbytes::{v5::{ConnectProperties, Filter, Packet}, QoS}, AsyncClient as RuClient, EventLoop as RuEventLoop, MqttOptions}, Outgoing};
use srad_types::{payload::{Message, Payload}, topic::{DeviceTopic, StateTopic, TopicFilter}};

use srad_client::{topic_and_payload_to_event, Event, LastWill, StatePayload};

fn qos_to_mqtt_qos(qos: srad_types::topic::QoS) -> QoS {
  match qos {
    srad_types::topic::QoS::AtMostOnce => QoS::AtMostOnce,
    srad_types::topic::QoS::AtLeastOnce => QoS::AtLeastOnce
  }
}

fn topic_filter_to_mqtt_filter(topic_filter: TopicFilter) -> Filter {
  Filter::new(topic_filter.topic, qos_to_mqtt_qos(topic_filter.qos))
}

/// A [srad_client::Client] implementation using [rumqttc]
#[derive(Clone)]
pub struct Client {
  client: RuClient
}

impl Client {

  async fn publish(&self, topic: String, qos: QoS, retain: bool, payload: Vec<u8>) -> Result<(), ()> {
    match self.client.publish(topic, qos, retain, payload).await {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }
  
  fn try_publish(&self, topic: String, qos: QoS, retain: bool, payload: Vec<u8>) -> Result<(), ()> {
    match self.client.try_publish(topic, qos, retain, payload) {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }
}

#[async_trait]
impl srad_client::Client for Client {

  async fn disconnect(&self) -> Result<(),()> {
    match self.client.disconnect().await {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }

  async fn publish_state_message(&self, topic: StateTopic, payload: StatePayload) -> Result<(),()> {
    let (qos, retain) = payload.get_publish_quality_retain();
    self.publish(topic.topic, qos_to_mqtt_qos(qos), retain, Vec::<u8>::from(payload)).await
  }

  async fn try_publish_state_message(&self, topic: StateTopic, payload: StatePayload) -> Result<(),()> {
    let (qos, retain) = payload.get_publish_quality_retain();
    self.try_publish(topic.topic, qos_to_mqtt_qos(qos), retain, Vec::<u8>::from(payload))
  }

  async fn publish_node_message(&self, topic: srad_types::topic::NodeTopic, payload: srad_types::payload::Payload) -> Result<(),()> {
    let (qos, retain) = topic.get_publish_quality_retain();
    self.publish(topic.topic, qos_to_mqtt_qos(qos), retain, payload.encode_to_vec()).await
  }

  async fn try_publish_node_message(&self, topic: srad_types::topic::NodeTopic, payload: srad_types::payload::Payload) -> Result<(),()> {
    let (qos, retain) = topic.get_publish_quality_retain();
    self.try_publish(topic.topic, qos_to_mqtt_qos(qos), retain, payload.encode_to_vec())
  }

  async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(),()> {
    let (qos, retain) = topic.get_publish_quality_retain();
    self.publish(topic.topic, qos_to_mqtt_qos(qos), retain, payload.encode_to_vec()).await
  }

  async fn try_publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(),()> {
    let (qos, retain) = topic.get_publish_quality_retain();
    self.try_publish(topic.topic, qos_to_mqtt_qos(qos), retain, payload.encode_to_vec())
  }

  async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(),()> {
    let filters: Vec<Filter> = topics.into_iter().map(|x| {topic_filter_to_mqtt_filter(x)}).collect();
    match self.client.subscribe_many(filters).await {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }

}

enum ConnectionState {
  Disconnected,
  ManualDisconnected,
  Connected,
}

/// An [srad_client::EventLoop] implementation using [rumqttc]
pub struct EventLoop {
  state: ConnectionState,
  el: RuEventLoop 
}

impl EventLoop {

  /// Create a new `Eventloop`.
  /// 
  /// `options` are the mqtt options to create the rumqtt client with. Some options will be overwritten to ensure Sparkplug compliance.
  /// 
  /// `cap` specifies the capacity of the bounded async channel for the client handle.
  pub fn new(options: MqttOptions, cap: usize) -> (Self, Client) {
    let mut options = options;
    let mut connection_properties = match options.connect_properties() {
      Some(p) => p,
      None => ConnectProperties::new(),
    };
    /* Sparkplug requires session expiry interval to be 0 */
    connection_properties.session_expiry_interval = Some(0);

    options
      .set_clean_start(true)
      .set_connect_properties(connection_properties);

    let (client, eventloop) = RuClient::new(options, cap);
    (EventLoop{el: eventloop, state: ConnectionState::Disconnected}, Client{client})
  }

  async fn poll_rumqtt(&mut self) -> Option<Event>
  {
    let event = self.el.poll().await;
    match event {
      Ok(event) => {
        trace!("{event:?}");
        return match event {
          rumqttc::v5::Event::Incoming(Packet::ConnAck(_)) => {
            self.state = ConnectionState::Connected;
            Some(Event::Online)
          },
          rumqttc::v5::Event::Incoming(Packet::Disconnect(_)) => {
            self.state = ConnectionState::Disconnected;
            Some(Event::Offline)
          },
          rumqttc::v5::Event::Incoming(Packet::Publish(publish)) => Some(topic_and_payload_to_event(publish.topic.to_vec(), publish.payload.to_vec())),
          rumqttc::v5::Event::Outgoing(Outgoing::Disconnect) => {
            self.state = ConnectionState::ManualDisconnected;
            Some(Event::Offline)
          }
          _ => None
        }
      },
      Err(e) => {
        match self.state {
            ConnectionState::Connected=>{
              error!("Client error: {e}");
              self.state = ConnectionState::Disconnected;
              Some(Event::Offline)
            },
            ConnectionState::Disconnected=>{
              error!("Client error on reconnect attempt: {e}");
              tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
              None
            }
            ConnectionState::ManualDisconnected => None,
        }
      },
    }
  }
}

#[async_trait]
impl srad_client::EventLoop for EventLoop 
{
  async fn poll(&mut self) -> Event {
    loop {
      if let Some(event) = self.poll_rumqtt().await {
        return event 
      }
    }
  }

  fn set_last_will(&mut self, will: LastWill) {
    let qos = qos_to_mqtt_qos(will.qos);
    let mqtt_will = rumqttc::v5::mqttbytes::v5::LastWill::new(will.topic, will.payload, qos, will.retain, None);
    self.el.options.set_last_will(mqtt_will);
  }

}
