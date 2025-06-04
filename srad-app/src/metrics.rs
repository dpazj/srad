use srad_types::{
    constants::BDSEQ,
    payload::{self, DataType, MetaData, Metric},
    traits, MetricId, MetricValue, PropertySet,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PayloadMetricError {
    #[error("Timestamp not included")]
    MissingTimestamp,
    #[error("Datatype not included")]
    MissingDatatype,
    #[error("Invalid Datatype value")]
    InvalidDatatype,
    #[error("Missing Name")]
    MissingName,
    #[error("Metric is_null was false but not value was provided")]
    NotNullNoValue,
    #[error("Invalid Properties")]
    InvalidProperties,
}

pub(crate) fn bdseq_from_payload_metrics(vec: &Vec<Metric>) -> Result<u8, ()> {
    for x in vec {
        match &x.name {
            Some(name) => {
                if name != BDSEQ {
                    continue;
                }
            }
            None => continue,
        }
        match &x.value {
            Some(x) => match i64::try_from(MetricValue::from(x.clone())) {
                Ok(v) => {
                    if v > u8::MAX as i64 || v < 0 {
                        return Err(());
                    }
                    return Ok(v as u8);
                }
                Err(_) => {
                    return Err(());
                }
            },
            None => {
                return Err(());
            }
        };
    }
    Err(())
}

/// Represents a metric value to be published
pub struct PublishMetric {
    metric_identifier: MetricId,
    value: MetricValue,
    timestamp: Option<u64>,
}

impl PublishMetric {
    /// Creates a new metric for publishing with the specified identifier and value.
    pub fn new<T: traits::MetricValue>(metric_identifier: MetricId, value: T) -> Self {
        Self {
            metric_identifier,
            value: value.into(),
            timestamp: None,
        }
    }

    /// Sets a timestamp for the published metric
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }
}

impl From<PublishMetric> for Metric {
    fn from(val: PublishMetric) -> Self {
        let mut metric = Metric::new();
        match val.metric_identifier {
            MetricId::Name(name) => metric.set_name(name),
            MetricId::Alias(alias) => metric.set_alias(alias),
        };
        metric.set_value(val.value.into());
        metric.timestamp = val.timestamp;
        metric
    }
}

/// Information about a metric provided from a birth message  
#[derive(Debug)]
pub struct MetricBirthDetails {
    /// The name of the metric
    pub name: String,
    /// An optional alias. If set, all future CMD publishes should use this value when referring to the metric.
    pub alias: Option<u64>,
    /// The datatype of the metric.
    pub datatype: DataType,
}

impl MetricBirthDetails {
    fn new(name: String, alias: Option<u64>, datatype: DataType) -> Self {
        Self {
            name,
            alias,
            datatype,
        }
    }

    pub fn get_metric_id(&self) -> MetricId {
        if let Some(alias) = self.alias { MetricId::Alias(alias) } else { MetricId::Name(self.name.clone())}
    }
}

/// Information about a metric from a message
#[derive(Debug)]
pub struct MetricDetails {
    /// The value of the metric
    pub value: Option<MetricValue>,
    pub properties: Option<PropertySet>,
    pub metadata: Option<MetaData>,
    /// The timestamp associated with the value of the metric
    pub timestamp: u64,
    /// Is the metric a historical metric
    pub is_historical: bool,
    /// Should the metric be persisted
    pub is_transient: bool,
}

macro_rules! metric_details_try_from_payload_metric {
    ($metric:expr) => {{
        let timestamp = $metric
            .timestamp
            .ok_or(PayloadMetricError::MissingTimestamp)?;
        let value = if let Some(value) = $metric.value {
            Some(value.into())
        } else if let Some(is_null) = $metric.is_null {
            if is_null {
                None
            } else {
                return Err(PayloadMetricError::NotNullNoValue);
            }
        } else {
            return Err(PayloadMetricError::NotNullNoValue);
        };

        let properties = match $metric.properties {
            Some(ps) => match ps.try_into() {
                Ok(properties) => Some(properties),
                Err(_) => return Err(PayloadMetricError::InvalidProperties),
            },
            None => None,
        };

        let metadata = $metric.metadata;
        let is_historical = $metric.is_historical.unwrap_or(false);
        let is_transient = $metric.is_transient.unwrap_or(false);

        Ok(MetricDetails {
            value,
            properties,
            metadata,
            timestamp,
            is_historical,
            is_transient,
        })
    }};
}

pub(crate) fn get_metric_id_and_details_from_payload_metrics(
    metrics: Vec<payload::Metric>,
) -> Result<Vec<(MetricId, MetricDetails)>, PayloadMetricError> {
    let mut metric_id_details = Vec::with_capacity(metrics.len());
    for x in metrics {
        let id = if let Some(alias) = x.alias {
            MetricId::Alias(alias)
        } else if let Some(name) = x.name {
            MetricId::Name(name)
        } else {
            return Err(PayloadMetricError::MissingName);
        };
        let details = metric_details_try_from_payload_metric!(x)?;
        metric_id_details.push((id, details))
    }
    Ok(metric_id_details)
}

pub(crate) fn get_metric_birth_details_from_birth_metrics(
    metrics: Vec<payload::Metric>,
) -> Result<Vec<(MetricBirthDetails, MetricDetails)>, PayloadMetricError> {
    //make sure metrics names and aliases are unique
    let mut results = Vec::with_capacity(metrics.len());

    for x in metrics {
        let datatype = match x
            .datatype
            .ok_or(PayloadMetricError::MissingDatatype)?
            .try_into()
        {
            Ok(datatype) => datatype,
            Err(_) => return Err(PayloadMetricError::InvalidDatatype),
        };

        let name = x.name.ok_or(PayloadMetricError::MissingName)?;
        let alias = x.alias;
        let birth_details = MetricBirthDetails::new(name, alias, datatype);
        let details = metric_details_try_from_payload_metric!(x)?;
        results.push((birth_details, details));
    }

    Ok(results)
}
