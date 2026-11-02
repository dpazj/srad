use srad::types::{
    payload::{data_set::data_set_value::Value, DataType},
    DataSetColumn, DataSetRow, MetricValue, TypedDataSet,
};

#[derive(DataSetRow, Clone, Debug, PartialEq)]
struct Row {
    #[dataset(rename = "X")]
    x: i32,
    y: f64,
}

#[test]
fn derive_defines_const_columns() {
    assert_eq!(
        Row::COLUMNS,
        &[
            DataSetColumn::new("X", DataType::Int32),
            DataSetColumn::new("y", DataType::Double),
        ]
    );
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
    let mut data_set = TypedDataSet::<Row>::new();
    data_set.add_row(Row { x: 1, y: 2.5 });

    let proto = data_set.to_proto();
    assert_eq!(proto.columns, vec!["X", "y"]);
    assert_eq!(
        proto.types,
        vec![DataType::Int32 as u32, DataType::Double as u32]
    );

    let metric_value: MetricValue = data_set.clone().into();
    assert_eq!(
        TypedDataSet::<Row>::try_from(metric_value).unwrap(),
        data_set
    );
}

#[test]
fn raw_identifier_uses_unraw_column_name() {
    #[derive(DataSetRow, Clone, Debug, PartialEq)]
    struct RawRow {
        r#type: i32,
    }

    let row = RawRow { r#type: 1 };
    assert_eq!(RawRow::COLUMNS[0].name, "type");
    assert_eq!(RawRow::try_from_row(row.into_row()).unwrap().r#type, 1);
}

#[test]
fn generic_rows_infer_bounds() {
    #[derive(DataSetRow, Clone, Debug, PartialEq)]
    struct GenericRow<T> {
        value: T,
    }

    let row = GenericRow { value: 1_i32 };
    assert_eq!(GenericRow::<i32>::COLUMNS[0].datatype, DataType::Int32);
    assert_eq!(
        GenericRow::<i32>::try_from_row(row.clone().into_row()).unwrap(),
        row
    );
}

#[derive(DataSetRow, Clone, Debug, PartialEq)]
struct TextRow {
    #[dataset(datatype = "Text")]
    text: String,
    #[dataset(datatype = "Uuid")]
    id: String,
}

#[test]
fn datatype_override_sets_column_type() {
    assert_eq!(TextRow::COLUMNS[0].datatype, DataType::Text);
    assert_eq!(TextRow::COLUMNS[1].datatype, DataType::Uuid);

    let row = TextRow {
        text: "hello".into(),
        id: "some-uuid".into(),
    };
    let mut data_set = TypedDataSet::<TextRow>::new();
    data_set.add_row(row);

    let proto = data_set.to_proto();
    assert_eq!(
        proto.types,
        vec![DataType::Text as u32, DataType::Uuid as u32]
    );
    assert_eq!(TypedDataSet::<TextRow>::try_from(proto).unwrap(), data_set);
}

#[derive(DataSetRow, Clone, Debug, PartialEq)]
struct OptionalRow {
    id: i32,
    value: Option<f64>,
    #[dataset(datatype = "Text")]
    label: Option<String>,
}

#[test]
fn option_fields_are_columns() {
    assert_eq!(OptionalRow::COLUMN_COUNT, 3);
    assert_eq!(OptionalRow::COLUMNS[1].datatype, DataType::Double);
    assert_eq!(OptionalRow::COLUMNS[2].datatype, DataType::Text);

    let mut data_set = TypedDataSet::<OptionalRow>::new();
    data_set.add_row(OptionalRow {
        id: 1,
        value: Some(1.5),
        label: None,
    });
    data_set.add_row(OptionalRow {
        id: 2,
        value: None,
        label: Some("two".into()),
    });

    let proto = data_set.to_proto();
    assert_eq!(
        proto.rows[0].elements[1].value,
        Some(Value::DoubleValue(1.5))
    );
    assert_eq!(proto.rows[0].elements[2].value, None);
    assert_eq!(proto.rows[1].elements[1].value, None);
    assert_eq!(
        proto.rows[1].elements[2].value,
        Some(Value::StringValue("two".into()))
    );

    assert_eq!(
        TypedDataSet::<OptionalRow>::try_from(proto).unwrap(),
        data_set
    );
}

#[derive(DataSetRow, Clone, Debug, PartialEq)]
struct SkippedRow {
    id: i32,
    #[dataset(skip)]
    marker: std::marker::PhantomData<u8>,
    #[dataset(skip)]
    internal: String,
}

#[test]
fn skipped_fields_are_not_columns() {
    assert_eq!(SkippedRow::COLUMN_COUNT, 1);
    assert_eq!(SkippedRow::COLUMNS[0].name, "id");

    let row = SkippedRow {
        id: 7,
        marker: std::marker::PhantomData,
        internal: "not encoded".into(),
    };
    let proto = row.into_row();
    assert_eq!(proto.elements.len(), 1);

    assert_eq!(
        SkippedRow::try_from_row(proto).unwrap(),
        SkippedRow {
            id: 7,
            marker: std::marker::PhantomData,
            internal: String::new(),
        }
    );
}
