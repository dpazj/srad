use crate::constants::STATE;

use super::constants::{DBIRTH, DCMD, DDATA, DDEATH, NBIRTH, NDEATH, NDATA, NCMD, SPBV01};

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceMessage{
  DBirth, 
  DDeath, 
  DData, 
  DCmd 
}

impl DeviceMessage {
  fn as_str(&self) -> &str{
    match self {
        DeviceMessage::DBirth => DBIRTH,
        DeviceMessage::DDeath => DDEATH,
        DeviceMessage::DData => DDATA, 
        DeviceMessage::DCmd => DCMD
    } 
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeMessage{
  NBirth, 
  NDeath, 
  NData, 
  NCmd
}

impl NodeMessage {
  fn as_str(&self) -> &str{
    match self {
        NodeMessage::NBirth => NBIRTH,
        NodeMessage::NDeath => NDEATH,
        NodeMessage::NData => NDATA, 
        NodeMessage::NCmd => NCMD
    } 
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeTopic{
  pub topic: String,
  pub message_type: NodeMessage,
}

impl NodeTopic {
  pub fn new(group_id: &str, message_type: NodeMessage, node_id: &str) -> Self {
    Self {
      topic: node_topic(group_id, &message_type, node_id),
      message_type: message_type
    }
  }

  pub fn get_publish_quality_retain(&self) -> (QoS, bool) {
    match self.message_type {
      NodeMessage::NBirth => (QoS::AtMostOnce, false),
      NodeMessage::NData => (QoS::AtMostOnce, false),
      NodeMessage::NCmd => (QoS::AtMostOnce, false),
      NodeMessage::NDeath => todo!(),
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceTopic {
  pub topic: String,
  pub message_type: DeviceMessage,
}

impl DeviceTopic {

  pub fn new(group_id: &str, message_type: DeviceMessage, node_id: &str, device_id: &str) -> Self {
    Self {
      topic: device_topic(group_id, &message_type, node_id, device_id),
      message_type: message_type
    }
  }

  pub fn get_publish_quality_retain(&self) -> (QoS, bool) {
    match self.message_type {
      DeviceMessage::DBirth => (QoS::AtLeastOnce, false),
      DeviceMessage::DData => (QoS::AtMostOnce, false),
      DeviceMessage::DCmd => (QoS::AtMostOnce, false),
      DeviceMessage::DDeath => (QoS::AtLeastOnce, false)
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateTopic {
  topic: String,
}

impl StateTopic {

  pub fn new() -> Self {
    Self {
      topic: state_sub_topic()
    }
  }

  pub fn new_host(host_id: &str) -> Self {
    Self {
      topic: state_host_topic(host_id)
    }
  }

}

#[derive(Clone, Debug, PartialEq)]
pub enum Topic {
  NodeTopic(NodeTopic),
  DeviceTopic(DeviceTopic),
  State(StateTopic),
  Node{group_id: String, node_id: String},
  Group {id: String},
  Namespace,
}

impl Into<String> for Topic {

  fn into(self) -> String {
    match self {
      Topic::NodeTopic(node_topic) => node_topic.topic,
      Topic::DeviceTopic(device_topic) => device_topic.topic,
      Topic::State(state_topic) => state_topic.topic,
      Topic::Node { group_id, node_id  } => format!("{}/{}/+/{}/#", SPBV01, group_id, node_id),
      Topic::Group { id } => format!("{}/{}/#", SPBV01, id),
      Topic::Namespace => format!("{}/#", SPBV01),
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum QoS {
  AtMostOnce,
  AtLeastOnce,
  ExactlyOnce
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopicFilter {
  pub topic: Topic,
  pub qos: QoS
}

impl TopicFilter {

  pub fn new(topic: Topic) -> Self {
    Self::new_with_qos(topic, QoS::AtMostOnce)
  }

  pub fn new_with_qos(topic: Topic, qos: QoS) -> Self {
    Self {topic, qos}
  }
}

pub fn node_topic_raw(group_id: &str, message_type: &str, node_id: &str) -> String {
  format!("{}/{}/{}/{}", SPBV01, group_id, message_type, node_id)
}

pub fn node_topic(group_id: &str, message_type: &NodeMessage, node_id: &str) -> String {
  node_topic_raw(group_id, message_type.as_str(), node_id)
}

pub fn device_topic(group_id: &str, message_type: &DeviceMessage, node_id: &str, device_id: &str) -> String {
  format!("{}/{}/{}/{}/{}", SPBV01, group_id, message_type.as_str(), node_id, device_id)
}

pub fn state_host_topic(host_id: &str) -> String{
  format!("{}/{}/{}", SPBV01, STATE, host_id)
}

pub fn state_sub_topic() -> String {
  state_host_topic( "#")
}

