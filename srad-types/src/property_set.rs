use std::collections::HashMap;

use crate::{constants::QUALITY, payload::{self, property_value}, quality::Quality, traits, PropertyValue as PropertyValueValue};

#[derive(Debug)]
struct PropertyValue {
  value: Option<PropertyValueValue>,
  datatype: Option<payload::DataType>
}

impl From<PropertyValue> for payload::PropertyValue {
  fn from(value: PropertyValue) -> Self {
    let mut is_null = None;
    let mut pvv= None;
    match value.value {
      Some(v) => pvv = Some(v.into()),
      None => is_null = Some(true),
    }
    payload::PropertyValue { r#type: value.datatype.map(|x| x as u32), is_null, value: pvv }
  }
}

impl TryFrom<payload::PropertyValue> for PropertyValue {
  type Error = ();

  fn try_from(payload: payload::PropertyValue) -> Result<Self, Self::Error> {

    let value = if let Some(value) = payload.value {
      Some(PropertyValueValue::new(value)) 
    } else if let Some(is_null) = payload.is_null {
      if is_null { None } else { return Err(()) }
    } else {
      return Err(())
    };

    let ty = match payload.r#type {
      Some(v) => Some(v.try_into()?),
      None => None, 
    };
    Ok(Self { value, datatype: ty})
  }
}

/// A collection of property values
#[derive(Debug)]
pub struct PropertySet(HashMap<String, PropertyValue>);

impl PropertySet{

  pub fn new_with_quality(quality: Quality) -> Self {
    let mut pset = PropertySet(HashMap::new());
    pset.0.insert(QUALITY.to_string(), PropertyValue {value: Some(quality.into()), datatype: Some(payload::DataType::Int32) });
    pset
  }

  pub fn new() -> Self {
    Self::new_with_quality(Quality::Good)
  }

  pub fn insert<K: Into<String>, V: traits::PropertyValue>(&mut self, k: K, v: Option<V>) -> Result<(), ()>{
    let key = k.into();
    if key == QUALITY { return Err(()) }
    let pv = PropertyValue { value: v.map(V::into), datatype: Some(V::default_datatype()) };
    self.0.insert(key, pv );
    Ok(())
  }

}

impl traits::HasDataType for PropertySet {
  fn supported_datatypes() -> &'static [payload::DataType] {
    static SUPPORTED_TYPES: [payload::DataType; 1] = [payload::DataType::PropertySet];
    &SUPPORTED_TYPES
  }
}

impl TryFrom<PropertyValueValue> for PropertySet {
  type Error = ();
  
  fn try_from(value: PropertyValueValue) -> Result<Self, Self::Error> {
    let value: payload::property_value::Value = value.into();
    if let payload::property_value::Value::PropertysetValue(v) = value {
      Ok(v.try_into()?)
    } else { 
      Err(()) 
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
    payload::PropertySet { keys: keys, values: values }
  }
}

impl TryFrom<payload::PropertySet> for PropertySet {
  type Error = ();

  fn try_from(value: payload::PropertySet) -> Result<Self, Self::Error> {
    let keys = value.keys;
    let values= value.values;
    let len = keys.len();
    if len != values.len() { return Err(()) }

    let mut hashmap: HashMap<String, PropertyValue> = HashMap::with_capacity(len);
    for (k, v) in keys.into_iter().zip(values) {
      let value= v.try_into()?;
      hashmap.insert(k, value);
    }

    Ok(PropertySet (hashmap))
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
    type Error = ();

    fn try_from(value: payload::PropertySetList) -> Result<Self, Self::Error> {
      let mut out = Vec::with_capacity(value.propertyset.len());
      for x in value.propertyset.into_iter() {
        out.push(x.try_into()?)
      }
      Ok(out)
    }
}

impl traits::HasDataType for PropertySetList {
  fn supported_datatypes() -> &'static [payload::DataType] {
    static SUPPORTED_TYPES: [payload::DataType; 1] = [payload::DataType::PropertySetList];
    &SUPPORTED_TYPES
  }
}

impl TryFrom<PropertyValueValue> for PropertySetList {
  type Error = ();
  
  fn try_from(value: PropertyValueValue) -> Result<Self, Self::Error> {
    let value: payload::property_value::Value = value.into();
    if let payload::property_value::Value::PropertysetsValue(v) = value {
      Ok(v.try_into()?)
    } else { 
      Err(()) 
    }
  }
}

impl From<PropertySetList> for PropertyValueValue {
  fn from(value: PropertySetList) -> Self {
    PropertyValueValue::new(property_value::Value::PropertysetsValue(value.into()))
  }
}

impl traits::PropertyValue for PropertySetList {}
