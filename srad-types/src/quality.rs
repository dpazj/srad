use crate::{payload::DataType, traits, FromValueTypeError, PropertyValue};

/// Represents the different quality property values of a metric
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Good = 0,
    Bad = 192,
    Stale = 500,
}

impl TryFrom<i32> for Quality {
    type Error = QualityError;

    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            x if x == Quality::Good as i32 => Ok(Quality::Good),
            x if x == Quality::Bad as i32 => Ok(Quality::Bad),
            x if x == Quality::Stale as i32 => Ok(Quality::Stale),
            _ => Err(QualityError::UnknownValue(v)),
        }
    }
}

/// Errors that can occur while converting a raw quality value.
#[derive(Debug, thiserror::Error)]
pub enum QualityError {
    #[error("unknown quality value: {0}")]
    UnknownValue(i32),
}

impl traits::HasDataType for Quality {
    const DEFAULT_DATATYPE: DataType = DataType::Int32;
    const SUPPORTED_DATATYPES: &'static [DataType] = &[DataType::Int32];
}

const _: () = traits::assert_default_datatype_supported::<Quality>();

impl TryFrom<PropertyValue> for Quality {
    type Error = FromValueTypeError;

    fn try_from(value: PropertyValue) -> Result<Self, Self::Error> {
        let value: i32 = value.try_into()?;
        value.try_into().map_err(|e: QualityError| {
            FromValueTypeError::InvalidValue(format!(
                "int value was not a valid quality enum value: {e}"
            ))
        })
    }
}

impl From<Quality> for PropertyValue {
    fn from(value: Quality) -> Self {
        (value as i32).into()
    }
}

impl traits::PropertyValue for Quality {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_quality_reports_the_value() {
        assert!(matches!(
            Quality::try_from(42),
            Err(QualityError::UnknownValue(42))
        ));
    }
}
