use crate::{metadata::MetaData, payload::DataType, value};

/// Trait used to query the Sparkplug datatype(s) that an implementing type supports
pub trait HasDataType {
    /// Default [crate::payload::DataType] the type maps to.
    ///
    /// This must be included in [`Self::SUPPORTED_DATATYPES`].
    const DEFAULT_DATATYPE: DataType;

    /// All Sparkplug [crate::payload::DataType]'s the type supports.
    ///
    /// This is an associated const rather than a function so it can be used in const
    /// contexts, e.g. compile-time validation in derive macros.
    const SUPPORTED_DATATYPES: &'static [DataType];
}

/// Returns whether `datatype` is contained in `supported`.
///
/// [`slice::contains`] is not callable from const contexts, so this is the const-compatible
/// equivalent used by generated `DataSetRow` implementations.
#[doc(hidden)]
pub const fn datatype_is_supported(datatype: DataType, supported: &[DataType]) -> bool {
    let mut index = 0;
    while index < supported.len() {
        // Casting to the discriminant allows comparison without requiring `const PartialEq`.
        if supported[index] as i32 == datatype as i32 {
            return true;
        }
        index += 1;
    }
    false
}

/// Asserts at compile time that a type's [`HasDataType::DEFAULT_DATATYPE`] is included in its
/// [`HasDataType::SUPPORTED_DATATYPES`].
///
/// This is used by derive macros and the crate's own [`HasDataType`] implementations so an
/// invalid implementation fails to compile instead of producing the wrong column datatype.
#[doc(hidden)]
pub const fn assert_default_datatype_supported<T: HasDataType>() {
    assert!(
        datatype_is_supported(T::DEFAULT_DATATYPE, T::SUPPORTED_DATATYPES),
        "HasDataType::DEFAULT_DATATYPE must be included in HasDataType::SUPPORTED_DATATYPES",
    );
}

impl<T> HasDataType for Option<T>
where
    T: HasDataType,
{
    const DEFAULT_DATATYPE: DataType = T::DEFAULT_DATATYPE;
    const SUPPORTED_DATATYPES: &'static [DataType] = T::SUPPORTED_DATATYPES;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_forwards_datatypes() {
        assert_eq!(
            Option::<String>::SUPPORTED_DATATYPES,
            String::SUPPORTED_DATATYPES
        );
        assert_eq!(Option::<String>::DEFAULT_DATATYPE, String::DEFAULT_DATATYPE);
        assert!(datatype_is_supported(
            DataType::Text,
            String::SUPPORTED_DATATYPES
        ));
        assert!(datatype_is_supported(
            DataType::Uuid,
            String::SUPPORTED_DATATYPES
        ));
        assert!(!datatype_is_supported(
            DataType::Int32,
            String::SUPPORTED_DATATYPES
        ));
    }
}
