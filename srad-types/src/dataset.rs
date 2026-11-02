use std::marker::PhantomData;

use thiserror::Error;

use crate::{
    payload::{
        self,
        data_set::{self, data_set_value},
        metric, DataType,
    },
    traits::{self, HasDataType},
    value, FromValueTypeError, MetricValue, TemplateMetricValuePartial,
};

/// A Sparkplug DataSet column definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataSetColumn {
    pub name: &'static str,
    pub datatype: DataType,
}

impl DataSetColumn {
    pub const fn new(name: &'static str, datatype: DataType) -> Self {
        Self { name, datatype }
    }
}

/// Low-level Sparkplug DataSet wrapper.
///
/// Does not validate column or row consistency. Prefer [`TypedDataSet`], which provides
/// typed rows and validated columns and values.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataSet(pub payload::DataSet);

impl DataSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn init<C, S, T>(&mut self, columns: C, types: T) -> &mut Self
    where
        C: IntoIterator<Item = S>,
        S: Into<String>,
        T: IntoIterator<Item = DataType>,
    {
        let columns: Vec<String> = columns.into_iter().map(Into::into).collect();
        self.0.num_of_columns = Some(columns.len() as u64);
        self.0.columns = columns;
        self.0.types = types.into_iter().map(|t| t as u32).collect();
        self
    }

    pub fn add_row(&mut self, row: data_set::Row) -> &mut Self {
        self.0.rows.push(row);
        self
    }

    pub fn add_row_values<I, V>(&mut self, elements: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: Into<value::DataSetValue>,
    {
        let elements = elements
            .into_iter()
            .map(|v| data_set::DataSetValue {
                value: Some(v.into().0),
            })
            .collect();
        self.add_row(data_set::Row { elements })
    }

    pub fn as_proto(&self) -> &payload::DataSet {
        &self.0
    }

    pub fn into_proto(self) -> payload::DataSet {
        self.0
    }
}

impl From<DataSet> for payload::DataSet {
    fn from(value: DataSet) -> Self {
        value.0
    }
}

impl From<payload::DataSet> for DataSet {
    fn from(value: payload::DataSet) -> Self {
        Self(value)
    }
}

impl HasDataType for DataSet {
    const DEFAULT_DATATYPE: DataType = DataType::DataSet;

    fn supported_datatypes() -> &'static [DataType] {
        static SUPPORTED_TYPES: [DataType; 1] = [DataType::DataSet];
        &SUPPORTED_TYPES
    }
}

impl traits::MetricValue for DataSet {}

impl From<DataSet> for MetricValue {
    fn from(value: DataSet) -> Self {
        MetricValue::new(metric::Value::DatasetValue(value.0))
    }
}

impl TryFrom<MetricValue> for DataSet {
    type Error = FromValueTypeError;

    fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        if let metric::Value::DatasetValue(value) = value.0 {
            Ok(Self(value))
        } else {
            Err(FromValueTypeError::InvalidVariantType)
        }
    }
}

impl TemplateMetricValuePartial for DataSet {
    type Error = ();

    fn metric_value_if_ne(&self, other: &Self) -> Option<Option<MetricValue>> {
        if self == other {
            None
        } else {
            Some(Some(self.clone().into()))
        }
    }

