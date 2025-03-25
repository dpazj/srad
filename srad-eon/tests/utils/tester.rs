use std::time::Duration;

use srad_types::{constants::{BDSEQ, NODE_CONTROL_REBIRTH}, payload::{metric, DataType, Payload}, topic::{DeviceMessage, DeviceTopic, NodeMessage, NodeTopic, QoS, StateTopic, Topic, TopicFilter} };
use tokio::time::timeout;

use crate::utils::client::OutboundMessage;

use super::client::Broker;

pub fn verify_nbirth_payload(payload: Payload, expected_bdseq: u64)
{

  /* [tck-id-topics-nbirth-seq-num] The NBIRTH MUST include a sequence number in the payload and it MUST have a value of 0. */
  assert_eq!(payload.seq, Some(0));
  assert_ne!(payload.timestamp, None);

  let mut contains_node_control = false;
  let mut contains_bdseq= false;
  for metric in payload.metrics {
    assert_ne!(metric.datatype, None);

    let metric_name = match &metric.name {
        Some(name) => name,
        None => panic!("Metric name is required in birth payload"),
    };

    if let Some(_) = &metric.value  {
      assert_eq!(metric.is_null, None)    
    }

    if let Some(is_null) = &metric.is_null {
      if *is_null == true { assert_eq!(metric.value, None) }
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
      assert_eq!(metric.value, Some(metric::Value::LongValue(expected_bdseq)));
    }
  }
  assert!(contains_node_control);
  assert!(contains_bdseq);
}


pub fn verify_dbirth_payload(payload: Payload, expected_seq: u64)
{
  assert_eq!(payload.seq, Some(expected_seq));
  assert_ne!(payload.timestamp, None);
}

pub async fn test_node_online(broker: &mut Broker, group_id: &str, node_id: &str, expected_bdseq: u64)
{
  broker.tx_event.send(Some(srad_client::Event::Online)).unwrap();
  let subscription = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();
  let filters = match subscription {
    OutboundMessage::Subscribe(filters) => filters,
    message => panic!("got {message:?}")
  };

  let expected_filters = vec![
    TopicFilter::new_with_qos(Topic::NodeTopic(NodeTopic::new(group_id, NodeMessage::NCmd, node_id)), QoS::AtLeastOnce),
    TopicFilter::new_with_qos(Topic::DeviceTopic(DeviceTopic::new(group_id, DeviceMessage::DCmd, node_id, "+")), QoS::AtLeastOnce),
    TopicFilter::new_with_qos(Topic::State(StateTopic::new()), QoS::AtLeastOnce)
  ];

  assert_eq!(filters.len(), expected_filters.len());
  for x in expected_filters {
    assert!(filters.contains(&x), "Sub filters did not contain expected filter: {x:?}")
  }

  let birth= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();

  let (topic, payload) = match birth {
    OutboundMessage::NodeMessage { topic, payload }=> (topic, payload),
    _ => panic!()
  };

  assert_eq!(topic, NodeTopic::new(group_id, NodeMessage::NBirth, node_id));
  verify_nbirth_payload(payload, expected_bdseq);
}

pub async fn verify_device_birth(broker: &mut Broker, group_id: &str, node_id: &str, device_name: &str, expected_seq: u64) 
{
  let device_birth= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();

  let (topic, payload) = match device_birth {
    OutboundMessage::DeviceMessage{ topic, payload } => (topic, payload),
    _ => panic!()
  };
  assert_eq!(topic, DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device_name));
  verify_dbirth_payload(payload, expected_seq);
}
