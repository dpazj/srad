use crate::{DeviceMessage, Event, Message, MessageError, MessageKind, NodeMessage, StatePayload};

use prost::Message as ProstMessage;
use srad_types::{
    constants::STATE,
    payload::{Payload, StateBirthDeathCertificate},
};

enum MessageProducer {
    Device,
    Node,
}

fn process_topic_message(
    message_part: &[u8],
    payload: &[u8],
) -> Result<(MessageProducer, Message), MessageError> {
    if message_part.len() < 2 {
        return Err(MessageError::InvalidSparkplugTopic);
    }
    let producer = match message_part[0] {
        b'N' => MessageProducer::Node,
        b'D' => MessageProducer::Device,
        _ => return Err(MessageError::InvalidSparkplugTopic),
    };

    let payload = match Payload::decode(payload) {
        Ok(payload) => payload,
        Err(e) => return Err(MessageError::DecodePayloadError(e)),
    };

    let message_kind = match &message_part[1..] {
        b"BIRTH" => MessageKind::Birth,
        b"DEATH" => MessageKind::Death,
        b"DATA" => MessageKind::Data,
        b"CMD" => MessageKind::Cmd,
        msg => {
            let message_string = String::from_utf8(msg.into())?;
            MessageKind::Other(message_string)
        }
    };
    Ok((
        producer,
        Message {
            payload,
            kind: message_kind,
        },
    ))
}

/// Utility function to help clients convert topic and payload data to an [Event]
pub fn topic_and_payload_to_event(topic: Vec<u8>, payload: Vec<u8>) -> Event {
    let mut iter = topic.split(|c| *c == b'/');

    let spbv10 = iter.next();
    if spbv10.is_none() {
        return Event::InvalidPublish {
            reason: MessageError::InvalidSparkplugTopic,
            topic,
            payload,
        };
    }

    let state_or_group_id = match iter.next() {
        Some(val) => val,
        None => {
            return Event::InvalidPublish {
                reason: MessageError::InvalidSparkplugTopic,
                topic,
                payload,
            }
        }
    };

    if STATE.as_bytes().eq(state_or_group_id) {
        let host_id = match iter.next() {
            Some(val) => match String::from_utf8(val.into()) {
                Ok(id) => id,
                Err(e) => {
                    return Event::InvalidPublish {
                        reason: e.into(),
                        topic,
                        payload,
                    }
                }
            },
            None => {
                return Event::InvalidPublish {
                    reason: MessageError::InvalidSparkplugTopic,
                    topic,
                    payload,
                }
            }
        };

        return match StateBirthDeathCertificate::try_from(payload.as_slice()) {
            Ok(cert) => {
                let payload = match cert.online {
                    true => StatePayload::Online {
                        timestamp: cert.timestamp,
                    },
                    false => StatePayload::Offline {
                        timestamp: cert.timestamp,
                    },
                };
                Event::State {
                    host_id,
                    payload,
                }
            }
            Err(e) => Event::InvalidPublish {
                reason: MessageError::StatePayloadJsonDecodeError(e),
                topic,
                payload,
            },
        };
    }

    let group_id = match String::from_utf8(state_or_group_id.into()) {
        Ok(id) => id,
        Err(e) => {
            return Event::InvalidPublish {
                reason: e.into(),
                topic,
                payload,
            }
        }
    };

    //get message type
    let (message_producer, message) = match iter.next() {
        Some(val) => match process_topic_message(val, &payload) {
            Ok(res) => res,
            Err(e) => {
                return Event::InvalidPublish {
                    reason: e,
                    topic,
                    payload,
                }
            }
        },
        None => {
            return Event::InvalidPublish {
                reason: MessageError::InvalidSparkplugTopic,
                topic,
                payload,
            }
        }
    };

    //get node _id
    let node_id = match iter.next() {
        Some(val) => match String::from_utf8(val.into()) {
            Ok(id) => id,
            Err(e) => {
                return Event::InvalidPublish {
                    reason: e.into(),
                    topic,
                    payload,
                }
            }
        },
        None => {
            return Event::InvalidPublish {
                reason: MessageError::InvalidSparkplugTopic,
                topic,
                payload,
            }
        }
    };

    
    match message_producer {
        MessageProducer::Node => {
            if iter.next().is_some() {
                return Event::InvalidPublish {
                    reason: MessageError::InvalidSparkplugTopic,
                    topic,
                    payload,
                };
            }
            Event::Node(NodeMessage {
                group_id,
                node_id,
                message,
            })
        }
        MessageProducer::Device => {
            let device_id = match iter.next() {
                Some(val) => match String::from_utf8(val.into()) {
                    Ok(id) => id,
                    Err(e) => {
                        return Event::InvalidPublish {
                            reason: e.into(),
                            topic,
                            payload,
                        }
                    }
                },
                None => {
                    return Event::InvalidPublish {
                        reason: MessageError::InvalidSparkplugTopic,
                        topic,
                        payload,
                    }
                }
            };
            if iter.next().is_some() {
                return Event::InvalidPublish {
                    reason: MessageError::InvalidSparkplugTopic,
                    topic,
                    payload,
                };
            }
            Event::Device(DeviceMessage {
                group_id,
                node_id,
                device_id,
                message,
            })
        }
    }
}