    fn try_update_from_metric_value(
        &mut self,
        other: Option<MetricValue>,
    ) -> Result<(), Self::Error> {
        *self = Self::try_from(other.ok_or(())?).map_err(|_| ())?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum DataSetError {
    #[error("expected {expected} columns but got {actual}")]
    ColumnCountMismatch { expected: usize, actual: usize },
    #[error("column {index} has datatype {actual} but expected {expected:?}")]
    ColumnTypeMismatch {
        index: usize,
        expected: DataType,
        actual: u32,
    },
    #[error("column {index} has name {actual:?} but expected {expected:?}")]
    ColumnNameMismatch {
        index: usize,
        expected: &'static str,
        actual: String,
    },
    #[error("expected {expected} row elements but got {actual}")]
    RowLengthMismatch { expected: usize, actual: usize },
    #[error("row element {index} has an unexpected datatype, expected {expected:?}")]
    ElementTypeMismatch { index: usize, expected: DataType },
    #[error("row element {index} has no value")]
    ElementValueMissing { index: usize },
    #[error("metric value was not a DataSet")]
    InvalidMetricValue,
    #[error(transparent)]
    InvalidElementValue(#[from] FromValueTypeError),
}

/// Defines the columns and row conversion for a [`TypedDataSet`].
pub trait DataSetRow: Sized {
    const COLUMNS: &'static [DataSetColumn];
    const COLUMN_COUNT: usize = Self::COLUMNS.len();

    fn into_row(self) -> data_set::Row;

    fn try_from_row(row: data_set::Row) -> Result<Self, DataSetError>;

    fn validate_row(row: &data_set::Row) -> Result<(), DataSetError> {
        if row.elements.len() != Self::COLUMN_COUNT {
            return Err(DataSetError::RowLengthMismatch {
                expected: Self::COLUMN_COUNT,
                actual: row.elements.len(),
            });
        }
        for (index, element) in row.elements.iter().enumerate() {
            let value = element
                .value
                .as_ref()
                .ok_or(DataSetError::ElementValueMissing { index })?;
            let column = Self::COLUMNS[index];
            if !datatype_matches(column.datatype, value) {
                return Err(DataSetError::ElementTypeMismatch {
                    index,
                    expected: column.datatype,
                });
            }
        }
        Ok(())
    }
}

/// Typed Sparkplug DataSet wrapper.
pub struct TypedDataSet<R: DataSetRow> {
    inner: DataSet,
    _row: PhantomData<fn() -> R>,
}

impl<R: DataSetRow> Clone for TypedDataSet<R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _row: PhantomData,
        }
    }
}

impl<R: DataSetRow> PartialEq for TypedDataSet<R> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<R: DataSetRow> std::fmt::Debug for TypedDataSet<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TypedDataSet").field(&self.inner).finish()
    }
}

impl<R: DataSetRow> Default for TypedDataSet<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: DataSetRow> TypedDataSet<R> {
    /// Creates an empty dataset with the schema defined by [`DataSetRow::COLUMNS`].
    pub fn new() -> Self {
        let mut inner = DataSet::new();
        inner.init(
            R::COLUMNS.iter().map(|column| column.name),
            R::COLUMNS.iter().map(|column| column.datatype),
        );
        Self {
            inner,
            _row: PhantomData,
        }
    }

    pub fn add_row(&mut self, row: R) -> &mut Self {
        self.inner.add_row(row.into_row());
        self
    }

    pub fn try_add_row(&mut self, row: data_set::Row) -> Result<&mut Self, DataSetError> {
        R::validate_row(&row)?;
        self.inner.add_row(row);
        Ok(self)
    }

    pub fn as_data_set(&self) -> &DataSet {
        &self.inner
    }

    pub fn into_data_set(self) -> DataSet {
        self.inner
    }

    pub fn as_proto(&self) -> &payload::DataSet {
        self.inner.as_proto()
    }

    pub fn into_proto(self) -> payload::DataSet {
        self.inner.into_proto()
    }
}

impl<R: DataSetRow> TryFrom<DataSet> for TypedDataSet<R> {
    type Error = DataSetError;

    fn try_from(value: DataSet) -> Result<Self, Self::Error> {
        let actual_columns = value.0.num_of_columns.unwrap_or_default() as usize;
        if actual_columns != R::COLUMN_COUNT {
            return Err(DataSetError::ColumnCountMismatch {
                expected: R::COLUMN_COUNT,
                actual: actual_columns,
            });
        }
        if value.0.columns.len() != R::COLUMN_COUNT {
            return Err(DataSetError::ColumnCountMismatch {
                expected: R::COLUMN_COUNT,
                actual: value.0.columns.len(),
            });
        }
        if value.0.types.len() != R::COLUMN_COUNT {
            return Err(DataSetError::ColumnCountMismatch {
                expected: R::COLUMN_COUNT,
                actual: value.0.types.len(),
            });
        }
        for (index, (actual, column)) in value.0.columns.iter().zip(R::COLUMNS).enumerate() {
            if actual.as_str() != column.name {
                return Err(DataSetError::ColumnNameMismatch {
                    index,
                    expected: column.name,
                    actual: actual.clone(),
                });
            }
        }
        for (index, (actual, column)) in value.0.types.iter().zip(R::COLUMNS).enumerate() {
            if *actual != column.datatype as u32 {
                return Err(DataSetError::ColumnTypeMismatch {
                    index,
                    expected: column.datatype,
                    actual: *actual,
                });
            }
        }
        for row in &value.0.rows {
            R::validate_row(row)?;
        }

        Ok(Self {
            inner: value,
            _row: PhantomData,
        })
    }
}

impl<R: DataSetRow> TryFrom<payload::DataSet> for TypedDataSet<R> {
    type Error = DataSetError;

