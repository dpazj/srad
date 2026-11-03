use std::marker::PhantomData;
use std::vec::IntoIter;

use log::warn;
use srad_types::payload::{Metric, Payload};
use srad_types::utils::timestamp;
use srad_types::PropertySet;
use srad_types::{traits, MetricId, MetricValue};
use srad_types::{MetaData, PartialTemplate, Template};

use thiserror::Error;

use crate::StateError;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("No metrics provided.")]
    NoMetrics,
    #[error("State Error: {0}.")]
    State(#[from] StateError),
    #[error("Client Error: {0}.")]
    Client(#[from] srad_client::ClientError),
}

/// A trait for publishing metrics to the network.
///
/// `MetricPublisher` defines a set of methods for publishing single metrics
/// or batches of metrics. It provides "try_" variants that may fail immediately.
///
/// "try_publish" variants will use the "try_publish" variants from the [srad_client::Client] trait.
/// Similarly, the "publish" variants will use the "publish" from the [srad_client::Client] trait.
pub trait MetricPublisher {
    /// Attempts to publish a batch of metrics without modifying their order.
    fn try_publish_metrics_unsorted(
        &self,
        metrics: Vec<PublishMetric>,
    ) -> impl std::future::Future<Output = Result<(), PublishError>> + Send;

    /// Attempts to publish a single metric.
    fn try_publish_metric(
        &self,
        metric: PublishMetric,
    ) -> impl std::future::Future<Output = Result<(), PublishError>> + Send {
        self.try_publish_metrics_unsorted(vec![metric])
    }

    /// Attempts to publish a batch of metrics after sorting by timestamp.
    fn try_publish_metrics(
        &self,
        mut metrics: Vec<PublishMetric>,
    ) -> impl std::future::Future<Output = Result<(), PublishError>> + Send {
        metrics.sort_by_key(|a| a.timestamp);
        self.publish_metrics_unsorted(metrics)
    }

    /// Publish a batch of metrics without modifying their order.
    fn publish_metrics_unsorted(
        &self,
        metrics: Vec<PublishMetric>,
    ) -> impl std::future::Future<Output = Result<(), PublishError>> + Send;

    /// Publish a single metric.
    fn publish_metric(
        &self,
        metric: PublishMetric,
    ) -> impl std::future::Future<Output = Result<(), PublishError>> + Send {
        self.publish_metrics_unsorted(vec![metric])
    }

    /// Publish a batch of metrics after sorting by timestamp.
    fn publish_metrics(
        &self,
        mut metrics: Vec<PublishMetric>,
    ) -> impl std::future::Future<Output = Result<(), PublishError>> + Send {
        metrics.sort_by_key(|a| a.timestamp);
        self.publish_metrics_unsorted(metrics)
    }
}

/// A struct for creating a metric to be published with associated metadata and properties.
///
/// `PublishMetric` provides a builder pattern for configuring metric publications,
/// allowing for optional fields like transience, historical status, custom timestamps,
/// metadata, and properties.
pub struct PublishMetric {
    metric_identifier: MetricId,
    value: Option<MetricValue>,
    is_transient: Option<bool>,
    is_historical: Option<bool>,
    timestamp: u64,
    metadata: Option<MetaData>,
    properties: Option<PropertySet>,
}

impl PublishMetric {
    pub(crate) fn new<T: traits::MetricValue>(
        metric_identifier: MetricId,
        value: Option<T>,
    ) -> Self {
        let metadata = if let Some(v) = &value {
            v.publish_metadata()
        } else {
            None
        };
        Self {
            metric_identifier,
            metadata,
            value: value.map(T::into),
            is_transient: None,
            is_historical: None,
            properties: None,
            timestamp: timestamp(),
        }
    }
    /// Sets a custom timestamp for the metric.
    ///
    /// By default, the current system time is used.
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Marks the metric as transient or persistent.
    ///
    /// Transient metrics are typically not stored permanently. By default metrics are not transient.
    pub fn transient(mut self, is_transient: bool) -> Self {
        self.is_transient = Some(is_transient);
        self
    }

    /// Marks the metric as a historical metric that does not represent a current value.
    ///
    /// By default, metrics are not historical.
    pub fn historical(mut self, is_historical: bool) -> Self {
        self.is_historical = Some(is_historical);
        self
    }

    /// Sets custom metadata for the metric.
    ///
    /// By default, the result from [MetricValue::publish_metadata][srad_types::traits::MetricValue::publish_metadata]  will be used.
    pub fn metadata(mut self, metadata: MetaData) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Sets custom properties for the metric.
    pub fn properties<P: Into<PropertySet>>(mut self, properties: P) -> Self {
        self.properties = Some(properties.into());
        self
    }
}

impl From<PublishMetric> for Metric {
    fn from(value: PublishMetric) -> Self {
        let mut metric = Metric::new();
        match value.metric_identifier {
            MetricId::Name(name) => metric.set_name(name),
            MetricId::Alias(alias) => metric.set_alias(alias),
        };

        metric.metadata = value.metadata.map(MetaData::into);

        if let Some(val) = value.value {
            metric.set_value(val.into());
        }

        metric.timestamp = Some(value.timestamp);
        metric.properties = value.properties.map(PropertySet::into);

        metric.is_historical = value.is_historical;
        metric.is_transient = value.is_transient;

        metric
    }
}

/// A token representing a birthed metric
///
/// Used to create a [PublishMetric] for publishing and match a [MessageMetric] identifier
pub struct MetricToken<T> {
    phantom: PhantomData<T>,
    /// The unique identifier of the metric
    pub id: MetricId,
}

impl<T> MetricToken<T> {
    pub(crate) fn new(id: MetricId) -> Self {
        Self {
            phantom: PhantomData,
            id,
        }
    }
}

impl<T> MetricToken<T>
where
    T: traits::MetricValue,
{
    /// Create a new [PublishMetric]
    pub fn create_publish_metric(&self, value: Option<T>) -> PublishMetric {
        PublishMetric::new(self.id.clone(), value)
    }
}

impl<T> MetricToken<T>
where
    T: Template,
{
    /// Create a new [PublishMetric]
    pub fn create_publish_template_metric(&self, value: T) -> PublishMetric {
        PublishMetric::new(self.id.clone(), Some(value.template_instance()))
    }
}

impl<T> MetricToken<T>
where
    T: Template + PartialTemplate,
{
    pub fn create_publish_template_metric_from_difference(
        &self,
        value: T,
        other: &T,
    ) -> Option<PublishMetric> {
        let diff = value.template_instance_from_difference(other)?;
        Some(PublishMetric::new(self.id.clone(), Some(diff)))
    }
}

/// A collection of metrics from a message
pub struct MessageMetrics {
    /// The timestamp of the payload
    pub timestamp: u64,
    metrics: Vec<Metric>,
}

impl MessageMetrics {
    pub fn len(&self) -> usize {
        self.metrics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metrics.len() == 0
    }

    /// Create metrics from a payload, using the current time if the payload
    /// does not include a timestamp.
    ///
    /// Some Sparkplug implementations omit the payload timestamp on command
    /// messages. The command is still actionable, so use the receive time
    /// instead of rejecting it.
    #[must_use]
    pub fn from_payload_lenient(value: Payload) -> Self {
        let timestamp = value.timestamp.unwrap_or_else(timestamp);
        Self {
            timestamp,
            metrics: value.metrics,
        }
    }
}

/// Errors that can occur when converting a Sparkplug [Metric] into a [MessageMetric].
#[derive(Debug, Error)]
pub enum MessageMetricError {
    #[error("metric has neither a name nor an alias")]
    MissingIdentifier,
    #[error("metric is marked null")]
    NullMetric,
    #[error("metric has no value and does not set is_null")]
    MissingValue,
}

/// Errors that can occur when converting a Sparkplug payload into [MessageMetrics].
#[derive(Debug, Error)]
pub enum MessageMetricsError {
    #[error("payload is missing a timestamp")]
    MissingTimestamp,
}

/// A metric from a message
pub struct MessageMetric {
    /// The unique identifier of the metric
    pub id: MetricId,
    pub timestamp: Option<u64>,
    pub value: Option<MetricValue>,
    pub properties: Option<PropertySet>,
}

impl TryFrom<Metric> for MessageMetric {
    type Error = MessageMetricError;

    fn try_from(value: Metric) -> Result<Self, Self::Error> {
        let id = if let Some(alias) = value.alias {
            MetricId::Alias(alias)
        } else if let Some(name) = value.name {
            MetricId::Name(name)
        } else {
            return Err(MessageMetricError::MissingIdentifier);
        };

        let metric_value = if value.value.is_some() {
            value.value.map(MetricValue::from)
        } else if let Some(is_null) = value.is_null {
            if is_null {
                return Err(MessageMetricError::NullMetric);
            }
            None
        } else {
            return Err(MessageMetricError::MissingValue);
        };

        Ok(MessageMetric {
            id,
            timestamp: value.timestamp,
            value: metric_value,
            properties: None,
        })
    }
}

pub struct MessageMetricsIterator {
    metric_iter: IntoIter<Metric>,
}

impl Iterator for MessageMetricsIterator {
    type Item = MessageMetric;

    fn next(&mut self) -> Option<Self::Item> {
        let metric = self.metric_iter.next();
        match metric {
            Some(metric) => match metric.try_into() {
                Ok(message_metric) => Some(message_metric),
                Err(e) => {
                    warn!("Got invalid or badly formed metric - skipping: {e}");
                    self.next()
                }
            },
            None => None,
        }
    }
}

impl IntoIterator for MessageMetrics {
    type Item = MessageMetric;

    type IntoIter = MessageMetricsIterator;

    fn into_iter(self) -> Self::IntoIter {
        MessageMetricsIterator {
            metric_iter: self.metrics.into_iter(),
        }
    }
}

impl TryFrom<Payload> for MessageMetrics {
    type Error = MessageMetricsError;

    fn try_from(value: Payload) -> Result<Self, Self::Error> {
        /*
        tck-id-payloads-ncmd-timestamp and tck-id-payloads-cmd-timestamp
        messages MUST include a payload timestamp that denotes the time at which the message was published.
        */
        let timestamp = match value.timestamp {
            Some(timestamp) => timestamp,
            None => return Err(MessageMetricsError::MissingTimestamp),
        };

        Ok(MessageMetrics {
            timestamp,
            metrics: value.metrics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_without_identifier_is_rejected() {
        assert!(matches!(
            MessageMetric::try_from(Metric::new()),
            Err(MessageMetricError::MissingIdentifier)
        ));
    }

    #[test]
    fn null_metric_is_rejected() {
        let mut metric = Metric::new();
        metric.set_name("metric".into());
        metric.set_null();
        assert!(matches!(
            MessageMetric::try_from(metric),
            Err(MessageMetricError::NullMetric)
        ));
    }

    #[test]
    fn metric_without_value_is_rejected() {
        let mut metric = Metric::new();
        metric.set_name("metric".into());
        assert!(matches!(
            MessageMetric::try_from(metric),
            Err(MessageMetricError::MissingValue)
        ));
    }

    #[test]
    fn payload_without_timestamp_is_rejected() {
        assert!(matches!(
            MessageMetrics::try_from(Payload::default()),
            Err(MessageMetricsError::MissingTimestamp)
        ));
    }

    #[test]
    fn payload_without_timestamp_can_be_read_leniently() {
        let mut metric = Metric::new();
        metric.set_name("metric".into());
        let payload = Payload {
            metrics: vec![metric],
            ..Default::default()
        };

        let metrics = MessageMetrics::from_payload_lenient(payload);
        assert!(metrics.timestamp > 0);
        assert_eq!(metrics.len(), 1);
    }
}
