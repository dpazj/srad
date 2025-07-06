use std::marker::PhantomData;
use std::vec::IntoIter;

use log::warn;
use srad_types::payload::{Metric, Payload};
use srad_types::utils::timestamp;
use srad_types::MetaData;
use srad_types::PropertySet;
use srad_types::{traits, MetricId, MetricValue};

use thiserror::Error;

use crate::StateError;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("No metrics provided.")]
    NoMetrics,
    #[error("State Error: {0}.")]
    State(StateError),
}

impl From<StateError> for PublishError {
    fn from(value: StateError) -> Self {
        PublishError::State(value)
    }
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
        metrics.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
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
        metrics.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        self.publish_metrics_unsorted(metrics)
    }
}

/// A structure for creating a metric to be published with associated metadata and properties.
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

impl<T> MetricToken<T>
{
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
    type Error = ();

    fn try_from(value: Metric) -> Result<Self, Self::Error> {
        let id = if let Some(alias) = value.alias {
            MetricId::Alias(alias)
        } else if let Some(name) = value.name {
            MetricId::Name(name)
        } else {
            return Err(());
        };

        let metric_value = if value.value.is_some() {
            value.value.map(MetricValue::from)
        } else if let Some(is_null) = value.is_null {
            if is_null {
                return Err(());
            }
            None
        } else {
            return Err(());
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
                Err(_) => {
                    warn!("Got invalid or badly formed metric - skipping");
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
    type Error = ();

    fn try_from(value: Payload) -> Result<Self, Self::Error> {
        /*
        tck-id-payloads-ncmd-timestamp and tck-id-payloads-cmd-timestamp
        messages MUST include a payload timestamp that denotes the time at which the message was published.
        */
        let timestamp = match value.timestamp {
            Some(timestamp) => timestamp,
            None => return Err(()),
        };

        Ok(MessageMetrics {
            timestamp,
            metrics: value.metrics,
        })
    }
}
