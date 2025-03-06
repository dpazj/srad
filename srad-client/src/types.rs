use std::string::FromUtf8Error;

use srad_types::{constants, payload::{Metric, Payload}, topic::{self, node_topic, state_host_topic, QoS}};


#[derive(Debug)]
pub enum MessageError {
  InvalidPayload,
  InvalidSparkplugTopic,
  TopicUtf8Error(FromUtf8Error),
}

impl From<FromUtf8Error> for MessageError {
  fn from(e: FromUtf8Error) -> Self {
    MessageError::TopicUtf8Error(e)
  }
}

#[derive(Debug)]
pub enum ClientError {
  MessageError(MessageError),
  Other,
}

impl From<MessageError> for ClientError {
  fn from(e: MessageError) -> Self {
    ClientError::MessageError(e)
  }
}

#[derive(Debug)]
pub enum Message {
  Birth {payload: Payload},
  Death {payload: Payload},
  Cmd {payload: Payload}, 
  Data {payload: Payload},
  Other {name: String, payload: Payload}
}

#[derive(Debug)]
pub enum StatePayload {
  Online {timestamp: u64},
  Offline {timestamp: u64},
  Other()
}

#[derive(Debug)]
pub struct NodeMessage {
  pub group_id: String,
  pub node_id: String,
  pub message: Message,
}

#[derive(Debug)]
pub struct DeviceMessage{
  pub group_id: String,
  pub node_id: String,
  pub device_id: String,
  pub message: Message,
}

#[derive(Debug)]
pub enum Event {
  Offline,
  Online,
  Node(NodeMessage),
  Device(DeviceMessage),
  State {
    host_id: String,
    payload: StatePayload
  },
  InvalidPublish { reason: MessageError, topic: Vec<u8>, payload: Vec<u8> }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LastWill {
  pub topic: String,
  pub retain: bool,
  pub qos: QoS,
  pub payload: Vec<u8>
} 

impl LastWill {

  pub fn new_node(group: &str, node_id: &str, bdseq: u8) -> Self {
    let topic = node_topic(group, &topic::NodeMessage::NDeath, node_id);
    let mut metric = Metric::new(); 
    metric.set_name(constants::BDSEQ.to_string())
      .set_value(srad_types::payload::metric::Value::LongValue(bdseq as u64));
    let payload = Payload {
      seq: None, 
      metrics: vec![metric],
      uuid: None,
      timestamp: None,
      body: None
    };
    Self {
      topic,
      retain: false, 
      qos: QoS::AtLeastOnce,
      payload: payload.into()
    }
  }

  pub fn new_app(host_id: &str) -> Self {
    let payload = "{\"online\" : true, \"timestamp\" : 0}";
    Self {
      topic: state_host_topic(host_id),
      retain: true,
      qos: QoS::AtLeastOnce,
      payload: payload.into()
    }

  }

}

#[derive(PartialEq)]
enum PendingOfflineEventState {
  None, 
  PendingEvent,
  EventSent
}
