mod utils;

use std::time::Duration;

use srad_client::NodeMessage;
use srad_eon::{EoN, EoNBuilder, NoMetricManager};
use srad_types::{constants::NODE_CONTROL_REBIRTH, payload::{Metric, Payload}, topic::{DeviceMessage, DeviceTopic, NodeTopic}};
use tokio::time::timeout;
use utils::{client::{EventLoop, OutboundMessage}, tester::{test_node_online, verify_dbirth_payload, verify_device_birth, verify_nbirth_payload}};


#[tokio::test]
async fn node_session_establishment() {
  let group_id = "foo";
  let node_id = "bar";

  let (channel_eventloop, client, mut broker) = EventLoop::new();
  let builder= EoNBuilder::new(channel_eventloop, client)
    .with_group_id(group_id)
    .with_node_id(node_id);
  let (mut eventloop, _)  = EoN::new_from_builder(builder).unwrap();
  tokio::spawn(async move {
    eventloop.run().await
  });

  test_node_online(&mut broker, &group_id, &node_id, 0).await;
  broker.tx_event.send(Some(srad_client::Event::Offline)).unwrap();

  let last_will = broker.last_will();
  last_will.unwrap();

  test_node_online(&mut broker, &group_id, &node_id, 1).await;
}

#[tokio::test]
async fn device_session_establishment() {
  let group_id = "foo";
  let node_id = "bar";
  let device1_name = "device1";
  let device2_name = "device2";

  let (channel_eventloop, client, mut broker) = EventLoop::new();
  let builder= EoNBuilder::new(channel_eventloop, client)
    .with_group_id(group_id)
    .with_node_id(node_id);
  let (mut eventloop, handle)  = EoN::new_from_builder(builder).unwrap();

  tokio::spawn(async move {
    eventloop.run().await
  });
  /* Add device before node is online */
  handle.register_device(device1_name, NoMetricManager::new()).await.unwrap();

  test_node_online(&mut broker, &group_id, &node_id, 0).await;

  let device_birth= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();

  let (topic, payload) = match device_birth {
    OutboundMessage::DeviceMessage{ topic, payload } => (topic, payload),
    _ => panic!()
  };
  assert_eq!(topic, DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device1_name));
  verify_dbirth_payload(payload, 1);

  broker.tx_event.send(Some(srad_client::Event::Offline)).unwrap();

  let last_will = broker.last_will();
  last_will.unwrap();

  test_node_online(&mut broker, &group_id, &node_id, 1).await;

  let device_birth= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();

  let (topic, payload) = match device_birth {
    OutboundMessage::DeviceMessage{ topic, payload } => (topic, payload),
    _ => panic!()
  };
  assert_eq!(topic, DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device1_name));
  verify_dbirth_payload(payload, 1);

  /* Add device while node is online */
  handle.register_device(device2_name, NoMetricManager::new()).await.unwrap();
  verify_device_birth(&mut broker, &group_id, &node_id, &device2_name, 2).await;
}

fn create_rebirth_message(group_id: &str, node_id: &str) -> NodeMessage {
  let mut metric = Metric::new();
  metric.set_name(NODE_CONTROL_REBIRTH.into()).set_value(srad_types::payload::metric::Value::BooleanValue(true));

  NodeMessage {
    group_id: group_id.to_string(), 
    node_id: node_id.to_string(), 
    message: srad_client::Message::Cmd { payload: Payload { timestamp: Some(0), metrics: vec![metric], seq: None, uuid: None, body: None }
  }}
}

#[tokio::test]
async fn rebirth() {
  let group_id = "foo";
  let node_id = "bar";
  let device1_name = "dev1";
  let device2_name = "dev2";

  let (channel_eventloop, client, mut broker) = EventLoop::new();
  let builder= EoNBuilder::new(channel_eventloop, client)
    .with_group_id(group_id)
    .with_node_id(node_id);
  let (mut eventloop, handle)  = EoN::new_from_builder(builder).unwrap();
  tokio::spawn(async move {
    eventloop.run().await
  });
  test_node_online(&mut broker, &group_id, &node_id, 0).await;

  broker.tx_event.send(Some(srad_client::Event::Node(create_rebirth_message(&group_id, &node_id)))).unwrap();

  let node_rebirth = timeout(Duration::from_secs(1), broker.rx_outbound.recv()).await.unwrap().unwrap();
  let (topic, payload) = match node_rebirth {
    OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
    _ => panic!()
  };
  assert_eq!(topic, NodeTopic::new(group_id, srad_types::topic::NodeMessage::NBirth, node_id));
  verify_nbirth_payload(payload, 0);

  // rebirth with multiple devices
  handle.register_device(device1_name, NoMetricManager::new()).await.unwrap();
  verify_device_birth(&mut broker, &group_id, &node_id, &device1_name, 1).await;
  handle.register_device(device2_name, NoMetricManager::new()).await.unwrap();
  verify_device_birth(&mut broker, &group_id, &node_id, &device2_name, 2).await;

  broker.tx_event.send(Some(srad_client::Event::Node(create_rebirth_message(&group_id, &node_id)))).unwrap();
  let node_rebirth = timeout(Duration::from_secs(1), broker.rx_outbound.recv()).await.unwrap().unwrap();
  let (topic, payload) = match node_rebirth {
    OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
    _ => panic!()
  };
  assert_eq!(topic, NodeTopic::new(group_id, srad_types::topic::NodeMessage::NBirth, node_id));
  verify_nbirth_payload(payload, 0);

  let mut dev_rebirths = vec![];
  dev_rebirths.push(timeout(Duration::from_secs(1), broker.rx_outbound.recv()).await.unwrap().unwrap());
  dev_rebirths.push(timeout(Duration::from_secs(1), broker.rx_outbound.recv()).await.unwrap().unwrap());

  let mut dev_rebirth_topics = vec![];
  let mut expected_seq = 1;
  for x in dev_rebirths {
    let (topic, payload) = match x {
      OutboundMessage::DeviceMessage{ topic, payload } => (topic, payload),
      _ => panic!()
    };
    dev_rebirth_topics.push(topic);
    verify_dbirth_payload(payload, expected_seq);
    expected_seq += 1;
  }

  let expected_dev_rebirth_topics = vec! [
    DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device1_name),
    DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device2_name),
  ];

  for expected in expected_dev_rebirth_topics {
    assert!(dev_rebirth_topics.contains(&expected))
  }

}
