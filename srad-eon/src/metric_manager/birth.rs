use srad_types::{payload::{DataType, Metric, ToMetric}, property_set::PropertySet, traits::{self, MetaData}, MetricId, MetricValue};

use crate::{
  error::SpgError, metric::MetricToken, registry::{MetricRegistry, MetricRegistryInserter, MetricRegistryInserterType, MetricValidToken}, utils::timestamp
};

pub struct BirthMetricDetails<T> {
  pub name: String,
  use_alias: bool,
  datatype: DataType,
  initial_value: Option<T>,
  metadata: Option<MetaData>,
  properties: Option<PropertySet>,
  timestamp: u64
}

impl<T> BirthMetricDetails<T> 
where T: traits::MetricValue
{

  fn new(name: String, initial_value:Option<T>, datatype: DataType) -> Self {
    Self {
      name: name,
      use_alias: true,
      datatype: datatype,
      initial_value: initial_value, 
      metadata: None,
      properties: None,
      timestamp: timestamp()
    }
  }

  pub fn new_with_initial_value<S: Into<String>>(name :S, initial_value: T) -> Self
  {
    Self::new(name.into(), Some(initial_value), T::default_datatype())
  }

  fn new_with_explicit_datatype(name :String, datatype: DataType, initial_value: Option<T>) -> Result<Self, ()>
  {
    if !T::supported_datatypes().contains(&datatype) { return Err(()) }
    Ok(Self::new(name.into(), initial_value, datatype))
  }

  pub fn new_with_initial_value_explicit_type<S: Into<String>>(name :S, initial_value: T, datatype: DataType) -> Result<Self, ()>
  {
    Self::new_with_explicit_datatype(name.into(), datatype, Some(initial_value))
  }
  
  pub fn new_without_initial_value<S: Into<String>>(name :S, datatype: DataType) -> Result<Self, ()>
  {
    Self::new_with_explicit_datatype(name.into(), datatype, None)
  }

  pub fn use_alias(mut self, use_alias: bool) -> Self {
    self.use_alias = use_alias;
    self
  }

  pub fn with_timestamp(mut self, timestamp: u64) -> Self {
    self.timestamp = timestamp;
    self
  }

  pub fn with_metadata(mut self, metadata: MetaData) -> Self {
    self.metadata = Some(metadata);
    self
  }

  pub fn with_properties<P: Into<PropertySet>>(mut self, properties: P) -> Self {
    self.properties = Some(properties.into());
    self
  }

}

impl<T> ToMetric for BirthMetricDetails<T> 
where T: traits::MetricValue
{
  fn to_metric(self) -> Metric {
    let mut birth_metric = Metric::new();
    birth_metric
      .set_name(self.name)
      .set_datatype(self.datatype);
    birth_metric.timestamp = Some(self.timestamp);
    birth_metric.metadata = self.metadata.map(MetaData::into);

    if let Some(val) = self.initial_value {
      let val: MetricValue = val.into();
      birth_metric.set_value(val.into());
    }

    birth_metric.properties = self.properties.map(PropertySet::into); 

    birth_metric
  }
}

pub struct BirthInitializer<'a> {
  registry: MetricRegistryInserter<'a>,
  birth_metrics: Vec<Metric>
}

impl<'a> BirthInitializer<'a>{

  pub(crate) fn new(inserter_type: MetricRegistryInserterType, registry: &'a mut MetricRegistry) -> Self{
    Self {
      registry: MetricRegistryInserter::new(inserter_type, registry),
      birth_metrics: Vec::new()
    } 
  }

  pub fn create_metric<T: traits::MetricValue>(&mut self, details: BirthMetricDetails<T>) -> Result<MetricToken<T>, SpgError>
  {
    let id = self.registry.register_metric(&details.name, details.datatype, details.use_alias)?;
    let mut metric = details.to_metric();
    if let MetricId::Alias(alias) = id.id() {
      metric.set_alias(*alias);
    }
    self.birth_metrics.push(metric); 
    Ok(id)
  }

  pub(crate) fn finish(self) -> (Vec<Metric>, MetricValidToken ){
    let tok = self.registry.finish();
    (self.birth_metrics, tok)
  }
}
