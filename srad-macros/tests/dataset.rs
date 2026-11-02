use srad::types::{
    payload::{data_set::data_set_value::Value, DataType},
    DataSetRow, MetricValue, TypedDataSet,
};

#[derive(DataSetRow, Clone, Debug, PartialEq)]
struct Row {
    #[dataset(rename = "X")]
    x: i32,
    y: f64,
}

#[test]
fn derive_defines_const_columns() {
    assert_eq!(Row::COLUMNS, &["X", "y"]);
    assert_eq!(Row::COLUMN_TYPES, &[DataType::Int32, DataType::Double]);
    assert_eq!(Row::COLUMN_COUNT, 2);
}

#[test]
fn row_round_trip() {
    let row = Row { x: 1, y: 2.5 };
    let proto = row.clone().into_row();

    assert!(matches!(proto.elements[0].value, Some(Value::IntValue(1))));
    assert!(matches!(
        proto.elements[1].value,
        Some(Value::DoubleValue(2.5))
    ));
    assert_eq!(Row::try_from_row(proto).unwrap(), row);
}

#[test]
fn typed_data_set_round_trip() {
    let mut data_set = TypedDataSet::<Row>::new().unwrap();
    data_set.add_row(Row { x: 1, y: 2.5 });

    assert_eq!(data_set.as_proto().columns, vec!["X", "y"]);
    assert_eq!(
        data_set.as_proto().types,
        vec![DataType::Int32 as u32, DataType::Double as u32]
    );

    let metric_value: MetricValue = data_set.clone().into();
    assert_eq!(
        TypedDataSet::<Row>::try_from(metric_value).unwrap(),
        data_set
    );
}
