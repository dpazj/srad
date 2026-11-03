use std::collections::HashMap;

use crate::{
    constants::QUALITY,
    payload::{self, property_value, DataTypeError},
    quality::Quality,
    traits, PropertyValue as PropertyValueValue,
};

#[derive(Debug, Clone)]
struct PropertyValue {
    value: Option<PropertyValueValue>,
    datatype: Option<payload::DataType>,
}

/// Errors that can occur while converting a raw Sparkplug property value.
#[derive(Debug, thiserror::Error)]
pub enum PropertyValueError {
    #[error("property value has `is_null = false` but no value")]
    InvalidNullValue,
    #[error("property value is missing both `value` and `is_null`")]
    MissingValue,
    #[error("invalid property datatype: {0}")]
    InvalidDataType(#[from] DataTypeError),
}

/// Errors that can occur while working with a [PropertySet] or a property set list.
#[derive(Debug, thiserror::Error)]
pub enum PropertySetError {
    #[error("the `{QUALITY}` property is reserved")]
    ReservedQuality,
    #[error("property set key/value count mismatch: {keys} keys, {values} values")]
    KeyValueCountMismatch { keys: usize, values: usize },
    #[error(transparent)]
    InvalidPropertyValue(#[from] PropertyValueError),
    #[error("metric value is not a PropertySet")]
    NotAPropertySet,
    #[error("metric value is not a PropertySetList")]
    NotAPropertySetList,
}

impl From<PropertyValue> for payload::PropertyValue {
    fn from(value: PropertyValue) -> Self {
        let mut is_null = None;
        let mut pvv = None;
        match value.value {
            Some(v) => pvv = Some(v.into()),
            None => is_null = Some(true),
        }
        payload::PropertyValue {
            r#type: value.datatype.map(|x| x as u32),
            is_null,
            value: pvv,
        }
    }
}

impl TryFrom<payload::PropertyValue> for PropertyValue {
    type Error = PropertyValueError;

    fn try_from(payload: payload::PropertyValue) -> Result<Self, Self::Error> {
        let value = if let Some(value) = payload.value {
            Some(PropertyValueValue::new(value))
        } else if let Some(is_null) = payload.is_null {
            if is_null {
                None
            } else {
                return Err(PropertyValueError::InvalidNullValue);
            }
        } else {
            return Err(PropertyValueError::MissingValue);
        };

        let ty = match payload.r#type {
            Some(v) => Some(v.try_into()?),
            None => None,
        };
        Ok(Self {
            value,
            datatype: ty,
        })
    }
}

/// A collection of property values
#[derive(Debug, Clone)]
pub struct PropertySet(HashMap<String, PropertyValue>);

impl Default for PropertySet {
    fn default() -> Self {
        Self::new()
    }
}

impl PropertySet {
    pub fn new_with_quality(quality: Quality) -> Self {
        let mut pset = PropertySet(HashMap::new());
        pset.0.insert(
            QUALITY.to_string(),
            PropertyValue {
                value: Some(quality.into()),
                datatype: Some(payload::DataType::Int32),
            },
        );
        pset
    }

    pub fn new() -> Self {
        Self::new_with_quality(Quality::Good)
    }

    pub fn insert<K: Into<String>, V: traits::PropertyValue>(
        &mut self,
        k: K,
        v: Option<V>,
    ) -> Result<(), PropertySetError> {
        let key = k.into();
        if key == QUALITY {
            return Err(PropertySetError::ReservedQuality);
        }
        let pv = PropertyValue {
            value: v.map(V::into),
            datatype: Some(V::DEFAULT_DATATYPE),
        };
        self.0.insert(key, pv);
        Ok(())
    }
}

impl traits::HasDataType for PropertySet {
    const DEFAULT_DATATYPE: payload::DataType = payload::DataType::PropertySet;
    const SUPPORTED_DATATYPES: &'static [payload::DataType] = &[payload::DataType::PropertySet];
}

const _: () = traits::assert_default_datatype_supported::<PropertySet>();

impl TryFrom<PropertyValueValue> for PropertySet {
    type Error = PropertySetError;

