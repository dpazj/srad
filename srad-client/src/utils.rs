use crate::{DeviceMessage, Event, Message, MessageError, NodeMessage, StatePayload};

use srad_types::{constants::STATE, payload::Payload}; 
use prost::Message as ProstMessage;

enum MessageProducer {
  Device, 
  Node
}

fn process_topic_message(message_part: &[u8], payload: &[u8]) -> Result<(MessageProducer, Message), MessageError> {
  if message_part.len() < 2 {
    return Err(MessageError::InvalidSparkplugTopic);
  }
  let producer = match message_part[0] {
    b'N' => MessageProducer::Node, 
    b'D' => MessageProducer::Device,
    _ => {return Err(MessageError::InvalidSparkplugTopic)}
  };

  let payload = match Payload::decode(payload) {
    Ok(payload) => payload,
    Err(_) => {return Err(MessageError::InvalidPayload)},
  };

  let message = match &message_part[1..] {
    b"BIRTH" => Message::Birth { payload: payload },
    b"DEATH" => Message::Death { payload: payload },
    b"DATA" => Message::Data { payload: payload },
    b"CMD" => Message::Cmd { payload: payload },
    msg => {
      let message_string = String::from_utf8(msg.into())?;
      Message::Other { name: message_string, payload: payload}
    },
  };
  Ok ((producer, message))
}

pub fn topic_and_payload_to_event(topic: &[u8], payload: &[u8]) -> Result<Event, MessageError>
{
  let mut iter = topic.split(|c| *c == b'/');

  let spbv10= iter.next();
  if spbv10.is_none() {
    return Err(MessageError::InvalidSparkplugTopic); 
  }

  let state_or_group_id = match iter.next() {
    Some(val) => val,
    None => return Err(MessageError::InvalidSparkplugTopic),
  };

  if STATE.as_bytes().eq(state_or_group_id) {
    return Ok(Event::State { host_id: "not implemented".into(), payload: StatePayload::Other()}) //TODO
  }

  let group_id= String::from_utf8(state_or_group_id.to_vec())?;

  //get message type 
  let (message_producer, message) = match iter.next() {
    Some(val) => process_topic_message(val, payload)?,
    None => return Err(MessageError::InvalidSparkplugTopic),
  };

  //get node _id 
  let node_id = match iter.next() {
    Some(val) => String::from_utf8(val.to_vec())?,
    None => return Err(MessageError::InvalidSparkplugTopic),
  };

  let event = match message_producer {
    MessageProducer::Node => {
      if let Some(_) = iter.next() { return Err(MessageError::InvalidSparkplugTopic) }
      Event::Node(NodeMessage{ group_id: group_id, node_id: node_id, message: message })
    },
    MessageProducer::Device => {
      let device_id = match iter.next() {
        Some(val) => String::from_utf8(val.to_vec())?,
        None => return Err(MessageError::InvalidSparkplugTopic),
      };
      if let Some(_) = iter.next() { return Err(MessageError::InvalidSparkplugTopic) }
      Event::Device(DeviceMessage { group_id: group_id, node_id: node_id, device_id: device_id, message: message})
    }
  };
  Ok(event)
}
