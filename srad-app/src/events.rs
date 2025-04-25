use srad_types::MetricId;

use crate::{MetricBirthDetails, MetricDetails, NodeIdentifier};

#[derive(Debug)]
pub struct NBirth {
    pub id: NodeIdentifier,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

#[derive(Debug)]
pub struct NDeath {
    pub id: NodeIdentifier,
}

#[derive(Debug)]
pub struct NData {
    pub id: NodeIdentifier,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}

#[derive(Debug)]
pub struct DBirth {
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricBirthDetails, MetricDetails)>,
}

#[derive(Debug)]
pub struct DDeath {
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
}

#[derive(Debug)]
pub struct DData {
    pub node_id: NodeIdentifier,
    pub device_name: String,
    pub timestamp: u64,
    pub metrics_details: Vec<(MetricId, MetricDetails)>,
}
