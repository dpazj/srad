use srad_client::{DeviceMessage, Message, NodeMessage};
use srad_types::{payload::Payload, MetricId};

use crate::{
    bdseq_from_payload_metrics, get_metric_birth_details_from_birth_metrics,
    get_metric_id_and_details_from_payload_metrics, MetricBirthDetails, MetricDetails,
    NodeIdentifier, PayloadMetricError,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PayloadError {
    #[error("Seq not included in payload")]
    MissingSeq,
    #[error("Invalid seq")]
    InvalidSeq,
    #[error("Timestamp not included in payload")]
    InvalidBdseq,
    #[error("invalid bdseq")]
    MissingTimestamp,
    #[error("Metric error: {0}")]
    MetricError(#[from] PayloadMetricError),
    #[error("Unable to decode payload: {0}")]
    DecodePayload(#[from] srad_client::MessageError),
}

#[derive(Debug)]
pub struct PayloadErrorDetails {
    pub node_id: NodeIdentifier,
    pub device: Option<String>,
    pub error: PayloadError,
}

impl PayloadErrorDetails {
    fn new(node_id: NodeIdentifier, error: PayloadError) -> Self {
        Self {
            node_id,
            device: None,
            error,
        }
    }

    fn with_device(mut self, device: String) -> Self {
        self.device = Some(device);
        self
    }
}

pub enum MessageTryFromError {
    PayloadError(PayloadErrorDetails),
    UnsupportedVerb,
}

#[derive(Debug)]
pub enum NodeEvent {
    Birth(NBirth),
    Death(NDeath),
    Data(NData),
    Raw(Message),
}

/// The decoded form of a [NodeEvent].
#[derive(Debug)]
pub enum DecodedNodeEvent {
    Birth(NBirth),
    Death(NDeath),
    Data(NData),
}

impl From<DecodedNodeEvent> for NodeEvent {
    fn from(event: DecodedNodeEvent) -> Self {
        match event {
            DecodedNodeEvent::Birth(v) => Self::Birth(v),
            DecodedNodeEvent::Death(v) => Self::Death(v),
            DecodedNodeEvent::Data(v) => Self::Data(v),
        }
    }
}

impl NodeEvent {
    pub fn from_message(message: Message) -> Result<Option<DecodedNodeEvent>, PayloadError> {
        let Message { payload, kind } = message;
        match kind {
            srad_client::MessageKind::Birth => Ok(Some(DecodedNodeEvent::Birth(NBirth::try_from(
                payload.decode()?,
            )?))),
            srad_client::MessageKind::Death => Ok(Some(DecodedNodeEvent::Death(NDeath::try_from(
                payload.decode()?,
            )?))),
            srad_client::MessageKind::Data => Ok(Some(DecodedNodeEvent::Data(NData::try_from(
                payload.decode()?,
            )?))),
            srad_client::MessageKind::Cmd | srad_client::MessageKind::Other(_) => Ok(None),
        }
    }
}

#[derive(Debug)]
pub struct AppNodeEvent {
    pub id: NodeIdentifier,
    pub event: NodeEvent,
}

impl TryFrom<NodeMessage> for AppNodeEvent {
    type Error = MessageTryFromError;

    fn try_from(value: NodeMessage) -> Result<Self, Self::Error> {
        let NodeMessage {
            group_id,
            node_id,
            message,
        } = value;
        let id = NodeIdentifier {
            group: group_id,
            node: node_id,
        };

        if message.payload.is_encoded() {
            return match &message.kind {
                srad_client::MessageKind::Cmd | srad_client::MessageKind::Other(_) => {
                    Err(MessageTryFromError::UnsupportedVerb)
                }
                _ => Ok(AppNodeEvent {
                    id,
                    event: NodeEvent::Raw(message),
                }),
            };
        }

        match NodeEvent::from_message(message) {
            Ok(Some(event)) => Ok(AppNodeEvent {
                id,
                event: event.into(),
            }),
            Ok(None) => Err(MessageTryFromError::UnsupportedVerb),
            Err(e) => Err(MessageTryFromError::PayloadError(PayloadErrorDetails::new(
                id, e,
            ))),
        }
    }
}

#[derive(Debug)]
pub enum DeviceEvent {
    Birth(DBirth),
    Death(DDeath),
    Data(DData),
    Raw(Message),
}

/// The decoded form of a [DeviceEvent].
#[derive(Debug)]
pub enum DecodedDeviceEvent {
    Birth(DBirth),
    Death(DDeath),
    Data(DData),
}

impl From<DecodedDeviceEvent> for DeviceEvent {
    fn from(event: DecodedDeviceEvent) -> Self {
        match event {
            DecodedDeviceEvent::Birth(v) => Self::Birth(v),
            DecodedDeviceEvent::Death(v) => Self::Death(v),
            DecodedDeviceEvent::Data(v) => Self::Data(v),
        }
    }
}

impl DeviceEvent {
    pub fn from_message(message: Message) -> Result<Option<DecodedDeviceEvent>, PayloadError> {
        let Message { payload, kind } = message;
        match kind {
            srad_client::MessageKind::Birth => Ok(Some(DecodedDeviceEvent::Birth(
                DBirth::try_from(payload.decode()?)?,
            ))),
            srad_client::MessageKind::Death => Ok(Some(DecodedDeviceEvent::Death(
                DDeath::try_from(payload.decode()?)?,
            ))),
            srad_client::MessageKind::Data => Ok(Some(DecodedDeviceEvent::Data(DData::try_from(
                payload.decode()?,
            )?))),
            srad_client::MessageKind::Cmd | srad_client::MessageKind::Other(_) => Ok(None),
        }
    }
}

#[derive(Debug)]
pub struct AppDeviceEvent {
    pub id: NodeIdentifier,
    pub name: String,
    pub event: DeviceEvent,
}

impl TryFrom<DeviceMessage> for AppDeviceEvent {
    type Error = MessageTryFromError;

    fn try_from(value: DeviceMessage) -> Result<Self, Self::Error> {
        let DeviceMessage {
            group_id,
            node_id,
            device_id,
            message,
        } = value;
        let id = NodeIdentifier {
            group: group_id,
            node: node_id,
        };
        let name = device_id;

        if message.payload.is_encoded() {
            return match &message.kind {
                srad_client::MessageKind::Cmd | srad_client::MessageKind::Other(_) => {
                    Err(MessageTryFromError::UnsupportedVerb)
                }
                _ => Ok(AppDeviceEvent {
                    id,
                    name,
                    event: DeviceEvent::Raw(message),
                }),
            };
        }

        match DeviceEvent::from_message(message) {
            Ok(Some(event)) => Ok(AppDeviceEvent {
                id,
                name,
                event: event.into(),
            }),
            Ok(None) => Err(MessageTryFromError::UnsupportedVerb),
            Err(e) => Err(MessageTryFromError::PayloadError(
                PayloadErrorDetails::new(id, e).with_device(name),
            )),
        }
    }
}

#[derive(Debug)]
pub struct NBirth {
    pub bdseq: u8,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

impl TryFrom<Payload> for NBirth {
    type Error = PayloadError;

    fn try_from(payload: Payload) -> Result<Self, Self::Error> {
        match payload.seq {
            Some(seq) => {
                if seq != 0 {
                    return Err(PayloadError::InvalidSeq);
                }
            }
            None => return Err(PayloadError::MissingSeq),
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadError::MissingTimestamp),
        };

        let bdseq = match bdseq_from_payload_metrics(&payload.metrics) {
            Ok(bdseq) => bdseq,
            Err(_) => return Err(PayloadError::InvalidBdseq),
        };

        let metrics_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
            Ok(details) => details,
            Err(e) => return Err(PayloadError::MetricError(e)),
        };

        Ok(NBirth {
            bdseq,
            timestamp,
            metrics_details,
        })
    }
}

#[derive(Debug)]
pub struct NDeath {
    pub bdseq: u8,
}

impl TryFrom<Payload> for NDeath {
    type Error = PayloadError;

    fn try_from(payload: Payload) -> Result<Self, Self::Error> {
        let bdseq = match bdseq_from_payload_metrics(&payload.metrics) {
            Ok(bdseq) => bdseq,
            Err(_) => return Err(PayloadError::InvalidBdseq),
        };
        Ok(NDeath { bdseq })
    }
}

#[derive(Debug)]
pub struct NData {
    pub seq: u8,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}

impl TryFrom<Payload> for NData {
    type Error = PayloadError;

    fn try_from(payload: Payload) -> Result<Self, Self::Error> {
        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => return Err(PayloadError::MissingSeq),
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadError::MissingTimestamp),
        };

        let metrics_details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
        {
            Ok(details) => details,
            Err(e) => return Err(PayloadError::MetricError(e)),
        };

        Ok(NData {
            seq,
            timestamp,
            metrics_details,
        })
    }
}

