use std::time::Duration;

use srad_eon::NodeHandle;
use srad_types::{
    constants::{BDSEQ, NODE_CONTROL_REBIRTH},
    payload::{metric, DataType, Metric, Payload},
    topic::{
        DeviceMessage, DeviceTopic, NodeMessage, NodeTopic, QoS, StateTopic, Topic, TopicFilter,
    },
    MetricValue,
};
use tokio::time::timeout;

use srad_client::channel::{ChannelBroker, OutboundMessage};

pub fn create_test_ndeath_payload(expected_bdseq: i64) -> Payload {
    let mut metric = Metric::new();
    metric
        .set_name(BDSEQ.to_string())
        .set_value(MetricValue::from(expected_bdseq).into());
    Payload {
        timestamp: None,
        metrics: vec![metric],
        seq: None,
        uuid: None,
        body: None,
    }
}

pub fn test_ddeath_payload(payload: Payload, expected_seq: u64) {
    assert_ne!(payload.timestamp, None);
    assert_eq!(payload.seq, Some(expected_seq));
    assert_eq!(payload.metrics.len(), 0);
    assert_eq!(payload.body, None);
    assert_eq!(payload.uuid, None);
}

pub fn verify_nbirth_payload(payload: Payload, expected_bdseq: i64) {
    /* [tck-id-topics-nbirth-seq-num] The NBIRTH MUST include a sequence number in the payload and it MUST have a value of 0. */
    assert_eq!(payload.seq, Some(0));
    assert_ne!(payload.timestamp, None);

    let mut contains_node_control = false;
    let mut contains_bdseq = false;
    for metric in payload.metrics {
        assert_ne!(metric.datatype, None);

        let metric_name = match &metric.name {
            Some(name) => name,
            None => panic!("Metric name is required in birth payload"),
        };

        if metric.value.is_some() {
            assert_eq!(metric.is_null, None)
        }

        if let Some(is_null) = &metric.is_null {
            if *is_null {
                assert_eq!(metric.value, None)
            }
        }

        if metric_name.eq(NODE_CONTROL_REBIRTH) {
            contains_node_control = true;
            assert_eq!(metric.alias, None);
            assert_eq!(metric.datatype, Some(DataType::Boolean as u32));
            assert_eq!(metric.value, Some(metric::Value::BooleanValue(false)));
        }

        if metric_name.eq(BDSEQ) {
            contains_bdseq = true;
            assert_eq!(metric.alias, None);
            assert_eq!(metric.datatype, Some(DataType::Int64 as u32));
            assert_eq!(metric.value, Some(MetricValue::from(expected_bdseq).into()));
        }
    }
    assert!(contains_node_control);
    assert!(contains_bdseq);
}

pub fn verify_dbirth_payload(payload: Payload, expected_seq: u64) {
    assert_eq!(payload.seq, Some(expected_seq));
    assert_ne!(payload.timestamp, None);
}

pub async fn test_node_online(
    broker: &mut ChannelBroker,
    group_id: &str,
    node_id: &str,
    expected_bdseq: i64,
) {
    broker.tx_event.send(srad_client::Event::Online).unwrap();
    let subscription = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    let filters = match subscription {
        OutboundMessage::Subscribe(filters) => filters,
        message => panic!("got {message:?}"),
    };

    let expected_filters = vec![
        TopicFilter::new_with_qos(
            Topic::NodeTopic(NodeTopic::new(group_id, NodeMessage::NCmd, node_id)),
            QoS::AtLeastOnce,
        ),
        TopicFilter::new_with_qos(
            Topic::DeviceTopic(DeviceTopic::new(
                group_id,
                DeviceMessage::DCmd,
                node_id,
                "+",
            )),
            QoS::AtLeastOnce,
        ),
        TopicFilter::new_with_qos(Topic::State(StateTopic::new()), QoS::AtLeastOnce),
    ];

    assert_eq!(filters.len(), expected_filters.len());
    for x in expected_filters {
        assert!(
            filters.contains(&x),
            "Sub filters did not contain expected filter: {x:?}"
        )
    }

    let birth = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();

    let (topic, payload) = match birth {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        _ => panic!(),
    };

    assert_eq!(
        topic,
        NodeTopic::new(group_id, NodeMessage::NBirth, node_id)
    );
    verify_nbirth_payload(payload, expected_bdseq);
}

pub async fn verify_device_birth(
    broker: &mut ChannelBroker,
    group_id: &str,
    node_id: &str,
    device_name: &str,
    expected_seq: u64,
) {
    let device_birth = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();

    let (topic, payload) = match device_birth {
        OutboundMessage::DeviceMessage { topic, payload } => (topic, payload),
        _ => panic!(),
    };
    assert_eq!(
        topic,
        DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device_name)
    );
    verify_dbirth_payload(payload, expected_seq);
}

// Test graceful shutdown using handle.cancel()
pub async fn test_graceful_shutdown(
    broker: &mut ChannelBroker,
    handle: &NodeHandle,
    group_id: &str,
    node_id: &str,
    expected_bdseq: i64,
) {
    handle.cancel().await;
    let node_death = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();

    let (topic, payload) = match node_death {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        _ => panic!(),
    };
    assert_eq!(
        topic,
        NodeTopic::new(group_id, srad_types::topic::NodeMessage::NDeath, node_id)
    );
    assert_eq!(payload, create_test_ndeath_payload(expected_bdseq));

    let disconnect = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(disconnect, OutboundMessage::Disconnect);
}
