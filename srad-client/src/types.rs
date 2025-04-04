use std::string::FromUtf8Error;

use srad_types::{payload::Payload, topic::{state_host_topic, NodeTopic, NodeMessage as NodeMessageType, QoS}};


#[derive(Debug, PartialEq)]
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

#[derive(Debug, PartialEq)]
pub enum MessageKind {
  Birth,
  Death,
  Cmd, 
  Data,
  Other(String)
}

#[derive(Debug, PartialEq)]
pub struct Message {
  pub payload: Payload,
  pub kind: MessageKind
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatePayload {
  Online {timestamp: u64},
  Offline {timestamp: u64},
  Other(Vec<u8>)
}

impl StatePayload {
  pub fn get_publish_quality_retain(&self) -> (QoS, bool) {
    match self {
        StatePayload::Online { timestamp: _ } => (QoS::AtLeastOnce, false),
        StatePayload::Offline { timestamp: _ } => (QoS::AtLeastOnce, true),
        StatePayload::Other(_) => (QoS::AtMostOnce, false),
    }
  }
}

impl From<StatePayload> for Vec<u8> {
    fn from(value: StatePayload) -> Self {
      match value {
        StatePayload::Online { timestamp } => format!("{{\"online\" : true, \"timestamp\" : {}}}", timestamp).into(),
        StatePayload::Offline { timestamp } => format!("{{\"online\" : false, \"timestamp\" : {}}}", timestamp).into(),
        StatePayload::Other (data) => data,
      } 
    }
}

#[derive(Debug, PartialEq)]
pub struct NodeMessage {
  pub group_id: String,
  pub node_id: String,
  pub message: Message,
}

#[derive(Debug, PartialEq)]
pub struct DeviceMessage{
  pub group_id: String,
  pub node_id: String,
  pub device_id: String,
  pub message: Message,
}

#[derive(Debug, PartialEq)]
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

  pub fn new_node(group: &str, node_id: &str, payload: Payload) -> Self {
    let topic = NodeTopic::new(group, NodeMessageType::NDeath, node_id);
    let (qos, retain) = topic.get_publish_quality_retain();
    Self {
      retain: retain, 
      qos: qos,
      payload: payload.into(),
      topic: topic.topic,
    }
  }

  pub fn new_app(host_id: &str, timestamp: u64) -> Self {
    Self {
      topic: state_host_topic(host_id),
      retain: true,
      qos: QoS::AtLeastOnce,
      payload: StatePayload::Offline { timestamp }.into() 
    }

  }

}

