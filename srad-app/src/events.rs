use srad_client::{DeviceMessage, NodeMessage};
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
}

#[derive(Debug)]
pub struct AppNodeEvent {
    pub id: NodeIdentifier,
    pub event: NodeEvent,
}

impl TryFrom<NodeMessage> for AppNodeEvent {
    type Error = MessageTryFromError;

    fn try_from(value: NodeMessage) -> Result<Self, Self::Error> {
        let id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };

        let event = match value.message.kind {
            srad_client::MessageKind::Birth => match NBirth::try_from(value.message.payload) {
                Ok(v) => NodeEvent::Birth(v),
                Err(e) => {
                    return Err(MessageTryFromError::PayloadError(PayloadErrorDetails::new(
                        id, e,
                    )))
                }
            },
            srad_client::MessageKind::Death => match NDeath::try_from(value.message.payload) {
                Ok(v) => NodeEvent::Death(v),
                Err(e) => {
                    return Err(MessageTryFromError::PayloadError(PayloadErrorDetails::new(
                        id, e,
                    )))
                }
            },
            srad_client::MessageKind::Cmd => return Err(MessageTryFromError::UnsupportedVerb),
            srad_client::MessageKind::Data => match NData::try_from(value.message.payload) {
                Ok(v) => NodeEvent::Data(v),
                Err(e) => {
                    return Err(MessageTryFromError::PayloadError(PayloadErrorDetails::new(
                        id, e,
                    )))
                }
            },
            srad_client::MessageKind::Other(_) => return Err(MessageTryFromError::UnsupportedVerb),
        };
        Ok(AppNodeEvent { id, event })
    }
}

#[derive(Debug)]
pub enum DeviceEvent {
    Birth(DBirth),
    Death(DDeath),
    Data(DData),
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
        let id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };
        let name = value.device_id;

        let event = match value.message.kind {
            srad_client::MessageKind::Birth => match DBirth::try_from(value.message.payload) {
                Ok(v) => DeviceEvent::Birth(v),
                Err(e) => {
                    return Err(MessageTryFromError::PayloadError(
                        PayloadErrorDetails::new(id, e).with_device(name),
                    ))
                }
            },
            srad_client::MessageKind::Death => match DDeath::try_from(value.message.payload) {
                Ok(v) => DeviceEvent::Death(v),
                Err(e) => {
                    return Err(MessageTryFromError::PayloadError(
                        PayloadErrorDetails::new(id, e).with_device(name),
                    ))
                }
            },
            srad_client::MessageKind::Cmd => return Err(MessageTryFromError::UnsupportedVerb),
            srad_client::MessageKind::Data => match DData::try_from(value.message.payload) {
                Ok(v) => DeviceEvent::Data(v),
                Err(e) => {
                    return Err(MessageTryFromError::PayloadError(
                        PayloadErrorDetails::new(id, e).with_device(name),
                    ))
                }
            },
            srad_client::MessageKind::Other(_) => return Err(MessageTryFromError::UnsupportedVerb),
        };
        Ok(AppDeviceEvent { id, name, event })
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
