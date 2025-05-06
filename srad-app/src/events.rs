use srad_client::{DeviceMessage, NodeMessage};
use srad_types::MetricId;

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

#[derive(Debug)]
pub struct NBirth {
    pub bdseq: u8,
    pub id: NodeIdentifier,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

impl TryFrom<NodeMessage> for NBirth {
    type Error = PayloadErrorDetails;

    fn try_from(value: NodeMessage) -> Result<Self, Self::Error> {
        let payload = value.message.payload;
        let id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };

        match payload.seq {
            Some(seq) => {
                if seq != 0 {
                    return Err(PayloadErrorDetails::new(id, PayloadError::InvalidSeq));
                }
            }
            None => return Err(PayloadErrorDetails::new(id, PayloadError::MissingSeq)),
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadErrorDetails::new(id, PayloadError::MissingTimestamp)),
        };

        let bdseq = match bdseq_from_payload_metrics(&payload.metrics) {
            Ok(bdseq) => bdseq,
            Err(_) => return Err(PayloadErrorDetails::new(id, PayloadError::InvalidBdseq)),
        };

        let metrics_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
            Ok(details) => details,
            Err(e) => return Err(PayloadErrorDetails::new(id, PayloadError::MetricError(e))),
        };

        Ok(NBirth {
            bdseq,
            id,
            timestamp,
            metrics_details,
        })
    }
}

#[derive(Debug)]
pub struct NDeath {
    pub id: NodeIdentifier,
    pub bdseq: u8,
}

impl TryFrom<NodeMessage> for NDeath {
    type Error = PayloadErrorDetails;

    fn try_from(value: NodeMessage) -> Result<Self, Self::Error> {
        let payload = value.message.payload;
        let id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };

        let bdseq = match bdseq_from_payload_metrics(&payload.metrics) {
            Ok(bdseq) => bdseq,
            Err(_) => return Err(PayloadErrorDetails::new(id, PayloadError::InvalidBdseq)),
        };

        Ok(NDeath { bdseq, id })
    }
}

#[derive(Debug)]
pub struct NData {
    pub seq: u8,
    pub id: NodeIdentifier,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}

impl TryFrom<NodeMessage> for NData {
    type Error = PayloadErrorDetails;

    fn try_from(value: NodeMessage) -> Result<Self, Self::Error> {
        let payload = value.message.payload;
        let id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };

        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => return Err(PayloadErrorDetails::new(id, PayloadError::MissingSeq)),
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => return Err(PayloadErrorDetails::new(id, PayloadError::MissingTimestamp)),
        };

        let metrics_details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
        {
            Ok(details) => details,
            Err(e) => return Err(PayloadErrorDetails::new(id, PayloadError::MetricError(e))),
        };

        Ok(NData {
            seq,
            id,
            timestamp,
            metrics_details,
        })
    }
}

#[derive(Debug)]
pub struct DBirth {
    pub seq: u8,
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

impl TryFrom<DeviceMessage> for DBirth {
    type Error = PayloadErrorDetails;

    fn try_from(value: DeviceMessage) -> Result<Self, Self::Error> {
        let payload = value.message.payload;
        let node_id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };
        let device_name = value.device_id;

        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                return Err(PayloadErrorDetails::new(node_id, PayloadError::MissingSeq)
                    .with_device(device_name))
            }
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => {
                return Err(
                    PayloadErrorDetails::new(node_id, PayloadError::MissingTimestamp)
                        .with_device(device_name),
                )
            }
        };

        let metrics_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
            Ok(details) => details,
            Err(e) => {
                return Err(
                    PayloadErrorDetails::new(node_id, PayloadError::MetricError(e))
                        .with_device(device_name),
                )
            }
        };

        Ok(DBirth {
            seq,
            timestamp,
            metrics_details,
            node_id,
            device_name,
        })
    }
}

#[derive(Debug)]
pub struct DDeath {
    pub seq: u8,
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
}

impl TryFrom<DeviceMessage> for DDeath {
    type Error = PayloadErrorDetails;

    fn try_from(value: DeviceMessage) -> Result<Self, Self::Error> {
        let payload = value.message.payload;
        let node_id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };
        let device_name = value.device_id;

        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                return Err(PayloadErrorDetails::new(node_id, PayloadError::MissingSeq)
                    .with_device(device_name))
            }
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => {
                return Err(
                    PayloadErrorDetails::new(node_id, PayloadError::MissingTimestamp)
                        .with_device(device_name),
                )
            }
        };

        Ok(DDeath {
            seq,
            timestamp,
            node_id,
            device_name,
        })
    }
}

#[derive(Debug)]
pub struct DData {
    pub seq: u8,
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}

impl TryFrom<DeviceMessage> for DData {
    type Error = PayloadErrorDetails;

    fn try_from(value: DeviceMessage) -> Result<Self, Self::Error> {
        let payload = value.message.payload;
        let node_id = NodeIdentifier {
            group: value.group_id,
            node: value.node_id,
        };
        let device_name = value.device_id;

        let seq = match payload.seq {
            Some(seq) => seq as u8,
            None => {
                return Err(PayloadErrorDetails::new(node_id, PayloadError::MissingSeq)
                    .with_device(device_name))
            }
        };

        let timestamp = match payload.timestamp {
            Some(ts) => ts,
            None => {
                return Err(
                    PayloadErrorDetails::new(node_id, PayloadError::MissingTimestamp)
                        .with_device(device_name),
                )
            }
        };

        let metrics_details = match get_metric_id_and_details_from_payload_metrics(payload.metrics)
        {
            Ok(details) => details,
            Err(e) => {
                return Err(
                    PayloadErrorDetails::new(node_id, PayloadError::MetricError(e))
                        .with_device(device_name),
                )
            }
        };

        Ok(DData {
            seq,
            timestamp,
            metrics_details,
            node_id,
            device_name,
        })
    }
}