    fn try_from(value: payload::DataSet) -> Result<Self, Self::Error> {
        DataSet::from(value).try_into()
    }
}

impl<R: DataSetRow> From<TypedDataSet<R>> for DataSet {
    fn from(value: TypedDataSet<R>) -> Self {
        value.inner
    }
}

impl<R: DataSetRow> From<TypedDataSet<R>> for payload::DataSet {
    fn from(value: TypedDataSet<R>) -> Self {
        value.inner.into()
    }
}

impl<R: DataSetRow> From<TypedDataSet<R>> for MetricValue {
    fn from(value: TypedDataSet<R>) -> Self {
        value.inner.into()
    }
}

impl<R: DataSetRow> TryFrom<MetricValue> for TypedDataSet<R> {
    type Error = DataSetError;

    fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        DataSet::try_from(value)
            .map_err(|_| DataSetError::InvalidMetricValue)?
            .try_into()
    }
}

impl<R: DataSetRow> HasDataType for TypedDataSet<R> {
    const DEFAULT_DATATYPE: DataType = DataType::DataSet;

    fn supported_datatypes() -> &'static [DataType] {
        static SUPPORTED_TYPES: [DataType; 1] = [DataType::DataSet];
        &SUPPORTED_TYPES
    }
}

impl<R: DataSetRow> traits::MetricValue for TypedDataSet<R> {}

impl<R: DataSetRow> TemplateMetricValuePartial for TypedDataSet<R> {
    type Error = ();

    fn metric_value_if_ne(&self, other: &Self) -> Option<Option<MetricValue>> {
        if self == other {
            None
        } else {
            Some(Some(self.clone().into()))
        }
    }

    fn try_update_from_metric_value(
        &mut self,
        other: Option<MetricValue>,
    ) -> Result<(), Self::Error> {
        *self = Self::try_from(other.ok_or(())?).map_err(|_| ())?;
        Ok(())
    }
}

fn datatype_matches(datatype: DataType, value: &data_set_value::Value) -> bool {
    use data_set_value::Value;

    match datatype {
        DataType::Int8 => matches!(value, Value::IntValue(v) if int8_fits(*v)),
        DataType::Int16 => matches!(value, Value::IntValue(v) if int16_fits(*v)),
        DataType::Int32 => matches!(value, Value::IntValue(_)),
        DataType::UInt8 => matches!(value, Value::IntValue(v) if *v <= u8::MAX as u32),
        DataType::UInt16 => matches!(value, Value::IntValue(v) if *v <= u16::MAX as u32),
        DataType::UInt32 => matches!(value, Value::IntValue(_)),
        DataType::Int64 | DataType::UInt64 | DataType::DateTime => {
            matches!(value, Value::LongValue(_))
        }
        DataType::Float => matches!(value, Value::FloatValue(_)),
        DataType::Double => matches!(value, Value::DoubleValue(_)),
        DataType::Boolean => matches!(value, Value::BooleanValue(_)),
        DataType::String | DataType::Text | DataType::Uuid => {
            matches!(value, Value::StringValue(_))
        }
        _ => false,
    }
}

fn int8_fits(value: u32) -> bool {
    let narrowed = value as u8;
    value == narrowed as u32 || value == narrowed as i8 as i32 as u32
}

fn int16_fits(value: u32) -> bool {
    let narrowed = value as u16;
    value == narrowed as u32 || value == narrowed as i16 as i32 as u32
}

#[doc(hidden)]
pub fn data_set_value_from<T>(value: T) -> data_set_value::Value
where
    T: traits::DataSetValue,
{
    let value: value::DataSetValue = value.into();
    value.0
}