    fn try_from(value: PropertyValueValue) -> Result<Self, Self::Error> {
        let value: payload::property_value::Value = value.into();
        if let payload::property_value::Value::PropertysetValue(v) = value {
            Ok(v.try_into()?)
        } else {
            Err(PropertySetError::NotAPropertySet)
        }
    }
}

impl From<PropertySet> for PropertyValueValue {
    fn from(value: PropertySet) -> Self {
        PropertyValueValue::new(property_value::Value::PropertysetValue(value.into()))
    }
}

impl traits::PropertyValue for PropertySet {}

impl From<PropertySet> for payload::PropertySet {
    fn from(value: PropertySet) -> Self {
        let len = value.0.len();
        let mut keys = Vec::with_capacity(len);
        let mut values: Vec<payload::PropertyValue> = Vec::with_capacity(len);
        value.0.into_iter().for_each(|(k, v)| {
            keys.push(k);
            values.push(v.into());
        });
        payload::PropertySet { keys, values }
    }
}

impl TryFrom<payload::PropertySet> for PropertySet {
    type Error = PropertySetError;

    fn try_from(value: payload::PropertySet) -> Result<Self, Self::Error> {
        let keys = value.keys;
        let values = value.values;
        let len = keys.len();
        if len != values.len() {
            return Err(PropertySetError::KeyValueCountMismatch {
                keys: len,
                values: values.len(),
            });
        }

        let mut hashmap: HashMap<String, PropertyValue> = HashMap::with_capacity(len);
        for (k, v) in keys.into_iter().zip(values) {
            let value = v.try_into()?;
            hashmap.insert(k, value);
        }

        Ok(PropertySet(hashmap))
    }
}

type PropertySetList = Vec<PropertySet>;

impl From<PropertySetList> for payload::PropertySetList {
    fn from(value: PropertySetList) -> Self {
        let mut out = Vec::with_capacity(value.len());
        for x in value.into_iter() {
            out.push(x.into())
        }
        payload::PropertySetList { propertyset: out }
    }
}

impl TryFrom<payload::PropertySetList> for PropertySetList {
    type Error = PropertySetError;

    fn try_from(value: payload::PropertySetList) -> Result<Self, Self::Error> {
        let mut out = Vec::with_capacity(value.propertyset.len());
        for x in value.propertyset.into_iter() {
            out.push(x.try_into()?)
        }
        Ok(out)
    }
}

impl traits::HasDataType for PropertySetList {
    const DEFAULT_DATATYPE: payload::DataType = payload::DataType::PropertySetList;
    const SUPPORTED_DATATYPES: &'static [payload::DataType] = &[payload::DataType::PropertySetList];
}

const _: () = traits::assert_default_datatype_supported::<PropertySetList>();

impl TryFrom<PropertyValueValue> for PropertySetList {
    type Error = PropertySetError;

    fn try_from(value: PropertyValueValue) -> Result<Self, Self::Error> {
        let value: payload::property_value::Value = value.into();
        if let payload::property_value::Value::PropertysetsValue(v) = value {
            Ok(v.try_into()?)
        } else {
            Err(PropertySetError::NotAPropertySetList)
        }
    }
}

impl From<PropertySetList> for PropertyValueValue {
    fn from(value: PropertySetList) -> Self {
        PropertyValueValue::new(property_value::Value::PropertysetsValue(value.into()))
    }
}

impl traits::PropertyValue for PropertySetList {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_quality_property_is_rejected() {
        let mut set = PropertySet::new();
        assert!(matches!(
            set.insert(QUALITY, Some(1u32)),
            Err(PropertySetError::ReservedQuality)
        ));
    }

    #[test]
    fn key_value_count_mismatch_is_reported() {
        let payload = payload::PropertySet {
            keys: vec!["key".into()],
            values: Vec::new(),
        };
        assert!(matches!(
            PropertySet::try_from(payload),
            Err(PropertySetError::KeyValueCountMismatch { keys: 1, values: 0 })
        ));
    }
}
