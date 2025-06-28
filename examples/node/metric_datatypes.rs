use std::sync::Mutex;

use srad::{
    client_rumqtt as rumqtt,
    eon::{BirthMetricDetails, EoNBuilder, MetricManager, NodeMetricManager},
    types::{payload::DataType, DateTime},
};

use log::LevelFilter;

struct Data {
    boolean: bool,
    int8: i8,
    int16: i16,
    int32: i32,
    int64: i64,
    uint8: u8,
    uint16: u16,
    uint32: u32,
    uint64: u64,
    float: f32,
    double: f64,
    string: String,
    datetime: DateTime,
    text: String,
    //uuid
    //dataset
    bytes: Vec<u8>,
    //file
    //template
    bool_array: Vec<bool>,
    int8_array: Vec<i8>,
    int16_array: Vec<i16>,
    int32_array: Vec<i32>,
    int64_array: Vec<i64>,
    uint8_array: Vec<u8>,
    uint16_array: Vec<u16>,
    uint32_array: Vec<u32>,
    uint64_array: Vec<u64>,
    float_array: Vec<f32>,
    double_array: Vec<f64>,
    string_array: Vec<String>,
    datetime_array: Vec<DateTime>,
}

struct DatatypesManager(Mutex<Data>);

impl DatatypesManager {
    fn new() -> Self {
        Self(Mutex::new(Data {
            boolean: true,
            int8: i8::MIN,
            int16: i16::MIN,
            int32: i32::MIN,
            int64: i64::MIN,
            uint8: u8::MAX,
            uint16: u16::MAX,
            uint32: u32::MAX,
            uint64: u64::MAX,
            float: std::f32::consts::PI,
            double: std::f64::consts::PI,
            string: "Hello, World!".into(),
            datetime: DateTime::new(0),
            text: "Some Text".into(),
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
            bool_array: vec![
                true, false, true, false, true, true, true, false, true, false, false,
            ],
            int8_array: vec![i8::MAX, i8::MIN, 0, 1],
            int16_array: vec![i16::MAX, i16::MIN, 0, 1],
            int32_array: vec![i32::MAX, i32::MIN, 0, 1],
            int64_array: vec![i64::MAX, i64::MIN, 0, 1],
            uint8_array: vec![u8::MAX, u8::MIN, 0, 1],
            uint16_array: vec![u16::MAX, u16::MIN, 0, 1],
            uint32_array: vec![u32::MAX, u32::MIN, 0, 1],
            uint64_array: vec![u64::MAX, u64::MIN, 0, 1],
            float_array: vec![f32::MAX, f32::MIN, 0.0, 1.0, std::f32::consts::PI],
            double_array: vec![f64::MAX, f64::MIN, 0.0, 1.0, std::f64::consts::PI],
            string_array: vec!["Hello".into(), ",".into(), "World".into(), "!".into()],
            datetime_array: vec![DateTime::new(0), DateTime::new(1), DateTime::new(999999)],
        }))
    }
}

impl MetricManager for DatatypesManager {
    fn initialise_birth(&self, bi: &mut srad::eon::BirthInitializer) {
        let data = self.0.lock().unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Bool",
            data.boolean,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int8", data.int8,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int16", data.int16,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int32", data.int32,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int64", data.int64,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt8", data.uint8,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt16",
            data.uint16,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt32",
            data.uint32,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt64",
            data.uint64,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Float", data.float,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Double",
            data.double,
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "String",
            data.string.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "DateTime",
            data.datetime.clone(),
        ))
        .unwrap();
        bi.register_metric(
            BirthMetricDetails::new_with_initial_value_explicit_type(
                "Text",
                data.text.clone(),
                DataType::Text,
            )
            .unwrap(),
        )
        .unwrap();
        bi.register_metric(
            BirthMetricDetails::new_with_initial_value_explicit_type(
                "Bytes",
                data.bytes.clone(),
                DataType::Bytes,
            )
            .unwrap(),
        )
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "BoolArray",
            data.bool_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int8Array",
            data.int8_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int16Array",
            data.int16_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int32Array",
            data.int32_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "Int64Array",
            data.int64_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt8Array",
            data.uint8_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt16Array",
            data.uint16_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt32Array",
            data.uint32_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "UInt64Array",
            data.uint64_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "FloatArray",
            data.float_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "DoubleArray",
            data.double_array.clone(),
        ))
        .unwrap();
        bi.register_metric(BirthMetricDetails::new_with_initial_value(
            "StringArray",
            data.string_array.clone(),
        ))
        .unwrap();
        bi.register_metric(
            BirthMetricDetails::new_with_initial_value_explicit_type(
                "DateTimeArray",
                data.datetime_array.clone(),
                DataType::DateTimeArray,
            )
            .unwrap(),
        )
        .unwrap();
    }
}

impl NodeMetricManager for DatatypesManager {}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
        .init();

    let opts = rumqtt::MqttOptions::new("node", "localhost", 1883);

    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);

    let (eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("foo")
        .with_node_id("bar")
        .with_metric_manager(DatatypesManager::new())
        .build()
        .unwrap();

    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            println!("Failed to register CTRL-C handler: {e}");
            return;
        }
        handle.cancel().await;
    });

    eon.run().await;
}
