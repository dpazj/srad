use std::string::FromUtf8Error;

use crate::payload::{
    data_set::data_set_value, metric, property_value, template::parameter, DataType,
};
use crate::traits;

use paste::paste;
use thiserror::Error;

macro_rules! impl_wrapper_type_for_proto_value_type {
    ($new_type:ident, $wrapped_type:ty) => {
        #[derive(Debug, Clone)]
        pub struct $new_type(pub $wrapped_type);

        impl $new_type {
            pub fn new(inner: $wrapped_type) -> Self {
                Self(inner)
            }
        }

        impl From<$new_type> for $wrapped_type {
            fn from(value: $new_type) -> Self {
                value.0
            }
        }

        impl From<$wrapped_type> for $new_type {
            fn from(value: $wrapped_type) -> Self {
                $new_type(value)
            }
        }
    };
}

impl_wrapper_type_for_proto_value_type!(MetricValue, metric::Value);
impl_wrapper_type_for_proto_value_type!(PropertyValue, property_value::Value);
impl_wrapper_type_for_proto_value_type!(DataSetValue, data_set_value::Value);
impl_wrapper_type_for_proto_value_type!(ParameterValue, parameter::Value);

#[derive(Debug, PartialEq, Clone)]
pub struct DateTime {
    /* milliseconds since epoch (Jan 1, 1970) */
    pub date_time: u64,
}

impl DateTime {
    pub fn new(date_time: u64) -> Self {
        Self { date_time }
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_le_bytes(self) -> [u8; 8] {
        self.date_time.to_le_bytes()
    }

    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self::new(u64::from_le_bytes(bytes))
    }
}

fn bool_to_proto(val: bool) -> bool {
    val
}
fn u8_to_proto(val: u8) -> u32 {
    val as u32
}
fn u16_to_proto(val: u16) -> u32 {
    let b = val.to_le_bytes();
    u32::from_le_bytes([b[0], b[1], 0, 0])
}
fn u32_to_proto(val: u32) -> u32 {
    val
}
fn u64_to_proto(val: u64) -> u64 {
    val
}
fn i8_to_proto(val: i8) -> u32 {
    let b = val.to_le_bytes();
    u32::from_le_bytes([b[0], 0, 0, 0])
}
fn i16_to_proto(val: i16) -> u32 {
    let b = val.to_le_bytes();
    u32::from_le_bytes([b[0], b[1], 0, 0])
}
fn i32_to_proto(val: i32) -> u32 {
    u32::from_le_bytes(val.to_le_bytes())
}
fn i64_to_proto(val: i64) -> u64 {
    u64::from_le_bytes(val.to_le_bytes())
}
fn f32_to_proto(val: f32) -> f32 {
    val
}
fn f64_to_proto(val: f64) -> f64 {
    val
}
fn string_to_proto(val: String) -> String {
    val
}
fn datetime_to_proto(val: DateTime) -> u64 {
    val.date_time
}

fn proto_to_bool(val: bool) -> bool {
    val
}
fn proto_to_u8(val: u32) -> u8 {
    val as u8
}
fn proto_to_u16(val: u32) -> u16 {
    val as u16
}
fn proto_to_u32(val: u32) -> u32 {
    val
}
fn proto_to_u64(val: u64) -> u64 {
    val
}
fn proto_to_i8(val: u32) -> i8 {
    let bytes = val.to_le_bytes();
    i8::from_le_bytes([bytes[0]])
}
fn proto_to_i16(val: u32) -> i16 {
    let bytes = val.to_le_bytes();
    i16::from_le_bytes([bytes[0], bytes[1]])
}
fn proto_to_i32(val: u32) -> i32 {
    i32::from_le_bytes(val.to_le_bytes())
}
fn proto_to_i64(val: u64) -> i64 {
    i64::from_le_bytes(val.to_le_bytes())
}
fn proto_to_f32(val: f32) -> f32 {
    val
}
fn proto_to_f64(val: f64) -> f64 {
    val
}
fn proto_to_string(val: String) -> String {
    val
}
fn proto_to_datetime(val: u64) -> DateTime {
    DateTime { date_time: val }
}

/* Array type conversions */

