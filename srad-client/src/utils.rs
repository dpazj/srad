use crate::{
    Bytes, DeviceMessage, Event, Message, MessageError, MessageKind, MessagePayload, NodeMessage,
    StatePayload,
};

use prost::Message as ProstMessage;
use srad_types::{
    constants::STATE,
    payload::{Payload, StateBirthDeathCertificate},
};

enum MessageProducer {
    Device,
    Node,
}

fn parse_message_part(message_part: &[u8]) -> Result<(MessageProducer, MessageKind), MessageError> {
    if message_part.len() < 2 {
        return Err(MessageError::InvalidSparkplugTopic);
    }
    let producer = match message_part[0] {
        b'N' => MessageProducer::Node,
        b'D' => MessageProducer::Device,
        _ => return Err(MessageError::InvalidSparkplugTopic),
    };

    let kind = match &message_part[1..] {
        b"BIRTH" => MessageKind::Birth,
        b"DEATH" => MessageKind::Death,
        b"DATA" => MessageKind::Data,
        b"CMD" => MessageKind::Cmd,
        msg => MessageKind::Other(String::from_utf8(msg.into())?),
    };

    Ok((producer, kind))
}

fn build_message(
    payload: Bytes,
    kind: MessageKind,
    decode: bool,
) -> Result<Message, (MessageError, Bytes)> {
    let payload = if decode {
        match Payload::decode(payload.as_ref()) {
            Ok(payload) => MessagePayload::Decoded(payload),
            Err(e) => return Err((MessageError::DecodePayloadError(e), payload)),
        }
    } else {
        MessagePayload::Encoded(payload)
    };

    Ok(Message { payload, kind })
}

fn topic_and_payload_to_event_inner(topic: Bytes, payload: Bytes, decode: bool) -> Event {
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

        return match StateBirthDeathCertificate::try_from(payload.as_ref()) {
            Ok(cert) => {
                let payload = match cert.online {
                    true => StatePayload::Online {
                        timestamp: cert.timestamp,
                    },
                    false => StatePayload::Offline {
                        timestamp: cert.timestamp,
                    },
                };
                Event::State { host_id, payload }
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

    let (message_producer, kind) = match iter.next() {
        Some(val) => match parse_message_part(val) {
            Ok(parsed) => parsed,
            Err(reason) => {
                return Event::InvalidPublish {
                    reason,
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

            let message = match build_message(payload, kind, decode) {
                Ok(message) => message,
                Err((reason, payload)) => {
                    return Event::InvalidPublish {
                        reason,
                        topic,
                        payload,
                    }
                }
            };

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

            let message = match build_message(payload, kind, decode) {
                Ok(message) => message,
                Err((reason, payload)) => {
                    return Event::InvalidPublish {
                        reason,
                        topic,
                        payload,
                    }
                }
            };

            Event::Device(DeviceMessage {
                group_id,
                node_id,
                device_id,
                message,
            })
        }
    }
}

/// Utility function to help clients convert topic and payload data to a fully decoded [Event]
pub fn topic_and_payload_to_event<T, P>(topic: T, payload: P) -> Event
where
    T: Into<Bytes>,
    P: Into<Bytes>,
{
    topic_and_payload_to_event_inner(topic.into(), payload.into(), true)
}

/// Utility function to help clients convert topic and payload data to an [Event] without decoding node or device payloads
pub fn topic_and_payload_to_event_deferred<T, P>(topic: T, payload: P) -> Event
where
    T: Into<Bytes>,
    P: Into<Bytes>,
{
    topic_and_payload_to_event_inner(topic.into(), payload.into(), false)
}

#[cfg(test)]
mod tests {
    use prost::Message as _;
    use srad_types::{
        payload::{DataType, Metric},
        utils::timestamp,
        MetricValue,
    };

    use super::*;

    fn payload_bytes() -> Bytes {
        let mut metric = Metric::new();
        metric
            .set_name("A".into())
            .set_datatype(DataType::Int32)
            .set_value(MetricValue::from(1i32).into())
            .set_timestamp(timestamp());

        Payload {
            timestamp: Some(timestamp()),
            metrics: vec![metric],
            seq: Some(0),
            uuid: None,
            body: None,
        }
        .encode_to_vec()
        .into()
    }

    #[test]
    fn deferred_leaves_payload_encoded() {
        let event = topic_and_payload_to_event_deferred(
            Bytes::from_static(b"spBv1.0/group1/NBIRTH/node1"),
            payload_bytes(),
        );

        match event {
            Event::Node(node) => {
                assert!(node.message.payload.is_encoded());
                assert_eq!(node.message.kind, MessageKind::Birth);
                assert_eq!(node.message.decode().unwrap().metrics.len(), 1);
            }
            event => panic!("{event:?}"),
        }
    }

    #[test]
    fn eager_decodes_payload() {
        let event = topic_and_payload_to_event(
            Bytes::from_static(b"spBv1.0/group1/NDATA/node1"),
            payload_bytes(),
        );

        match event {
            Event::Node(node) => assert!(!node.message.payload.is_encoded()),
            event => panic!("{event:?}"),
        }
    }

    #[test]
    fn invalid_publish_keeps_payload_bytes() {
        let payload = payload_bytes();
        let event =
            topic_and_payload_to_event_deferred(Bytes::from_static(b"bad"), payload.clone());

        match event {
            Event::InvalidPublish {
                payload: invalid, ..
            } => assert_eq!(invalid, payload),
            event => panic!("{event:?}"),
        }
    }
}
