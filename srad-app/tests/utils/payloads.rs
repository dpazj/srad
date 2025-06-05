use srad_types::{
    constants::{BDSEQ, NODE_CONTROL_REBIRTH},
    payload::{metric, DataType, Metric, Payload},
    utils::timestamp,
    MetricValue,
};

pub fn new_nbirth_payload(metrics: Option<Vec<Metric>>) -> Payload {
    let mut bdseq = Metric::new();
    bdseq
        .set_name(BDSEQ.into())
        .set_datatype(DataType::Int64)
        .set_timestamp(timestamp())
        .set_value(MetricValue::from(0i64).into());

    let mut node_control_rebirth = Metric::new();
    node_control_rebirth
        .set_name(NODE_CONTROL_REBIRTH.into())
        .set_datatype(DataType::Boolean)
        .set_timestamp(timestamp())
        .set_value(MetricValue::from(true).into());

    let mut payload_metrics = vec![bdseq, node_control_rebirth];
    if let Some(metrics) = metrics {
        payload_metrics.extend(metrics);
    }

    Payload {
        timestamp: Some(timestamp()),
        metrics: payload_metrics,
        seq: Some(0),
        uuid: None,
        body: None,
    }
}

pub fn new_data_payload(seq: u8, metrics: Vec<Metric>) -> Payload {
    Payload {
        timestamp: Some(timestamp()),
        metrics,
        seq: Some(seq as u64),
        uuid: None,
        body: None,
    }
}

pub fn assert_payload_is_rebirth_request(payload: &Payload) {
    let mut valid = false;
    for x in &payload.metrics {
        let other = Some(NODE_CONTROL_REBIRTH.to_string());
        if !x.name.eq(&other) {
            continue;
        }
        let other = Some(metric::Value::BooleanValue(true));
        assert!(x.value.eq(&other));
        valid = true;
    }
    assert!(valid);
}
