use crate::{payload::DataType, traits, FromValueTypeError, PropertyValue};

/// Represents the different quality property values of a metric
pub enum Quality {
    Good = 0,
    Bad = 192,
    Stale = 500,
}

impl TryFrom<i32> for Quality {
    type Error = ();

    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            x if x == Quality::Good as i32 => Ok(Quality::Good),
            x if x == Quality::Bad as i32 => Ok(Quality::Bad),
            x if x == Quality::Stale as i32 => Ok(Quality::Stale),
            _ => Err(()),
        }
    }
}

impl traits::HasDataType for Quality {
    const DEFAULT_DATATYPE: DataType = DataType::Int32;
    const SUPPORTED_DATATYPES: &'static [DataType] = &[DataType::Int32];
}

impl TryFrom<PropertyValue> for Quality {
    type Error = FromValueTypeError;

    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        let value: i32 = value.try_into()?;
        match value.try_into() {
            Ok(value) => Ok(value),
            Err(_) => Err(FromValueTypeError::InvalidValue(
                "int value was not a valid quality enum value".to_string(),
            )),
        }
    }
}

impl From<Quality> for PropertyValue {
    fn from(value: Quality) -> Self {
        (value as i32).into()
    }
}

impl traits::PropertyValue for Quality {}
