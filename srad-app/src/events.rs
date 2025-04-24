use srad_types::MetricId;

use crate::{MetricBirthDetails, MetricDetails, NodeIdentifier};

pub struct NBirth {
    pub id: NodeIdentifier,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

pub struct NDeath {
    pub id: NodeIdentifier,
}

pub struct NData {
    pub id: NodeIdentifier,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}

pub struct DBirth {
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

pub struct DDeath {
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
}

pub struct DData {
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}
