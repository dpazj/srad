use std::string::FromUtf8Error;

use prost::DecodeError;
use srad_types::{
    payload::Payload,
    topic::{state_host_topic, NodeMessage as NodeMessageType, NodeTopic, QoS},
};
use thiserror::Error;

/// Error types for message processing operations.
///
/// This enum represents the various error conditions that can occur
/// when decoding sparkplug protobuf payloads, validating topics, or handling payloads.
#[derive(Error, Debug, PartialEq)]
pub enum MessageError {
    #[error("There was an error decoding the payload: {0}")]
    DecodePayloadError(DecodeError),
    #[error("The topic was invalid")]
    InvalidSparkplugTopic,
    #[error("Topic parts utf8 decode error: {0}")]
    TopicUtf8Error(FromUtf8Error),
    #[error("Unable to decode state message as json: {0}")]
    StatePayloadJsonDecodeError(String),
}

impl From<FromUtf8Error> for MessageError {
    fn from(e: FromUtf8Error) -> Self {
        MessageError::TopicUtf8Error(e)
    }
}

/// An enum representing the different type of message.
#[derive(Debug, PartialEq)]
pub enum MessageKind {
    Birth,
    Death,
    Cmd,
    Data,
    Other(String),
}

/// A Message structure containing payload and the type of topic it was received on
#[derive(Debug, PartialEq)]
pub struct Message {
    pub payload: Payload,
    pub kind: MessageKind,
}

/// An enum representing the different type message published on a STATE topic.
#[derive(Debug, Clone, PartialEq)]
pub enum StatePayload {
    Online { timestamp: u64 },
    Offline { timestamp: u64 },
}

impl StatePayload {
    /// Get the [QoS] and retain settings that the State message should be published with
    pub fn get_publish_quality_retain(&self) -> (QoS, bool) {
        match self {
            StatePayload::Online { timestamp: _ } => (QoS::AtLeastOnce, true),
            StatePayload::Offline { timestamp: _ } => (QoS::AtLeastOnce, true),
        }
    }
}

impl From<StatePayload> for Vec<u8> {
    fn from(value: StatePayload) -> Self {
        match value {
            StatePayload::Online { timestamp } => {
                format!("{{\"online\" : true, \"timestamp\" : {timestamp}}}").into()
            }
            StatePayload::Offline { timestamp } => {
                format!("{{\"online\" : false, \"timestamp\" : {timestamp}}}").into()
            }
        }
    }
}

/// Represents a message from a Node.
#[derive(Debug, PartialEq)]
pub struct NodeMessage {
    /// The group the node belongs to.
    pub group_id: String,
    /// The nodes unique identifier.
    pub node_id: String,
    /// The message.
    pub message: Message,
}

/// Represents a message from a Device.
#[derive(Debug, PartialEq)]
pub struct DeviceMessage {
    /// The group the node belongs to.
    pub group_id: String,
    /// The nodes unique identifier.
    pub node_id: String,
    /// The devices unique identifier.
    pub device_id: String,
    /// The message.
    pub message: Message,
}

/// An enum that represents the different types of events an [EventLoop](crate::EventLoop) implementation can produce.
#[derive(Debug, PartialEq)]
pub enum Event {
    Offline,
    Online,
    Node(NodeMessage),
    Device(DeviceMessage),
    State {
        host_id: String,
        payload: StatePayload,
    },
    InvalidPublish {
        reason: MessageError,
        topic: Vec<u8>,
        payload: Vec<u8>,
    },
}

/// Structure representing the last will of a Node or Application
#[derive(Debug, Clone, PartialEq)]
pub struct LastWill {
    pub topic: String,
    pub retain: bool,
    pub qos: QoS,
    pub payload: Vec<u8>,
}

impl LastWill {
    pub fn new_node(group: &str, node_id: &str, payload: Payload) -> Self {
        let topic = NodeTopic::new(group, NodeMessageType::NDeath, node_id);
        let (qos, retain) = topic.get_publish_quality_retain();
        Self {
            retain,
            qos,
            payload: payload.into(),
            topic: topic.topic,
        }
    }

    pub fn new_app(host_id: &str, timestamp: u64) -> Self {
        Self {
            topic: state_host_topic(host_id),
            retain: true,
            qos: QoS::AtLeastOnce,
            payload: StatePayload::Offline { timestamp }.into(),
        }
    }
}
