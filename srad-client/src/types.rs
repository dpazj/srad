use std::string::FromUtf8Error;

pub use bytes::Bytes;
use prost::{DecodeError, Message as ProstMessage};
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

/// A Sparkplug payload which may still be encoded.
#[derive(Debug, PartialEq)]
pub enum MessagePayload {
    /// Protobuf encoded payload.
    Encoded(Bytes),
    /// Decoded payload.
    Decoded(Payload),
}

impl MessagePayload {
    pub fn is_encoded(&self) -> bool {
        matches!(self, Self::Encoded(_))
    }

    pub fn decode(self) -> Result<Payload, MessageError> {
        match self {
            Self::Encoded(bytes) => {
                Payload::decode(bytes).map_err(MessageError::DecodePayloadError)
            }
            Self::Decoded(payload) => Ok(payload),
        }
    }
}

impl From<Payload> for MessagePayload {
    fn from(payload: Payload) -> Self {
        Self::Decoded(payload)
    }
}

impl From<Bytes> for MessagePayload {
    fn from(bytes: Bytes) -> Self {
        Self::Encoded(bytes)
    }
}

impl From<Vec<u8>> for MessagePayload {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Encoded(bytes.into())
    }
}

/// A Message struct containing payload and the type of topic it was received on
#[derive(Debug, PartialEq)]
pub struct Message {
    pub payload: MessagePayload,
    pub kind: MessageKind,
}

impl Message {
    pub fn decode(self) -> Result<Payload, MessageError> {
        self.payload.decode()
    }
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
        topic: Bytes,
        payload: Bytes,
    },
}

/// struct representing the last will of a Node or Application
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
        Self {
            topic: topic.topic,
            retain: true,
            qos: QoS::AtLeastOnce,
            payload: payload.into(),
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
