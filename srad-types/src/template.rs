use thiserror::Error;

use crate::{
    payload::{self, metric, DataType},
    traits::{self, HasDataType},
    FromValueTypeError, MetricValue, ParameterValue,
};

pub trait TemplateMetricValue {
    type Error;

    fn to_template_metric_value(self) -> Option<MetricValue>;

    fn try_from_template_metric_value(value: Option<MetricValue>) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

impl<T> TemplateMetricValue for T
where
    T: traits::MetricValue,
{
    type Error = ();

    fn to_template_metric_value(self) -> Option<MetricValue> {
        Some(T::into(self))
    }

    fn try_from_template_metric_value(value: Option<MetricValue>) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        match value {
            Some(value) => Self::try_from(value).map_err(|_| ()),
            None => Err(()),
        }
    }
    
}

impl<T> TemplateMetricValue for Option<T>
where
    T: traits::MetricValue,
{
    type Error = ();

    fn to_template_metric_value(self) -> Option<MetricValue> {
        self.map(T::into)
    }

    fn try_from_template_metric_value(value: Option<MetricValue>) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        match value {
            Some(value) => {
                let value = value.try_into().map_err(|_| ())?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

pub trait TemplateParameterValue {
    type Error;
    fn to_template_parameter_value(self) -> Option<ParameterValue>;
    fn try_from_template_parameter_value(value: Option<ParameterValue>) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

impl<T> TemplateParameterValue for T
where
    T: traits::ParameterValue,
{
    type Error = ();

    fn to_template_parameter_value(self) -> Option<ParameterValue> {
        Some(T::into(self))
    }

    fn try_from_template_parameter_value(value: Option<ParameterValue>) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        match value {
            Some(value) => Self::try_from(value).map_err(|_| ()),
            None => Err(()),
        }
    }
    
}

impl<T> TemplateParameterValue for Option<T>
where
    T: traits::ParameterValue,
{
    type Error = ();

    fn to_template_parameter_value(self) -> Option<ParameterValue> {
        self.map(T::into)
    }

    fn try_from_template_parameter_value(value: Option<ParameterValue>) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        match value {
            Some(value) => {
                let value = value.try_into().map_err(|_| ())?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

//A trait to support recursive partial templates
pub trait TemplateMetricValuePartial {
    type Error;
    fn metric_value_if_ne(&self, other: &Self) -> Option<Option<MetricValue>>;
    fn try_update_from_metric_value(&mut self, other: Option<MetricValue>) -> Result<(), Self::Error>;
}

macro_rules! impl_template_metric_value_partial {

    ($($ty:ty),* $(,)?) => {
        $(
            impl TemplateMetricValuePartial for $ty {
                type Error = ();
                fn metric_value_if_ne(&self, other: &Self) -> Option<Option<MetricValue>> {
                    if self == other {
                        return None
                    }
                    Some(Some(self.clone().into()))
                }
                fn try_update_from_metric_value(&mut self, other: Option<MetricValue>) -> Result<(), Self::Error> {
                    *self = <$ty>::try_from_template_metric_value(other)?;
                    Ok(())
                }
            }
        )*
    };

    ($($ty:ty),* $(,)?) => {
        $(
            impl TemplateMetricValuePartial for Vec<$ty> {
                fn metric_value_if_ne(&self, other: &Self) -> Option<Option<MetricValue>> {
                    if self == other {
                        return None
                    }
                    Some(Some(self.clone().into()))
                }
                fn try_update_from_metric_value(&mut self, other: Option<MetricValue>) -> Result<(), ()> {
                    *self = <$ty>::try_from_template_metric_value(other)?;
                    Ok(())
                }
            }
        )*
    };

}

impl_template_metric_value_partial!(bool, i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, String);

impl<T> TemplateMetricValuePartial for Option<T>
where
    T: TemplateMetricValuePartial + TemplateMetricValue + Into<MetricValue> + PartialEq + Clone,
{
    type Error = ();

    fn metric_value_if_ne(&self, other: &Self) -> Option<Option<MetricValue>> {
        if self == other {
            return None;
        }
        Some(self.clone().map(|x| x.into()))
    }

    fn try_update_from_metric_value(&mut self, other: Option<MetricValue>) -> Result<(), Self::Error> {
        *self = match other {
            Some(value) => Some(T::try_from_template_metric_value(Some(value)).map_err(|_|())?),
            None => None,
        };
        Ok(())
    }
}

pub type TemplateMetric = payload::Metric;

impl TemplateMetric {
    pub fn new_template_metric_raw(
        name: String,
        datatype: DataType,
        value: Option<MetricValue>,
    ) -> Self {
        TemplateMetric {
            name: Some(name),
            alias: None,
            timestamp: None,
            datatype: Some(datatype as u32),
            is_historical: None,
            is_transient: None,
            is_null: None,
            metadata: None,
            properties: None,
            value: value.map(payload::metric::Value::from),
        }
    }

    pub fn new_template_metric<T: TemplateMetricValue + traits::HasDataType>(
        name: String,
        value: T,
    ) -> Self {
        Self::new_template_metric_raw(
            name,
            T::default_datatype(),
            value.to_template_metric_value(),
        )
    }
}

pub type TemplateParameter = payload::template::Parameter;

impl TemplateParameter {
    pub fn new_template_parameter<T: TemplateParameterValue + traits::HasDataType>(
        name: String,
        value: T,
    ) -> Self {
        TemplateParameter {
            name: Some(name),
            r#type: Some(T::default_datatype() as u32),
            value: value
                .to_template_parameter_value()
                .map(payload::template::parameter::Value::from),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateDefinition {
    pub version: Option<String>,
    pub metrics: Vec<TemplateMetric>,
    pub parameters: Vec<TemplateParameter>,
}

impl HasDataType for TemplateDefinition {
    fn supported_datatypes() -> &'static [DataType] {
        static SUPPORTED_TYPES: [DataType; 1] = [DataType::Template];
        &SUPPORTED_TYPES
    }
}

impl traits::MetricValue for TemplateDefinition {}

impl From<TemplateDefinition> for payload::Template {
    fn from(value: TemplateDefinition) -> Self {
        payload::Template {
            version: value.version,
            metrics: value.metrics,
            parameters: value.parameters,
            template_ref: None,
            is_definition: Some(true),
        }
    }
}

impl From<TemplateDefinition> for MetricValue {
    fn from(value: TemplateDefinition) -> Self {
        MetricValue::new(metric::Value::TemplateValue(value.into()))
    }
}

impl TryFrom<MetricValue> for TemplateDefinition {
    type Error = FromValueTypeError;

    fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        if let metric::Value::TemplateValue(template) = value.0 {
            // [tck-id-payloads-template-definition-ref] A Template Definition MUST omit the template_ref field
            if template.template_ref.is_some() {
                return Err(FromValueTypeError::InvalidValue(
                    "Template payload violates tck-id-payloads-template-definition-ref".into(),
                ));
            }
            // [tck-id-payloads-template-definition-is-definition] A Template Definition MUST have is_definition set to true.
            if template.is_definition.unwrap_or(false) {
                return Err(FromValueTypeError::InvalidValue(
                    "Template payload violates tck-id-payloads-template-definition-is_definition"
                        .into(),
                ));
            }
            Ok(TemplateDefinition {
                version: template.version,
                metrics: template.metrics,
                parameters: template.parameters,
            })
        } else {
            Err(FromValueTypeError::InvalidVariantType)
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct TemplateInstance {
    //name of the metric that represents the template definition
    pub template_ref: String,
    pub version: Option<String>,
    pub metrics: Vec<TemplateMetric>,
    pub parameters: Vec<TemplateParameter>,
}

impl HasDataType for TemplateInstance {
    fn supported_datatypes() -> &'static [DataType] {
        static SUPPORTED_TYPES: [DataType; 1] = [DataType::Template];
        &SUPPORTED_TYPES
    }
}

impl traits::MetricValue for TemplateInstance {}

impl From<TemplateInstance> for payload::Template {
    fn from(value: TemplateInstance) -> Self {
        payload::Template {
            version: value.version,
            metrics: value.metrics,
            parameters: value.parameters,
            template_ref: Some(value.template_ref),
            is_definition: Some(false),
        }
    }
}

impl From<TemplateInstance> for MetricValue {
    fn from(value: TemplateInstance) -> Self {
        MetricValue::new(metric::Value::TemplateValue(value.into()))
    }
}

impl TryFrom<MetricValue> for TemplateInstance {
    type Error = FromValueTypeError;

    fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        if let metric::Value::TemplateValue(template) = value.0 {
            //[tck-id-payloads-template-instance-is-definition] A Template Instance MUST have is_definition set to false.
            if template.is_definition.unwrap_or(true) {
                return Err(FromValueTypeError::InvalidValue(
                    "Template payload violates tck-id-payloads-template-instance-is_definition"
                        .into(),
                ));
            }
            let template_ref = template
                .template_ref
                .ok_or(FromValueTypeError::InvalidValue(
                    "Template payload violates tck-id-payloads-template-instance-ref".into(),
                ))?;
            Ok(TemplateInstance {
                template_ref,
                version: template.version,
                metrics: template.metrics,
                parameters: template.parameters,
            })
        } else {
            Err(FromValueTypeError::InvalidVariantType)
        }
    }
}

#[derive(Debug)]
pub enum TemplateValue {
    Definition(TemplateDefinition),
    Instance(TemplateInstance),
}

impl TryFrom<MetricValue> for TemplateValue {
    type Error = FromValueTypeError;

    fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        if let metric::Value::TemplateValue(template) = &value.0 {
            let is_def = match template.is_definition {
                Some(is_def) => is_def,
                None => {
                    return Err(FromValueTypeError::InvalidValue(
                        "Template field template_ref cannot be None".into(),
                    ))
                }
            };
            Ok(match is_def {
                true => TemplateValue::Definition(TemplateDefinition::try_from(value)?),
                false => TemplateValue::Instance(TemplateInstance::try_from(value)?),
            })
        } else {
            Err(FromValueTypeError::InvalidVariantType)
        }
    }
}

/// Provides metadata information for a Template
///
/// This trait defines basic metadata for Sparkplug templates
pub trait TemplateMetadata {
    ///Provide an optional version for the template
    fn template_version() -> Option<&'static str> {
        None
    }

    ///Provide the name of the template
    fn template_name() -> &'static str;

    ///Provides a name that will be used as the metric name for the templates definition in a birth message.
    ///
    /// This should be unique in the context of an Edge Node.
    /// By default, this method combines the template name and version (if available) to create
    /// a metric name that follows Sparkplug conventions. The format is:
    /// - With version: `"template_name:version"`
    /// - Without version: `"template_name"`
    fn template_definition_metric_name() -> String {
        let version = Self::template_version();
        match version {
            Some(version) => format!("{}:{}", Self::template_name(), version),
            None => Self::template_name().into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("Invalid Template Payload")]
    InvalidPayload,
    #[error("Unexpected Parameter: {0}")]
    UnknownParameter(String),
    #[error("Unexpected Metric: {0}")]
    UnknownMetric(String),
    #[error("Template ref mismatch: {0}")]
    RefMismatch(String),
    #[error("Template version mismatch")]
    VersionMismatch,
    #[error("Invalid value for parameter field {0}")]
    InvalidParameterValue(String),
    #[error("Invalid value for metric field {0}")]
    InvalidMetricValue(String),
}

/// Trait used to represent a Template
///
/// **It is strongly recommended to use the [srad_macros::Template] derive macro instead of
/// implementing this trait manually**
pub trait Template: TemplateMetadata + TryFrom<TemplateInstance> {
    /// Returns the template definition
    fn template_definition() -> TemplateDefinition;
    /// Creates a template instance from the current state
    fn template_instance(&self) -> TemplateInstance;
}

impl<T> HasDataType for T
where
    T: Template,
{
    fn supported_datatypes() -> &'static [DataType] {
        static SUPPORTED_TYPES: [DataType; 1] = [DataType::Template];
        &SUPPORTED_TYPES
    }
}

/// Trait used to represent a Template implementation that supports generating partial template instances
/// and is updatable from partial template instances
///
/// **It is strongly recommended to use the [srad_macros::Template] derive macro instead of
/// implementing this trait manually**
pub trait PartialTemplate: Template {
    /// Create a template instance based on the difference between a template and another copy
    fn template_instance_from_difference(&self, other: &Self) -> Option<TemplateInstance>;
    /// Update the template from a [TemplateInstance]
    fn update_from_instance(&mut self, instance: TemplateInstance) -> Result<(), TemplateError>;
}
