use serde::{Deserialize, Serialize};

pub use crate::generated::sparkplug_payload::{payload::*, *};

pub use prost::Message;

impl Metric {
    pub fn new() -> Self {
        Self {
            name: None,
            alias: None,
            timestamp: None,
            datatype: None,
            is_historical: None,
            is_transient: None,
            is_null: Some(true),
            metadata: None,
            properties: None,
            value: None,
        }
    }

    pub fn set_name(&mut self, name: String) -> &mut Self {
        self.name = Some(name);
        self
    }

    pub fn set_alias(&mut self, alias: u64) -> &mut Self {
        self.alias = Some(alias);
        self
    }

    pub fn set_datatype(&mut self, datatype: DataType) -> &mut Self {
        self.datatype = Some(datatype as u32);
        self
    }

    pub fn set_timestamp(&mut self, timestamp: u64) -> &mut Self {
        self.timestamp = Some(timestamp);
        self
    }

    pub fn set_value(&mut self, value: metric::Value) -> &mut Self {
        self.value = Some(value);
        self.is_null = None;
        self
    }

    pub fn set_null(&mut self) -> &mut Self {
        self.value = None;
        self.is_null = Some(true);
        self
    }
}

pub trait ToMetric {
    fn to_metric(self) -> Metric;
}

impl From<Payload> for Vec<u8> {
    fn from(value: Payload) -> Self {
        let mut bytes = Vec::new();
        value.encode(&mut bytes).unwrap();
        bytes
    }
}

impl TryFrom<u32> for DataType {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            x if x == DataType::Unknown as u32 => Ok(DataType::Unknown),
            x if x == DataType::Int8 as u32 => Ok(DataType::Int8),
            x if x == DataType::Int16 as u32 => Ok(DataType::Int16),
            x if x == DataType::Int32 as u32 => Ok(DataType::Int32),
            x if x == DataType::Int64 as u32 => Ok(DataType::Int64),
            x if x == DataType::UInt8 as u32 => Ok(DataType::UInt8),
            x if x == DataType::UInt16 as u32 => Ok(DataType::UInt16),
            x if x == DataType::UInt32 as u32 => Ok(DataType::UInt32),
            x if x == DataType::UInt64 as u32 => Ok(DataType::UInt64),
            x if x == DataType::Float as u32 => Ok(DataType::Float),
            x if x == DataType::Double as u32 => Ok(DataType::Double),
            x if x == DataType::Boolean as u32 => Ok(DataType::Boolean),
            x if x == DataType::String as u32 => Ok(DataType::String),
            x if x == DataType::DateTime as u32 => Ok(DataType::DateTime),
            x if x == DataType::Text as u32 => Ok(DataType::Text),
            x if x == DataType::Uuid as u32 => Ok(DataType::Uuid),
            x if x == DataType::DataSet as u32 => Ok(DataType::DataSet),
            x if x == DataType::Bytes as u32 => Ok(DataType::Bytes),
            x if x == DataType::File as u32 => Ok(DataType::File),
            x if x == DataType::Template as u32 => Ok(DataType::Template),
            x if x == DataType::PropertySet as u32 => Ok(DataType::PropertySet),
            x if x == DataType::PropertySetList as u32 => Ok(DataType::PropertySetList),
            x if x == DataType::Int8Array as u32 => Ok(DataType::Int8Array),
            x if x == DataType::Int16Array as u32 => Ok(DataType::Int16Array),
            x if x == DataType::Int32Array as u32 => Ok(DataType::Int32Array),
            x if x == DataType::Int64Array as u32 => Ok(DataType::Int64Array),
            x if x == DataType::UInt8Array as u32 => Ok(DataType::UInt8Array),
            x if x == DataType::UInt16Array as u32 => Ok(DataType::UInt16Array),
            x if x == DataType::UInt32Array as u32 => Ok(DataType::UInt32Array),
            x if x == DataType::UInt64Array as u32 => Ok(DataType::UInt64Array),
            x if x == DataType::FloatArray as u32 => Ok(DataType::FloatArray),
            x if x == DataType::DoubleArray as u32 => Ok(DataType::DoubleArray),
            x if x == DataType::BooleanArray as u32 => Ok(DataType::BooleanArray),
            x if x == DataType::StringArray as u32 => Ok(DataType::StringArray),
            x if x == DataType::DateTimeArray as u32 => Ok(DataType::DateTimeArray),
            _ => Err(()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct StateBirthDeathCertificate {
    pub timestamp: u64,
    pub online: bool,
}

impl TryFrom<StateBirthDeathCertificate> for Vec<u8> {
    type Error = String;
    fn try_from(value: StateBirthDeathCertificate) -> Result<Self, Self::Error> {
        match serde_json::to_vec(&value) {
            Ok(v) => Ok(v),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl TryFrom<&[u8]> for StateBirthDeathCertificate {
    type Error = String;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match serde_json::from_slice::<StateBirthDeathCertificate>(value) {
            Ok(v) => Ok(v),
            Err(e) => Err(e.to_string()),
        }
    }
}
