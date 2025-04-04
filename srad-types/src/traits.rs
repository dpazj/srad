use crate::{metadata::MetaData, payload::DataType, value};

pub trait HasDataType {
  fn supported_datatypes() -> &'static [DataType];
  fn default_datatype() -> DataType { 
    let supported = Self::supported_datatypes();
    if supported.len() == 0 { panic!("supported_datatypes result has to contain at least one element") }
    supported[0]
  }
}

pub trait MetadataDescription {
  fn description(&self) -> String;
}

pub trait BytesTypeValue {
  fn size() -> usize;
}

pub trait FileTypeValue : BytesTypeValue {
  fn file_type() -> String;
  fn file_name() -> String;
}

pub trait MetricValue : TryFrom<value::MetricValue> + Into<value::MetricValue> + HasDataType { 
  fn birth_metadata(&self) -> Option<MetaData> { self.publish_metadata() }
  fn publish_metadata(&self) -> Option<MetaData> { None }
}

pub trait PropertyValue: TryFrom<value::PropertyValue> + Into<value::PropertyValue> + HasDataType { }
pub trait DataSetValue: TryFrom<value::DataSetValue> + Into<value::DataSetValue> + HasDataType { }
pub trait ParameterValue: TryFrom<value::ParameterValue> + Into<value::ParameterValue> + HasDataType { }