#[derive(Debug, Error)]
pub enum FromBytesError {
    #[error("Invalid format")]
    InvalidFormat,
    #[error("Invalid bytes size")]
    InvalidSize,
    #[error("StringArray string decoding error {0}")]
    BadStringElement(#[from] FromUtf8Error),
}

fn u8_vec_to_proto(vec: Vec<u8>) -> Vec<u8> {
    vec
}
fn proto_to_u8_vec(vec: Vec<u8>) -> Result<Vec<u8>, FromBytesError> {
    Ok(vec)
}

macro_rules! define_array_proto_conversions {
  ($ty:ty) => {
    paste! {
      fn [<$ty:lower _vec_to_proto>](vec: Vec<$ty>) -> Vec<u8> {
        let mut out = Vec::with_capacity(vec.len() * size_of::<$ty>());
        vec.into_iter().for_each(|x| out.extend(x.to_le_bytes()));
        out
      }

      fn [<proto_to_$ty:lower _vec>](vec: Vec<u8>) -> Result<Vec<$ty>,FromBytesError> {
        let div = std::mem::size_of::<$ty>();
        let len = vec.len();
        if len % div != 0 { return Err(FromBytesError::InvalidFormat) }
        let mut out = Vec::with_capacity(len/div);
        vec.chunks_exact(div).for_each(|x| { out.push(<$ty>::from_le_bytes(x.try_into().unwrap()));});
        Ok(out)
      }
    }
  };
}

define_array_proto_conversions!(i8);
define_array_proto_conversions!(i16);
define_array_proto_conversions!(i32);
define_array_proto_conversions!(i64);
define_array_proto_conversions!(u16);
define_array_proto_conversions!(u32);
define_array_proto_conversions!(u64);
define_array_proto_conversions!(f32);
define_array_proto_conversions!(f64);
define_array_proto_conversions!(DateTime);

fn pack_byte_with_bool(bools: &[bool]) -> u8 {
    bools
        .iter()
        .enumerate()
        .fold(0u8, |acc, (i, b)| acc | ((*b as u8) << (7 - i)))
}

fn bool_vec_to_proto(vec: Vec<bool>) -> Vec<u8> {
    /* BooleanArray as an array of bit-packed bytes preceded by a 4-byte integer that represents the total number of boolean values */
    let count = vec.len() as u32;
    let bool_bytes_len = count.div_ceil(8) as usize;
    let mut out = Vec::<u8>::with_capacity(std::mem::size_of::<u32>() + bool_bytes_len);
    /* Set first bytes as count */
    for x in count.to_le_bytes() {
        out.push(x);
    }
    /* Pack bools into bytes */
    let chunks = vec.chunks_exact(8);
    let remainder = chunks.remainder();
    chunks
        .into_iter()
        .for_each(|chunk| out.push(pack_byte_with_bool(chunk)));
    if !remainder.is_empty() {
        out.push(pack_byte_with_bool(remainder))
    }
    out
}

fn proto_to_bool_vec(bytes: Vec<u8>) -> Result<Vec<bool>, FromBytesError> {
    let len = bytes.len();
    if len < 4 {
        return Err(FromBytesError::InvalidSize);
    }
    let bool_bytes = len - 4;
    let bool_count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if bool_count == 0 {
        return Ok(Vec::new());
    }

    let needed_bytes = bool_count.div_ceil(8) as usize;
    if len < 4 + needed_bytes {
        return Err(FromBytesError::InvalidFormat);
    }

    let bools_data = &bytes.as_slice()[4..];
    let mut bools_out = Vec::with_capacity(bool_count as usize);

    for b in bools_data.iter().take(bool_bytes - 1) {
        for j in (0..8).rev() {
            bools_out.push(((b >> j) & 1) == 1);
        }
    }

    for i in 0..(bool_count % 8) {
        bools_out.push(((bools_data[bool_bytes - 1] >> (7 - i)) & 1) == 1);
    }
    Ok(bools_out)
}

fn string_vec_to_proto(vec: Vec<String>) -> Vec<u8> {
    /* StringArray as an array of null terminated strings */
    let buffer_len = vec.iter().fold(0usize, |len, string| len + string.len()) + vec.len();
    let mut out = Vec::with_capacity(buffer_len);
    vec.into_iter().for_each(|string| {
        out.extend(string.into_bytes());
        out.push(0x0);
    });
    out
}

fn proto_to_string_vec(vec: Vec<u8>) -> Result<Vec<String>, FromBytesError> {
    if let Some(last) = vec.last() {
        if *last != 0 {
            return Err(FromBytesError::InvalidFormat);
        }
    } else {
        return Ok(Vec::new());
    }

    let mut res = Vec::new();

    let mut split = vec.split(|x| *x == 0).peekable();
    while let Some(string_data) = split.next() {
        if split.peek().is_none() {
            break;
        }
        res.push(String::from_utf8(string_data.into())?)
    }
    Ok(res)
}

#[derive(Debug, Error)]
pub enum FromValueTypeError {
    #[error("Bytes decoding error: {0}")]
    ArrayDecodeError(#[from] FromBytesError),
    #[error("Value variant type was invalid")]
    InvalidVariantType,
    #[error("Value contained an invalid value: {0}")]
    InvalidValue(String),
}

/* Trait implementations */

macro_rules! impl_to_from_proto_value_type_for_type {
    (
    $type:ty,
    $wrapping_type:ty,
    $proto_variant:path,
    $to_proto_fn:ident,
    $from_proto_fn:ident
  ) => {
        impl From<$type> for $wrapping_type {
            fn from(value: $type) -> Self {
                $proto_variant($to_proto_fn(value)).into()
            }
        }

        impl TryFrom<$wrapping_type> for $type {
            type Error = FromValueTypeError;
            fn try_from(value: $wrapping_type) -> Result<Self, Self::Error> {
                if let $proto_variant(v) = value.0 {
                    Ok($from_proto_fn(v))
                } else {
                    Err(FromValueTypeError::InvalidVariantType)
                }
            }
        }
    };
}

macro_rules! array_length {
  ([$($element:expr),* $(,)?]) => {
    {
      0 $(+ {let _ = $element; 1})*
    }
  };
}

macro_rules! impl_has_datatype {
  ($type:ty, [$($datatypes:expr),* $(,)?]) => {
    impl traits::HasDataType for $type {
      fn supported_datatypes() -> &'static[DataType] {
        static SUPPORTED_TYPES: [DataType; array_length!([$($datatypes),*])] = [$($datatypes),*];
        &SUPPORTED_TYPES
      }
    }
  };
}

macro_rules! impl_basic_type {
  ($type:ty, [$($datatypes:expr),* $(,)?], $variant_metric:path, $variant_property:path, $variant_dataset:path, $variant_parameter:path, $to_proto_fn:ident, $from_proto_fn:ident) => {

    impl_has_datatype!($type, [$($datatypes),*]);

    impl traits::MetricValue for $type {}
    impl traits::PropertyValue for $type {}
    impl traits::DataSetValue for $type {}
    impl traits::ParameterValue for $type {}

    impl_to_from_proto_value_type_for_type!($type, MetricValue, $variant_metric, $to_proto_fn, $from_proto_fn) ;
    impl_to_from_proto_value_type_for_type!($type, PropertyValue, $variant_property, $to_proto_fn, $from_proto_fn);
    impl_to_from_proto_value_type_for_type!($type, DataSetValue, $variant_dataset, $to_proto_fn, $from_proto_fn);
    impl_to_from_proto_value_type_for_type!($type, ParameterValue, $variant_parameter, $to_proto_fn, $from_proto_fn);
  };
}

impl_basic_type!(
    bool,
    [DataType::Boolean],
    metric::Value::BooleanValue,
    property_value::Value::BooleanValue,
    data_set_value::Value::BooleanValue,
    parameter::Value::BooleanValue,
    bool_to_proto,
    proto_to_bool
);
impl_basic_type!(
    u8,
    [DataType::UInt8],
    metric::Value::IntValue,
    property_value::Value::IntValue,
    data_set_value::Value::IntValue,
    parameter::Value::IntValue,
    u8_to_proto,
    proto_to_u8
);
impl_basic_type!(
    u16,
    [DataType::UInt16],
    metric::Value::IntValue,
    property_value::Value::IntValue,
    data_set_value::Value::IntValue,
    parameter::Value::IntValue,
    u16_to_proto,
    proto_to_u16
);
impl_basic_type!(
    u32,
    [DataType::UInt32],
    metric::Value::IntValue,
    property_value::Value::IntValue,
    data_set_value::Value::IntValue,
    parameter::Value::IntValue,
    u32_to_proto,
    proto_to_u32
);
impl_basic_type!(
    u64,
    [DataType::UInt64],
    metric::Value::LongValue,
    property_value::Value::LongValue,
    data_set_value::Value::LongValue,
    parameter::Value::LongValue,
    u64_to_proto,
    proto_to_u64
);
impl_basic_type!(
    i8,
    [DataType::Int8],
    metric::Value::IntValue,
    property_value::Value::IntValue,
    data_set_value::Value::IntValue,
    parameter::Value::IntValue,
    i8_to_proto,
    proto_to_i8
);
impl_basic_type!(
    i16,
    [DataType::Int16],
    metric::Value::IntValue,
    property_value::Value::IntValue,
    data_set_value::Value::IntValue,
    parameter::Value::IntValue,
    i16_to_proto,
    proto_to_i16
);
impl_basic_type!(
    i32,
    [DataType::Int32],
    metric::Value::IntValue,
    property_value::Value::IntValue,
    data_set_value::Value::IntValue,
    parameter::Value::IntValue,
    i32_to_proto,
    proto_to_i32
);
impl_basic_type!(
    i64,
    [DataType::Int64],
    metric::Value::LongValue,
    property_value::Value::LongValue,
    data_set_value::Value::LongValue,
    parameter::Value::LongValue,
    i64_to_proto,
    proto_to_i64
);
impl_basic_type!(
    f32,
    [DataType::Float],
    metric::Value::FloatValue,
    property_value::Value::FloatValue,
    data_set_value::Value::FloatValue,
    parameter::Value::FloatValue,
    f32_to_proto,
    proto_to_f32
);
impl_basic_type!(
    f64,
    [DataType::Double],
    metric::Value::DoubleValue,
    property_value::Value::DoubleValue,
    data_set_value::Value::DoubleValue,
    parameter::Value::DoubleValue,
    f64_to_proto,
    proto_to_f64
);
impl_basic_type!(
    String,
    [DataType::String, DataType::Text],
    metric::Value::StringValue,
    property_value::Value::StringValue,
    data_set_value::Value::StringValue,
    parameter::Value::StringValue,
    string_to_proto,
    proto_to_string
);
impl_basic_type!(
    DateTime,
    [DataType::DateTime],
    metric::Value::LongValue,
    property_value::Value::LongValue,
    data_set_value::Value::LongValue,
    parameter::Value::LongValue,
    datetime_to_proto,
    proto_to_datetime
);

macro_rules! impl_vec_type_metric_value_conversions {
  ($type:ty, [$($datatypes:expr),* $(,)?], $to_proto_fn:ident, $from_proto_fn:ident) => {

    impl_has_datatype!(Vec::<$type>, [$($datatypes),*]);

    impl traits::MetricValue for Vec<$type> {}

    impl From<Vec<$type>> for MetricValue {
      fn from(value: Vec<$type>) -> Self {
        metric::Value::BytesValue($to_proto_fn(value)).into()
      }
    }

    impl TryFrom<MetricValue> for Vec<$type> {
      type Error = FromValueTypeError;
      fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        if let metric::Value::BytesValue(v) = value.0 {
          Ok( $from_proto_fn(v)?)
        }
        else {
          Err(FromValueTypeError::InvalidVariantType)
        }
      }
    }

  };
}

