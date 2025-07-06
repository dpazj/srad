use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use srad_types::{
    payload::{DataType, Metric},
    traits,
    utils::timestamp,
    MetaData, MetricId, MetricValue, PropertySet, Template,
};

use crate::{device::DeviceId, metric::MetricToken, node::TemplateRegistry};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BirthMetricError {
    #[error("Duplicate metric")]
    DuplicateMetric,
    #[error("The provided type does not support that datatype")]
    MetricValueDatatypeMismatch,
    #[error("The datatype is unsupported")]
    UnsupportedDatatype,
    #[error("A value was expected but not provided")]
    ValueNotProvided,
    #[error("The provided template uses a definition that has not been registered with the node.")]
    UnregisteredTemplate,
}


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
{
    fn new(name: String, initial_value: Option<T>, datatype: DataType) -> Self {
        Self {
            name,
            use_alias: true,
            datatype,
            initial_value,
            metadata: None,
            properties: None,
            timestamp: timestamp(),
        }
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

    fn into_metric_value(self, value: Option<MetricValue>) -> Metric {

        let mut birth_metric = Metric::new();
        birth_metric
            .set_name(self.name)
            .set_datatype(self.datatype);
        birth_metric.timestamp = Some(self.timestamp);
        birth_metric.metadata = self.metadata.map(MetaData::into);
        birth_metric.value = value.map(MetricValue::into);
        birth_metric.properties = self.properties.map(PropertySet::into);
        birth_metric

    }

}

impl<T> BirthMetricDetails<T>
where
    T: traits::MetricValue,
{

    pub fn new_with_initial_value<S: Into<String>>(name: S, initial_value: T) -> Self {
        Self::new(name.into(), Some(initial_value), T::default_datatype())
    }

    fn new_with_explicit_datatype(
        name: String,
        datatype: DataType,
        initial_value: Option<T>,
    ) -> Result<Self, BirthMetricError> {
        if !T::supported_datatypes().contains(&datatype) {
            return Err(BirthMetricError::MetricValueDatatypeMismatch);
        }
        Ok(Self::new(name, initial_value, datatype))
    }

    pub fn new_with_initial_value_explicit_type<S: Into<String>>(
        name: S,
        initial_value: T,
        datatype: DataType,
    ) -> Result<Self, BirthMetricError> {
        Self::new_with_explicit_datatype(name.into(), datatype, Some(initial_value))
    }

    pub fn new_without_initial_value<S: Into<String>>(
        name: S,
        datatype: DataType,
    ) -> Result<Self, BirthMetricError> {
        Self::new_with_explicit_datatype(name.into(), datatype, None)
    }

}

impl<T> BirthMetricDetails<T>
where
    T: Template 
{
    pub fn new_template_metric<S: Into<String>>(name: S, value: T) -> Self {
        Self::new(name.into(), Some(value), DataType::Template)
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
    template_registry: Arc<TemplateRegistry>,
}

impl BirthInitializer {
    pub(crate) fn new(
        inserter_type: BirthObjectType,
        template_registry: Arc<TemplateRegistry>,
    ) -> Self {
        Self {
            birth_metrics: Vec::new(),
            metric_names: HashSet::new(),
            metric_aliases: HashSet::new(),
            inserter_type,
            template_registry,
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
        self.metric_aliases.insert(alias);
        alias
    }

    fn create_metric_token<T>(
        &mut self,
        name: &String,
        use_alias: bool,
    ) -> Result<MetricToken<T>, BirthMetricError> {
        let metric = name.into();

        if self.metric_names.contains(&metric) {
            return Err(BirthMetricError::DuplicateMetric);
        }

        let id = match use_alias {
            true => {
                let alias = match &self.inserter_type {
                    BirthObjectType::Node => self.generate_alias(AliasType::Node, &metric),
                    BirthObjectType::Device(id) => {
                        self.generate_alias(AliasType::Device { id: *id }, &metric)
                    }
                };
                MetricId::Alias(alias)
            }
            false => MetricId::Name(metric),
        };

        Ok(MetricToken::new(id))
    }

    pub fn register_metric<T>(
        &mut self,
        mut details: BirthMetricDetails<T>,
    ) -> Result<MetricToken<T>, BirthMetricError> where 
        T: traits::MetricValue
    {
        if details.datatype == DataType::Template {
            debug_assert!(false, "Cannot register Template datatypes through this api");
            return Err(BirthMetricError::UnsupportedDatatype)
        }
        let tok = self.create_metric_token(&details.name, details.use_alias)?;
        let value = details.initial_value.take().map(T::into);
        let mut metric = details.into_metric_value(value);
        if let MetricId::Alias(alias) = &tok.id {
            metric.set_alias(*alias);
        }
        self.birth_metrics.push(metric);
        Ok(tok)
    }

    pub fn register_template_metric<T>(
        &mut self,
        mut details: BirthMetricDetails<T>,
    ) -> Result<MetricToken<T>, BirthMetricError> where 
        T: Template
    {
        let template_instance = match details.initial_value.take() {
            Some(value) => value.template_instance(),
            //we should never really get here since BirthMetricDetails does not allow 
            //the provision of a template metric without an initial value
            None => return Err(BirthMetricError::ValueNotProvided), 
        }; 

        if !self.template_registry.contains(&template_instance.template_ref) {
            return Err(BirthMetricError::UnregisteredTemplate);
        }

        let tok = self.create_metric_token(&details.name, details.use_alias)?;
        let mut metric: Metric = details.into_metric_value(Some(template_instance.into()));
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