#[doc(hidden)]
pub fn data_set_value_try_into<T>(
    element: data_set::DataSetValue,
    expected: DataType,
    index: usize,
) -> Result<T, DataSetError>
where
    T: traits::DataSetValue + TryFrom<value::DataSetValue, Error = FromValueTypeError>,
{
    let value = element
        .value
        .ok_or(DataSetError::ElementValueMissing { index })?;
    if !datatype_matches(expected, &value) {
        return Err(DataSetError::ElementTypeMismatch { index, expected });
    }
    Ok(T::try_from(value::DataSetValue(value))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{payload::data_set::data_set_value::Value, DateTime};

    #[derive(Clone, Debug, PartialEq)]
    struct TestRow {
        a: i32,
        b: f64,
    }

    impl DataSetRow for TestRow {
        const COLUMNS: &'static [DataSetColumn] = &[
            DataSetColumn::new("a", DataType::Int32),
            DataSetColumn::new("b", DataType::Double),
        ];

        fn into_row(self) -> data_set::Row {
            data_set::Row {
                elements: vec![
                    data_set::DataSetValue {
                        value: Some(data_set_value_from(self.a)),
                    },
                    data_set::DataSetValue {
                        value: Some(data_set_value_from(self.b)),
                    },
                ],
            }
        }

        fn try_from_row(row: data_set::Row) -> Result<Self, DataSetError> {
            if row.elements.len() != Self::COLUMN_COUNT {
                return Err(DataSetError::RowLengthMismatch {
                    expected: Self::COLUMN_COUNT,
                    actual: row.elements.len(),
                });
            }
            let mut elements = row.elements.into_iter();
            Ok(Self {
                a: data_set_value_try_into::<i32>(
                    elements.next().expect("row length checked"),
                    Self::COLUMNS[0].datatype,
                    0,
                )?,
                b: data_set_value_try_into::<f64>(
                    elements.next().expect("row length checked"),
                    Self::COLUMNS[1].datatype,
                    1,
                )?,
            })
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct TestRow3 {
        a: i32,
        b: f64,
        c: String,
    }

    impl DataSetRow for TestRow3 {
        const COLUMNS: &'static [DataSetColumn] = &[
            DataSetColumn::new("a", DataType::Int32),
            DataSetColumn::new("b", DataType::Double),
            DataSetColumn::new("c", DataType::String),
        ];

        fn into_row(self) -> data_set::Row {
            data_set::Row {
                elements: vec![
                    data_set::DataSetValue {
                        value: Some(data_set_value_from(self.a)),
                    },
                    data_set::DataSetValue {
                        value: Some(data_set_value_from(self.b)),
                    },
                    data_set::DataSetValue {
                        value: Some(data_set_value_from(self.c)),
                    },
                ],
            }
        }

        fn try_from_row(row: data_set::Row) -> Result<Self, DataSetError> {
            if row.elements.len() != Self::COLUMN_COUNT {
                return Err(DataSetError::RowLengthMismatch {
                    expected: Self::COLUMN_COUNT,
                    actual: row.elements.len(),
                });
            }
            let mut elements = row.elements.into_iter();
            Ok(Self {
                a: data_set_value_try_into::<i32>(
                    elements.next().expect("row length checked"),
                    Self::COLUMNS[0].datatype,
                    0,
                )?,
                b: data_set_value_try_into::<f64>(
                    elements.next().expect("row length checked"),
                    Self::COLUMNS[1].datatype,
                    1,
                )?,
                c: data_set_value_try_into::<String>(
                    elements.next().expect("row length checked"),
                    Self::COLUMNS[2].datatype,
                    2,
                )?,
            })
        }
    }

    fn data_set() -> DataSet {
        let mut data_set = DataSet::new();
        data_set
            .init(["time", "value"], [DataType::DateTime, DataType::Double])
            .add_row_values([
                value::DataSetValue::from(DateTime::new(1)),
                value::DataSetValue::from(1.5),
            ]);
        data_set
    }

    #[test]
    fn init_sets_schema() {
        let data_set = data_set();
        assert_eq!(data_set.0.num_of_columns, Some(2));
        assert_eq!(data_set.0.columns, vec!["time", "value"]);
        assert_eq!(
            data_set.0.types,
            vec![DataType::DateTime as u32, DataType::Double as u32]
        );
        assert_eq!(data_set.0.rows.len(), 1);
    }

    #[test]
    fn low_level_add_row_does_not_validate() {
        let mut data_set = data_set();
        data_set.add_row(data_set::Row { elements: vec![] });
        assert_eq!(data_set.0.rows.len(), 2);
    }

    #[test]
    fn metric_value_round_trip() {
        let data_set = data_set();
        let metric_value: MetricValue = data_set.clone().into();
        assert!(matches!(metric_value.0, metric::Value::DatasetValue(_)));
        assert_eq!(DataSet::try_from(metric_value).unwrap(), data_set);
    }

    #[test]
    fn typed_new_sets_schema() {
        let data_set = TypedDataSet::<TestRow>::new();
        assert_eq!(data_set.as_proto().num_of_columns, Some(2));
        assert_eq!(data_set.as_proto().columns, vec!["a", "b"]);
        assert_eq!(
            data_set.as_proto().types,
            vec![DataType::Int32 as u32, DataType::Double as u32]
        );
    }

    #[test]
    fn typed_add_row_builds_elements() {
        let mut data_set = TypedDataSet::<TestRow3>::new();
        data_set.add_row(TestRow3 {
            a: 1,
            b: 2.5,
            c: "three".into(),
        });

        let proto = data_set.as_proto();
        assert_eq!(proto.num_of_columns, Some(3));
        assert_eq!(proto.columns, vec!["a", "b", "c"]);
        assert_eq!(
            proto.types,
            vec![
                DataType::Int32 as u32,
                DataType::Double as u32,
                DataType::String as u32
            ]
        );
        assert!(matches!(
            proto.rows[0].elements[0].value,
            Some(Value::IntValue(1))
        ));
        assert!(matches!(
            proto.rows[0].elements[1].value,
            Some(Value::DoubleValue(2.5))
        ));
        assert!(matches!(
            proto.rows[0].elements[2].value,
            Some(Value::StringValue(ref v)) if v == "three"
        ));
    }

    #[test]
    fn typed_try_add_row_validates() {
        let mut data_set = TypedDataSet::<TestRow>::new();

        let wrong_length = data_set::Row {
            elements: vec![data_set::DataSetValue {
                value: Some(Value::IntValue(1)),
            }],
        };
        assert!(matches!(
            data_set.try_add_row(wrong_length),
            Err(DataSetError::RowLengthMismatch {
                expected: 2,
                actual: 1
            })
        ));

        let wrong_type = data_set::Row {
            elements: vec![
                data_set::DataSetValue {
                    value: Some(Value::StringValue("one".into())),
                },
                data_set::DataSetValue {
                    value: Some(Value::DoubleValue(2.0)),
                },
            ],
        };
        assert!(matches!(
            data_set.try_add_row(wrong_type),
            Err(DataSetError::ElementTypeMismatch {
                index: 0,
                expected: DataType::Int32
            })
        ));
    }

    #[test]
    fn integer_ranges_are_validated() {
        let element = |value: u32| data_set::DataSetValue {
            value: Some(Value::IntValue(value)),
        };

        assert!(matches!(
            data_set_value_try_into::<i8>(element(256), DataType::Int8, 0),
            Err(DataSetError::ElementTypeMismatch { .. })
        ));
        assert!(matches!(
            data_set_value_try_into::<u8>(element(u32::MAX), DataType::UInt8, 0),
            Err(DataSetError::ElementTypeMismatch { .. })
        ));
        assert!(matches!(
            data_set_value_try_into::<i16>(element(0x0001_0000), DataType::Int16, 0),
            Err(DataSetError::ElementTypeMismatch { .. })
        ));

        assert_eq!(
            data_set_value_try_into::<i8>(element(u32::MAX), DataType::Int8, 0).unwrap(),
            -1
        );
        assert_eq!(
            data_set_value_try_into::<i16>(element(0x0000_FFF8), DataType::Int16, 0).unwrap(),
            -8
        );
    }

    #[test]
    fn typed_metric_value_round_trip() {
        let mut data_set = TypedDataSet::<TestRow>::new();
        data_set.add_row(TestRow { a: 1, b: 2.5 });

        let metric_value: MetricValue = data_set.clone().into();
        assert_eq!(
            TypedDataSet::<TestRow>::try_from(metric_value).unwrap(),
            data_set
        );
    }

    #[test]
    fn typed_from_data_set_validates_schema() {
        let mut data_set = DataSet::new();
        data_set.init(["a", "b"], [DataType::Int32, DataType::Double]);

        assert!(TypedDataSet::<TestRow>::try_from(data_set.clone()).is_ok());

        let mut wrong_types = data_set.clone();
        wrong_types.0.types = vec![DataType::Int32 as u32, DataType::Float as u32];
        assert!(matches!(
            TypedDataSet::<TestRow>::try_from(wrong_types),
            Err(DataSetError::ColumnTypeMismatch {
                index: 1,
                expected: DataType::Double,
                ..
            })
        ));

        let mut wrong_names = data_set.clone();
        wrong_names.0.columns = vec!["a".into(), "c".into()];
        assert!(matches!(
            TypedDataSet::<TestRow>::try_from(wrong_names),
            Err(DataSetError::ColumnNameMismatch {
                index: 1,
                expected: "b",
                ..
            })
        ));

        let mut wrong_row = data_set;
        wrong_row.add_row(data_set::Row {
            elements: vec![data_set::DataSetValue {
                value: Some(Value::IntValue(1)),
            }],
        });
        assert!(matches!(
            TypedDataSet::<TestRow>::try_from(wrong_row),
            Err(DataSetError::RowLengthMismatch { .. })
        ));
    }
}