impl_vec_type_metric_value_conversions!(
    bool,
    [DataType::BooleanArray],
    bool_vec_to_proto,
    proto_to_bool_vec
);
impl_vec_type_metric_value_conversions!(
    u8,
    [DataType::UInt8Array, DataType::Bytes],
    u8_vec_to_proto,
    proto_to_u8_vec
);
impl_vec_type_metric_value_conversions!(
    u16,
    [DataType::UInt16Array],
    u16_vec_to_proto,
    proto_to_u16_vec
);
impl_vec_type_metric_value_conversions!(
    u32,
    [DataType::UInt32Array],
    u32_vec_to_proto,
    proto_to_u32_vec
);
impl_vec_type_metric_value_conversions!(
    u64,
    [DataType::UInt64Array],
    u64_vec_to_proto,
    proto_to_u64_vec
);
impl_vec_type_metric_value_conversions!(
    i8,
    [DataType::Int8Array],
    i8_vec_to_proto,
    proto_to_i8_vec
);
impl_vec_type_metric_value_conversions!(
    i16,
    [DataType::Int16Array],
    i16_vec_to_proto,
    proto_to_i16_vec
);
impl_vec_type_metric_value_conversions!(
    i32,
    [DataType::Int32Array],
    i32_vec_to_proto,
    proto_to_i32_vec
);
impl_vec_type_metric_value_conversions!(
    i64,
    [DataType::Int64Array],
    i64_vec_to_proto,
    proto_to_i64_vec
);
impl_vec_type_metric_value_conversions!(
    f32,
    [DataType::FloatArray],
    f32_vec_to_proto,
    proto_to_f32_vec
);
impl_vec_type_metric_value_conversions!(
    f64,
    [DataType::DoubleArray],
    f64_vec_to_proto,
    proto_to_f64_vec
);
impl_vec_type_metric_value_conversions!(
    DateTime,
    [DataType::DateTimeArray],
    datetime_vec_to_proto,
    proto_to_datetime_vec
);
impl_vec_type_metric_value_conversions!(
    String,
    [DataType::StringArray],
    string_vec_to_proto,
    proto_to_string_vec
);

