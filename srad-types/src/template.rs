use crate::{payload::{self, metric, DataType}, traits::{self, HasDataType}, MetricValue as MetricValue, ParameterValue};


type TemplateMetric = payload::Metric;

impl TemplateMetric {

}

type TemplateParameter = payload::template::Parameter;

impl TemplateParameter {

}

pub struct TemplateDefinition {
    name: String,
    version: Option<String>,
    metrics: Vec<TemplateMetric>,
    parameters: Vec<TemplateParameter>
}

impl From<TemplateDefinition> for payload::Template {
    fn from(value: TemplateDefinition) -> Self {
        todo!()
    }
}

pub struct TemplateInstance {
    name: String,
    version: Option<String>,
    metrics: Vec<TemplateMetric>,
    parameters: Vec<TemplateParameter>
}

impl From<TemplateInstance> for payload::Template {
    fn from(value: TemplateInstance) -> Self {
        todo!()
    }
}

pub trait TemplateMetadata {
    fn version() -> Option<&'static str>;
    fn name() -> &'static str;
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

