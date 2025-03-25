use std::marker::PhantomData;
use std::vec::IntoIter;

use srad_types::payload::ToMetric;
use srad_types::payload::{Metric, Payload};
use srad_types::property_set::PropertySet;
use srad_types::traits::MetaData;
use srad_types::utils::timestamp;
use srad_types::{traits, MetricId, MetricValue};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublishError {
  #[error("Connection state is Offline")]
  Offline,
  #[error("No metrics provided.")]
  NoMetrics,
  #[error("The node or device is not birthed.")]
  UnBirthed,
}

pub trait MetricPublisher {

  fn publish_metrics_unsorted(&self, metrics: Vec<PublishMetric>) -> impl std::future::Future<Output = Result<(),PublishError>> + Send;
  fn publish_metric(&self, metric: PublishMetric) -> impl std::future::Future<Output = Result<(),PublishError>> + Send {
    self.publish_metrics_unsorted(vec![metric])
  }
  fn publish_metrics(&self, mut metrics: Vec<PublishMetric>) -> impl std::future::Future<Output = Result<(),PublishError>> + Send {
    metrics.sort_by(|a,b| a.timestamp.cmp(&b.timestamp));
    self.publish_metrics_unsorted(metrics)
  }

}

pub struct PublishMetric 
{
  metric_identifier: MetricId,
  value: Option<MetricValue>,
  is_transient: Option<bool>,
  is_historical: Option<bool>,
  timestamp: u64,
  metadata: Option<MetaData>,
  properties: Option<PropertySet>
}

impl PublishMetric {

  pub(crate) fn new<T: traits::MetricValue> (metric_identifier: MetricId, value: Option<T>) -> Self {
    let metadata = if let Some(v) = &value { v.publish_metadata() } else {None};
    Self {
      metric_identifier,
      metadata: metadata,
      value: value.map(T::into),
      is_transient: None, 
      is_historical: None,
      properties: None,
      timestamp: timestamp()
    }
  }

  pub fn timestamp(mut self, timestamp: u64) -> Self {
    self.timestamp = timestamp;
    self
  }

  pub fn transient(mut self, is_transient: bool) -> Self {
    self.is_transient = Some(is_transient);
    self
  }

  pub fn historical(mut self, is_historical: bool) -> Self {
    self.is_historical = Some(is_historical);
    self
  }

  pub fn metadata_from_value<T: traits::MetricValue> (mut self, val: &T) -> Self {
    self.metadata = val.publish_metadata();
    self
  }

  pub fn metadata(mut self, metadata: MetaData) -> Self {
    self.metadata = Some(metadata);
    self
  }

  pub fn properties<P: Into<PropertySet>>(mut self, properties: P) -> Self {
    self.properties = Some(properties.into());
    self
  }

}

impl ToMetric for PublishMetric {
  fn to_metric(self) -> Metric {

    let mut metric = Metric::new();
    match self.metric_identifier {
      MetricId::Name(name) => metric.set_name(name),
      MetricId::Alias(alias) => metric.set_alias(alias),
    };

    metric.metadata = self.metadata.map(MetaData::into);

    if let Some(val) = self.value {
      let val: MetricValue = val.into();
      metric.set_value(val.into());
    }

    metric.timestamp = Some(self.timestamp);
    metric.properties = self.properties.map(PropertySet::into); 

    metric.is_historical = self.is_historical;
    metric.is_transient = self.is_transient;

    metric
  }
}

pub struct MetricToken<T> {
  phantom: PhantomData<T>,
  id: MetricId,
}

impl<T> MetricToken<T> 
where T: 
  traits::MetricValue
{
  pub(crate) fn new(id: MetricId)-> Self{
    Self {
      phantom: PhantomData,
      id, 
    }
  }

  pub fn create_publish_metric(&self, value: Option<T>) -> PublishMetric {
    PublishMetric::new( self.id.clone(), value)
  }

  pub fn id(&self) -> &MetricId {
    &self.id
  }

}

pub struct MessageMetrics {
  timestamp: u64,
  metrics: Vec<Metric>
}

impl MessageMetrics {

  pub fn len(&self) -> usize { self.metrics.len() }

}

pub struct MessageMetric {
  pub id: MetricId,
  pub timestamp: Option<u64>,
  pub value: Option<MetricValue>,
  pub properties: Option<PropertySet>
}

impl TryFrom<Metric> for MessageMetric {
  type Error = ();

  fn try_from(value: Metric) -> Result<Self, Self::Error> {
    let id = if let Some(alias) = value.alias  {
      MetricId::Alias(alias) 
    } else if let Some(name) = value.name {
      MetricId::Name(name) 
    } else {return Err(())};
  
    let metric_value = if value.value.is_some() {
      value.value.map(MetricValue::from)
    } else if let Some(is_null) = value.is_null {
      if is_null == false {return Err(())}
      None
    } else {return Err(())};

    Ok (MessageMetric { id: id, timestamp: value.timestamp, value: metric_value, properties: None})
  }
}

pub struct MessageMetricsIterator {
  metric_iter: IntoIter<Metric> 
}

impl Iterator for MessageMetricsIterator {
  type Item = MessageMetric;

  fn next(&mut self) -> Option<Self::Item> {
    let metric = self.metric_iter.next();
    match metric {
      Some(metric) => match metric.try_into() {
        Ok(message_metric) => Some(message_metric),
        Err(_) => {
          //TODO log error
          self.next()
        },
      },
      None => None,
    }
  }
}

impl IntoIterator for MessageMetrics {
  type Item = MessageMetric;

  type IntoIter = MessageMetricsIterator;

  fn into_iter(self) -> Self::IntoIter {
    MessageMetricsIterator {metric_iter: self.metrics.into_iter()}  
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

    /*
      tck-id-payloads-ncmd-seq and tck-id-payloads-dcmd-seq
      message MUST NOT include a sequence number.
    */
    if value.seq.is_some() {
      return Err(());
    }

    Ok (MessageMetrics { timestamp: timestamp, metrics: value.metrics })
  }
}