#[derive(Debug)]
pub struct DBirth {
    pub seq: u8,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

impl TryFrom<Payload> for DBirth {
    type Error = PayloadError;

    fn try_from(payload: Payload) -> Result<Self, Self::Error> {
        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                return Err(PayloadError::MissingSeq);
            }
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadError::MissingTimestamp),
        };

        let metrics_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
            Ok(details) => details,
            Err(e) => return Err(PayloadError::MetricError(e)),
        };

        Ok(DBirth {
            seq,
            timestamp,
            metrics_details,
        })
    }
}

#[derive(Debug)]
pub struct DDeath {
    pub seq: u8,
    pub timestamp: u64,
}

impl TryFrom<Payload> for DDeath {
    type Error = PayloadError;

    fn try_from(payload: Payload) -> Result<Self, Self::Error> {
        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => return Err(PayloadError::MissingSeq),
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadError::MissingTimestamp),
        };

        Ok(DDeath { seq, timestamp })
    }
}

#[derive(Debug)]
pub struct DData {
    pub seq: u8,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}

impl TryFrom<Payload> for DData {
    type Error = PayloadError;

    fn try_from(payload: Payload) -> Result<Self, Self::Error> {
        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => return Err(PayloadError::MissingSeq),
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadError::MissingTimestamp),
        };

        let metrics_details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
        {
            Ok(details) => details,
            Err(e) => return Err(PayloadError::MetricError(e)),
        };

        Ok(DData {
            seq,
            timestamp,
            metrics_details,
        })
    }
}
