use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

use srad_types::{
    payload::{DataType, Metric, ToMetric},
    traits,
    utils::timestamp,
    MetaData, MetricId, MetricValue, PropertySet,
};

use crate::{error::Error, metric::MetricToken, registry::DeviceId};

/// Details about a metric to be included in a birth message
pub struct BirthMetricDetails<T> {
    name: String,
    use_alias: bool,
    datatype: DataType,
    initial_value: Option<T>,
    metadata: Option<MetaData>,
    properties: Option<PropertySet>,
    timestamp: u64,
}

impl<T> BirthMetricDetails<T>
where
    T: traits::MetricValue,
{
    fn new(name: String, initial_value: Option<T>, datatype: DataType) -> Self {
        Self {
            name: name,
            use_alias: true,
            datatype: datatype,
            initial_value: initial_value,
            metadata: None,
            properties: None,
            timestamp: timestamp(),
        }
    }

    pub fn new_with_initial_value<S: Into<String>>(name: S, initial_value: T) -> Self {
        Self::new(name.into(), Some(initial_value), T::default_datatype())
    }

    fn new_with_explicit_datatype(
        name: String,
        datatype: DataType,
        initial_value: Option<T>,
    ) -> Result<Self, ()> {
        if !T::supported_datatypes().contains(&datatype) {
            return Err(());
        }
        Ok(Self::new(name.into(), initial_value, datatype))
    }

    pub fn new_with_initial_value_explicit_type<S: Into<String>>(
        name: S,
        initial_value: T,
        datatype: DataType,
    ) -> Result<Self, ()> {
        Self::new_with_explicit_datatype(name.into(), datatype, Some(initial_value))
    }

    pub fn new_without_initial_value<S: Into<String>>(
        name: S,
        datatype: DataType,
    ) -> Result<Self, ()> {
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
where
    T: traits::MetricValue,
{
    fn to_metric(self) -> Metric {
        let mut birth_metric = Metric::new();
        birth_metric.set_name(self.name).set_datatype(self.datatype);
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

enum AliasType {
    Node,
    Device { id: DeviceId },
}

pub enum BirthObjectType {
    Node,
    Device(DeviceId),
}

pub struct BirthInitializer {
    birth_metrics: Vec<Metric>,
    metric_names: HashSet<String>,
    metric_aliases: HashSet<u64>,
    inserter_type: BirthObjectType,
}

impl BirthInitializer {
    pub(crate) fn new(inserter_type: BirthObjectType) -> Self {
        Self {
            birth_metrics: Vec::new(),
            metric_names: HashSet::new(),
            metric_aliases: HashSet::new(),
            inserter_type: inserter_type,
        }
    }

    fn generate_alias(&mut self, alias_type: AliasType, metric_name: &String) -> u64 {
        let mut hasher = DefaultHasher::new();
        metric_name.hash(&mut hasher);
        let hash = hasher.finish() as u32;
        let id_part = match alias_type {
            AliasType::Node => 0,
            AliasType::Device { id } => id,
        };
        let mut alias = ((id_part as u64) << 32) | (hash as u64);
        while self.metric_aliases.contains(&alias) {
            alias += 1;
        }
        self.metric_aliases.insert(alias.clone());
        alias
    }

    fn create_metric_token<T: traits::MetricValue>(
        &mut self,
        name: &String,
        use_alias: bool,
    ) -> Result<MetricToken<T>, Error> {
        let metric = name.into();

        if self.metric_names.contains(&metric) {
            return Err(Error::DuplicateMetric);
        }

        let id = match use_alias {
            true => {
                let alias = match &self.inserter_type {
                    BirthObjectType::Node => self.generate_alias(AliasType::Node, &metric),
                    BirthObjectType::Device(id) => {
                        self.generate_alias(AliasType::Device { id: id.clone() }, &metric)
                    }
                };
                MetricId::Alias(alias)
            }
            false => MetricId::Name(metric),
        };

        Ok(MetricToken::new(id))
    }

    pub fn register_metric<T: traits::MetricValue>(
        &mut self,
        details: BirthMetricDetails<T>,
    ) -> Result<MetricToken<T>, Error> {
        let tok = self.create_metric_token(&details.name, details.use_alias)?;
        let mut metric = details.to_metric();
        if let MetricId::Alias(alias) = &tok.id {
            metric.set_alias(*alias);
        }
        self.birth_metrics.push(metric);
        Ok(tok)
    }

    pub(crate) fn finish(self) -> Vec<Metric> {
        self.birth_metrics
    }
}