#[derive(Debug)]
pub enum MetricValueKind {
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Boolean(bool),
    String(String),
    DateTime(DateTime),
    Text(String),
    Uuid(String),
    //DataSet,
    Bytes(Vec<u8>),
    File(Vec<u8>),
    //Template
    Int8Array(Vec<i8>),
    Int16Array(Vec<i16>),
    Int32Array(Vec<i32>),
    Int64Array(Vec<i64>),
    UInt8Array(Vec<u8>),
    UInt16Array(Vec<u16>),
    UInt32Array(Vec<u32>),
    UInt64Array(Vec<u64>),
    FloatArray(Vec<f32>),
    DoubleArray(Vec<f64>),
    BooleanArray(Vec<bool>),
    StringArray(Vec<String>),
    DateTimeArray(Vec<DateTime>),
}

#[derive(Debug, Error)]
pub enum FromMetricValueError {
    #[error("Bytes decoding error: {0}")]
    ValueDecodeError(#[from] FromValueTypeError),
    #[error("Unsupported datatype")]
    UnsupportedDataType(DataType),
    #[error("Invalid datatype provided")]
    InvalidDataType,
}

impl MetricValueKind {
    pub fn try_from_metric_value(
        datatype: DataType,
        value: MetricValue,
    ) -> Result<Self, FromMetricValueError> {
        let out = match datatype {
            DataType::Unknown => return Err(FromMetricValueError::InvalidDataType),
            DataType::Int8 => MetricValueKind::Int8(i8::try_from(value)?),
            DataType::Int16 => MetricValueKind::Int16(i16::try_from(value)?),
            DataType::Int32 => MetricValueKind::Int32(i32::try_from(value)?),
            DataType::Int64 => MetricValueKind::Int64(i64::try_from(value)?),
            DataType::UInt8 => MetricValueKind::UInt8(u8::try_from(value)?),
            DataType::UInt16 => MetricValueKind::UInt16(u16::try_from(value)?),
            DataType::UInt32 => MetricValueKind::UInt32(u32::try_from(value)?),
            DataType::UInt64 => MetricValueKind::UInt64(u64::try_from(value)?),
            DataType::Float => MetricValueKind::Float(f32::try_from(value)?),
            DataType::Double => MetricValueKind::Double(f64::try_from(value)?),
            DataType::Boolean => MetricValueKind::Boolean(bool::try_from(value)?),
            DataType::String => MetricValueKind::String(String::try_from(value)?),
            DataType::DateTime => MetricValueKind::DateTime(DateTime::try_from(value)?),
            DataType::Text => MetricValueKind::String(String::try_from(value)?),
            DataType::Uuid => MetricValueKind::Uuid(String::try_from(value)?),
            DataType::DataSet => {
                return Err(FromMetricValueError::UnsupportedDataType(DataType::DataSet))
            }
            DataType::Bytes => MetricValueKind::Bytes(Vec::<u8>::try_from(value)?),
            DataType::File => MetricValueKind::File(Vec::<u8>::try_from(value)?),
            DataType::Template => {
                return Err(FromMetricValueError::UnsupportedDataType(DataType::DataSet))
            }
            DataType::PropertySet => {
                return Err(FromMetricValueError::UnsupportedDataType(
                    DataType::PropertySet,
                ))
            }
            DataType::PropertySetList => {
                return Err(FromMetricValueError::UnsupportedDataType(
                    DataType::PropertySetList,
                ))
            }
            DataType::Int8Array => MetricValueKind::Int8Array(Vec::<i8>::try_from(value)?),
            DataType::Int16Array => MetricValueKind::Int16Array(Vec::<i16>::try_from(value)?),
            DataType::Int32Array => MetricValueKind::Int32Array(Vec::<i32>::try_from(value)?),
            DataType::Int64Array => MetricValueKind::Int64Array(Vec::<i64>::try_from(value)?),
            DataType::UInt8Array => MetricValueKind::UInt8Array(Vec::<u8>::try_from(value)?),
            DataType::UInt16Array => MetricValueKind::UInt16Array(Vec::<u16>::try_from(value)?),
            DataType::UInt32Array => MetricValueKind::UInt32Array(Vec::<u32>::try_from(value)?),
            DataType::UInt64Array => MetricValueKind::UInt64Array(Vec::<u64>::try_from(value)?),
            DataType::FloatArray => MetricValueKind::FloatArray(Vec::<f32>::try_from(value)?),
            DataType::DoubleArray => MetricValueKind::DoubleArray(Vec::<f64>::try_from(value)?),
            DataType::BooleanArray => MetricValueKind::BooleanArray(Vec::<bool>::try_from(value)?),
            DataType::StringArray => MetricValueKind::StringArray(Vec::<String>::try_from(value)?),
            DataType::DateTimeArray => {
                MetricValueKind::DateTimeArray(Vec::<DateTime>::try_from(value)?)
            }
        };
        Ok(out)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /*
      Test a metric value can be transformed into a protobuf metric value
    */
    macro_rules! test_metric_value_to_proto_and_back {
        ($metric_value:expr, $proto_type:ident) => {
            let proto_val: metric::Value = $metric_value.clone().into();
            //protobuf value variant is as expected
            assert!(matches!(proto_val, metric::Value::$proto_type(_)));
        };
    }

    /*
      Test a value for a type can be transformed into a MetricValue into the protobuf metric value struct and back.
    */
    macro_rules! test_value_to_proto_value_and_back {
        ($type:ty, $starting_value:expr, $proto_type:ident) => {
            let val_metric_value: MetricValue = ($starting_value as $type).into();

            test_metric_value_to_proto_and_back!(val_metric_value, $proto_type);

            let out: $type = val_metric_value.try_into().unwrap();
            assert_eq!($starting_value as $type, out);
        };
    }

    macro_rules! test_numeric_mix_max_to_proto_value_and_back {
        ($type:ty, $proto_type:ident) => {
            test_value_to_proto_value_and_back!($type, <$type>::MIN, $proto_type);
            test_value_to_proto_value_and_back!($type, <$type>::MAX, $proto_type);
        };
    }

    mod types {
        use super::*;
        use traits::HasDataType;

        #[test]
        fn i8() {
            test_numeric_mix_max_to_proto_value_and_back!(i8, IntValue);
        }

        #[test]
        fn i16() {
            test_numeric_mix_max_to_proto_value_and_back!(i16, IntValue);
        }

        #[test]
        fn i32() {
            test_numeric_mix_max_to_proto_value_and_back!(i32, IntValue);
        }

        #[test]
        fn i64() {
            test_numeric_mix_max_to_proto_value_and_back!(i64, LongValue);
        }

        #[test]
        fn u8() {
            test_numeric_mix_max_to_proto_value_and_back!(u8, IntValue);
        }

        #[test]
        fn u16() {
            test_numeric_mix_max_to_proto_value_and_back!(u16, IntValue);
        }

        #[test]
        fn u32() {
            test_numeric_mix_max_to_proto_value_and_back!(u32, IntValue);
        }

        #[test]
        fn u64() {
            test_numeric_mix_max_to_proto_value_and_back!(u64, LongValue);
        }

        #[test]
        fn f32() {
            test_numeric_mix_max_to_proto_value_and_back!(f32, FloatValue);
        }

        #[test]
        fn f64() {
            test_numeric_mix_max_to_proto_value_and_back!(f64, DoubleValue);
        }

        #[test]
        fn bool() {
            test_value_to_proto_value_and_back!(bool, false, BooleanValue);
            test_value_to_proto_value_and_back!(bool, true, BooleanValue);
        }

        #[test]
        fn string() {
            test_value_to_proto_value_and_back!(String, "test".to_string(), StringValue);
        }

        #[test]
        fn datetime() {
            test_value_to_proto_value_and_back!(DateTime, DateTime::new(0), LongValue);
        }

        #[test]
        fn bool_array() {
            test_value_to_proto_value_and_back!(
                Vec<bool>,
                vec![false, false, true, true, false, true, false, false, true, true, false, true],
                BytesValue
            );
        }

        #[test]
        fn datetime_array() {
            test_value_to_proto_value_and_back!(
                Vec<DateTime>,
                vec![DateTime::new(1), DateTime::new(42)],
                BytesValue
            );
        }

        #[test]
        fn metric_value_default_datatypes() {
            assert_eq!(bool::default_datatype(), DataType::Boolean);
            assert_eq!(i8::default_datatype(), DataType::Int8);
            assert_eq!(i16::default_datatype(), DataType::Int16);
            assert_eq!(i32::default_datatype(), DataType::Int32);
            assert_eq!(i64::default_datatype(), DataType::Int64);
            assert_eq!(u8::default_datatype(), DataType::UInt8);
            assert_eq!(u16::default_datatype(), DataType::UInt16);
            assert_eq!(u32::default_datatype(), DataType::UInt32);
            assert_eq!(u64::default_datatype(), DataType::UInt64);
            assert_eq!(f32::default_datatype(), DataType::Float);
            assert_eq!(f64::default_datatype(), DataType::Double);
            assert_eq!(String::default_datatype(), DataType::String);
            assert_eq!(DateTime::default_datatype(), DataType::DateTime);
            assert_eq!(Vec::<bool>::default_datatype(), DataType::BooleanArray);
            assert_eq!(Vec::<i8>::default_datatype(), DataType::Int8Array);
            assert_eq!(Vec::<i16>::default_datatype(), DataType::Int16Array);
            assert_eq!(Vec::<i32>::default_datatype(), DataType::Int32Array);
            assert_eq!(Vec::<i64>::default_datatype(), DataType::Int64Array);
            assert_eq!(Vec::<u8>::default_datatype(), DataType::UInt8Array);
            assert_eq!(Vec::<u16>::default_datatype(), DataType::UInt16Array);
            assert_eq!(Vec::<u32>::default_datatype(), DataType::UInt32Array);
            assert_eq!(Vec::<u64>::default_datatype(), DataType::UInt64Array);
            assert_eq!(Vec::<f32>::default_datatype(), DataType::FloatArray);
            assert_eq!(Vec::<f64>::default_datatype(), DataType::DoubleArray);
            assert_eq!(Vec::<String>::default_datatype(), DataType::StringArray);
        }
    }

    mod invalid_from_bytes_vec_conversion {
        use crate::value::{proto_to_bool_vec, proto_to_string_vec};

        fn test_bool_array_small_buffer(bool_count: u32, bool_byes_size: usize) {
            let mut bytes = bool_count.to_le_bytes().to_vec();
            bytes.resize(bool_byes_size, 0);
            assert!(proto_to_bool_vec(bytes).is_err())
        }

        #[test]
        fn bool() {
            //invalid bytes prefix
            let mut bytes = Vec::new();
            bytes.extend(vec![0, 0, 0]);
            assert!(proto_to_bool_vec(bytes).is_err());

            //valid prefix but size does not match
            test_bool_array_small_buffer(1, 0);
            test_bool_array_small_buffer(4, 0);
            test_bool_array_small_buffer(8, 0);
            test_bool_array_small_buffer(9, 1);
        }

        #[test]
        fn string() {
            /*non terminated string */
            assert!(proto_to_string_vec(vec![0x1]).is_err());
            /* Invalid utf8 string */
            assert!(proto_to_string_vec(b"Hello \xF0\x90\x80World\x00".to_vec()).is_err());
        }
    }

    mod array_type_bytes_conversion {
        use super::*;

        fn create_bool_bytes_vec(bool_count: u32, bool_bytes: Vec<u8>) -> Vec<u8> {
            let mut vec = bool_count.to_le_bytes().to_vec();
            vec.extend(bool_bytes);
            vec
        }

        #[test]
        fn bool() {
            let start = vec![true];
            let bytes = bool_vec_to_proto(start.clone());
            assert_eq!(bytes, create_bool_bytes_vec(1, vec![0b1000_0000]));
            assert_eq!(proto_to_bool_vec(bytes).unwrap(), start);

            let start = vec![true, false, true, false, true, true, true, false, true];
            let bytes = bool_vec_to_proto(start.clone());
            assert_eq!(
                bytes,
                create_bool_bytes_vec(9, vec![0b1010_1110, 0b1000_0000])
            );
            assert_eq!(proto_to_bool_vec(bytes).unwrap(), start);

            let start = vec![
                false, false, true, true, false, true, false, false, true, true, false, true,
            ];
            let bytes = bool_vec_to_proto(start.clone());
            assert_eq!(
                bytes,
                create_bool_bytes_vec(12, vec![0b0011_0100, 0b1101_0000])
            );
            assert_eq!(proto_to_bool_vec(bytes).unwrap(), start);
        }

        #[test]
        fn string() {
            let start = vec!["test".to_string()];
            let bytes = string_vec_to_proto(start.clone());
            assert_eq!(bytes, b"test\x00".to_vec());
            assert_eq!(proto_to_string_vec(bytes).unwrap(), start);

            let start = vec!["abc".to_string(), "123".to_string()];
            let bytes = string_vec_to_proto(start.clone());
            assert_eq!(bytes, b"abc\x00123\x00".to_vec());
            assert_eq!(proto_to_string_vec(bytes).unwrap(), start);

            let start = vec!["abc".to_string(), "".to_string(), "cba".to_string()];
            let bytes = string_vec_to_proto(start.clone());
            assert_eq!(bytes, b"abc\x00\x00cba\x00".to_vec());
            assert_eq!(proto_to_string_vec(bytes).unwrap(), start);
        }

        #[test]
        fn test_vec_u8_conversion_invalid_data() {
            /* invalid size data */
            let data = vec![0x00_u8, 0x01, 0x02, 0x03, 0x04];
            assert!(proto_to_u16_vec(data.clone()).is_err());
            assert!(proto_to_u32_vec(data.clone()).is_err());
            assert!(proto_to_u64_vec(data).is_err());
        }

        macro_rules! test_numeric_vec_u8_conversion{
        ($($t:ty), *) => {
          paste! {
            $(
              let vec = vec![0 as $t, <$t>::MIN, <$t>::MAX];
              assert_eq!(vec, [<proto_to_$t:lower _vec>]([<$t:lower _vec_to_proto>](vec.clone())).unwrap());
            )*
          }
        };
    }

        #[test]
        fn test_standard_vec_u8_convertable_types() {
            test_numeric_vec_u8_conversion!(u16, u32, u64, i8, i16, i32, i64, f32, f64);
            let vec = vec![
                DateTime::new(0),
                DateTime::new(u64::MIN),
                DateTime::new(u64::MAX),
            ];
            assert_eq!(
                vec,
                proto_to_datetime_vec(datetime_vec_to_proto(vec.clone())).unwrap()
            );
        }
    }
}
