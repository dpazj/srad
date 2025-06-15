use crate::{payload::{self, metric, DataType}, traits::{self, HasDataType}, MetricValue};

pub trait TemplateMetricValue {

    fn to_template_metric_value(self) -> Option<MetricValue>;

}

impl<T> TemplateMetricValue for T
where 
    T: traits::MetricValue
{
    fn to_template_metric_value(self) -> Option<MetricValue> {
        Some(T::into(self))
    }
}

impl<T> TemplateMetricValue for Option<T>
where 
    T: traits::MetricValue
{
    fn to_template_metric_value(self) -> Option<MetricValue> {
        self.map(T::into)
    }
}

pub type TemplateMetric = payload::Metric;

impl TemplateMetric {

    pub fn new_template_metric<T: TemplateMetricValue + traits::HasDataType>(name: String, value: T) -> Self {
        TemplateMetric {
            name: Some(name),
            alias: None,
            timestamp: None,
            datatype: Some(T::default_datatype() as u32),
            is_historical: None,
            is_transient: None,
            is_null: None,
            metadata: None,
            properties: None,
            value: value.to_template_metric_value().map(payload::metric::Value::from) 
        }
    }


}

pub type TemplateParameter = payload::template::Parameter;

impl TemplateParameter {

}

#[derive(Debug)]
pub struct TemplateDefinition {
    pub name: String,
    pub version: Option<String>,
    pub metrics: Vec<TemplateMetric>,
    pub parameters: Vec<TemplateParameter>
}

impl From<TemplateDefinition> for payload::Template {
    fn from(value: TemplateDefinition) -> Self {
        payload::Template { 
            version: value.version, 
            metrics: value.metrics, 
            parameters: value.parameters, 
            template_ref: None, 
            is_definition: Some(true)
        }
    }
}

#[derive(Debug)]
pub struct TemplateInstance {
    pub name: String,
    pub version: Option<String>,
    pub metrics: Vec<TemplateMetric>,
    pub parameters: Vec<TemplateParameter>
}

impl From<TemplateInstance> for payload::Template {
    fn from(value: TemplateInstance) -> Self {
        payload::Template { 
            version: value.version, 
            metrics: value.metrics, 
            parameters: value.parameters, 
            template_ref: Some(value.name), 
            is_definition: Some(false)
        }
    }
}

pub trait TemplateMetadata {
    fn template_version() -> Option<&'static str> { None }
    fn template_name() -> &'static str;
}

pub trait Template: TemplateMetadata 
{
    fn template_definition() -> TemplateDefinition;
    fn template_instance(&self) -> TemplateInstance;

    // // for each field compare and if not eq field.to_metric_value()
    // fn template_instance_from_difference(&self, other: &Self) -> TemplateInstance;

    // // for each field provided in instance run field.update_from_metric()
    // fn update_from_instance(&self, instance: TemplateInstance) -> Result<(), ()>;
}

impl<T> From<T> for metric::Value 
where 
    T: Template 
{
    fn from(value: T) -> Self {
        todo!()
    }
}

impl<T> HasDataType for T 
where 
    T: Template 
{
    fn supported_datatypes() -> &'static [DataType] {
        static SUPPORTED_TYPES: [DataType;1] = [DataType::Template];
        &SUPPORTED_TYPES
    }
}


// /// Trait used to update a value from a [value::MetricValue]
// pub trait TemplateMetricValueUpdatable{
//     fn update_from_metric_value(&mut self, metric_value: MetricValue) -> Result<(), ()>;
// }

// impl<T> TemplateMetricValueUpdatable for T 
// where 
//     T: traits::MetricValue
// {
//     fn update_from_metric_value(&mut self, value: MetricValue) -> Result<(), ()> {
//         let res = Self::try_from(value).map_err(|e| return ())?;
//         *self = res;
//         Ok(())
//     }
// }

