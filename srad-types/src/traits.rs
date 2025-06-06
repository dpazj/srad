use crate::{metadata::MetaData, payload::{self, DataType}, value};

/// Trait used to query the Sparkplug datatype(s) that an implementing type supports
pub trait HasDataType {
    /// Get all the Sparkplug [crate::payload::DataType]'s the type supports
    fn supported_datatypes() -> &'static [DataType];

    /// Default [crate::payload::DataType] the type maps to
    fn default_datatype() -> DataType {
        let supported = Self::supported_datatypes();
        if supported.is_empty() {
            panic!("supported_datatypes result has to contain at least one element")
        }
        supported[0]
    }
}

/// Trait used to represent that a type can represent a [value::MetricValue]
pub trait MetricValue:
    TryFrom<value::MetricValue> + Into<value::MetricValue> + HasDataType
{
    fn birth_metadata(&self) -> Option<MetaData> {
        self.publish_metadata()
    }
    fn publish_metadata(&self) -> Option<MetaData> {
        None
    }
}

/// Trait used to represent that a type can represent a [value::PropertyValue]
pub trait PropertyValue:
    TryFrom<value::PropertyValue> + Into<value::PropertyValue> + HasDataType
{
}

/// Trait used to represent that a type can represent a [value::DataSetValue]
pub trait DataSetValue:
    TryFrom<value::DataSetValue> + Into<value::DataSetValue> + HasDataType
{
}

/// Trait used to represent that a type can represent a [value::ParameterValue]
pub trait ParameterValue:
    TryFrom<value::ParameterValue> + Into<value::ParameterValue> + HasDataType
{
}


pub trait Template 
{
    fn template_definition() -> payload::Template;
    fn template_instance(&self) -> payload::Template;
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
