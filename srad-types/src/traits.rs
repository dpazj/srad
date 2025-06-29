use crate::{
    metadata::MetaData,
    payload::{self, DataType},
    value,
};

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

impl<T> HasDataType for Option<T>
where
    T: HasDataType,
{
    fn supported_datatypes() -> &'static [DataType] {
        T::supported_datatypes()
    }

    fn default_datatype() -> DataType {
        T::default_datatype()
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
