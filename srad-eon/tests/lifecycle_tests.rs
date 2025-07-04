mod utils;

use std::time::Duration;

use srad_client::{
    channel::{ChannelEventLoop, OutboundMessage},
    LastWill, NodeMessage,
};
use srad_eon::{EoNBuilder, NoMetricManager};
use srad_types::{
    constants::NODE_CONTROL_REBIRTH,
    payload::{Metric, Payload},
    topic::{DeviceMessage, DeviceTopic, NodeTopic},
};
use tokio::time::timeout;
use utils::tester::{
    create_test_ndeath_payload, test_graceful_shutdown, test_node_online, verify_dbirth_payload,
    verify_device_birth, verify_nbirth_payload,
};

use crate::utils::tester::{verify_device_death, verify_node_birth};

#[tokio::test]
async fn node_session() {
    let group_id = "foo";
    let node_id = "bar";

    let (channel_eventloop, client, mut broker) = ChannelEventLoop::new();
    let (eventloop, node) = EoNBuilder::new(channel_eventloop, client)
        .with_group_id(group_id)
        .with_node_id(node_id)
        .build()
        .unwrap();

    tokio::spawn(async move { eventloop.run().await });

    // Node goes online
    test_node_online(&mut broker, group_id, node_id, 0).await;

    // Node goes offline
    broker.tx_event.send(srad_client::Event::Offline).unwrap();
    let expected_last_will = LastWill::new_node(group_id, node_id, create_test_ndeath_payload(0));
    let last_will = broker.last_will().unwrap();
    assert_eq!(expected_last_will, last_will);

    // Node goes online again
    test_node_online(&mut broker, group_id, node_id, 1).await;

    test_graceful_shutdown(&mut broker, &node, group_id, node_id, 1).await;
}

#[tokio::test]
async fn device_session() {
    let group_id = "foo";
    let node_id = "bar";
    let device1_name = "device1";
    let device2_name = "device2";

    let (channel_eventloop, client, mut broker) = ChannelEventLoop::new();
    let (eventloop, handle) = EoNBuilder::new(channel_eventloop, client)
        .with_group_id(group_id)
        .with_node_id(node_id)
        .build()
        .unwrap();

    tokio::spawn(async move { eventloop.run().await });

    /* Add device and enable before node is online */
    handle
        .register_device(device1_name, NoMetricManager::new())
        .unwrap()
        .enable();

    test_node_online(&mut broker, group_id, node_id, 0).await;
    verify_device_birth(&mut broker, group_id, node_id, device1_name, 1).await;

    /* Test node offline and then online */
    broker.tx_event.send(srad_client::Event::Offline).unwrap();
    let last_will = broker.last_will();
    last_will.unwrap();
    test_node_online(&mut broker, group_id, node_id, 1).await;
    verify_device_birth(&mut broker, group_id, node_id, device1_name, 1).await;

    /* Node rebirth */
    handle.rebirth();
    verify_node_birth(&mut broker, group_id, node_id, 1).await;
    verify_device_birth(&mut broker, group_id, node_id, device1_name, 1).await;

    /* Add device while node is online */
    let dev = handle
        .register_device(device2_name, NoMetricManager::new())
        .unwrap();
    dev.enable();
    verify_device_birth(&mut broker, group_id, node_id, device2_name, 2).await;

    /* Disable device */
    dev.disable();
    verify_device_death(&mut broker, group_id, node_id, device2_name, 3).await;

    /* Enable device */
    dev.enable();
    verify_device_birth(&mut broker, group_id, node_id, device2_name, 4).await;

    /* Manual device rebirth */
    dev.rebirth();
    verify_device_birth(&mut broker, group_id, node_id, device2_name, 5).await;

    /* Remove device */
    handle.unregister_device(dev).await;
    verify_device_death(&mut broker, group_id, node_id, device2_name, 6).await;

    test_graceful_shutdown(&mut broker, &handle, group_id, node_id, 1).await;
}

fn create_rebirth_message(group_id: &str, node_id: &str) -> NodeMessage {
    let mut metric = Metric::new();
    metric
        .set_name(NODE_CONTROL_REBIRTH.into())
        .set_value(srad_types::payload::metric::Value::BooleanValue(true));

    NodeMessage {
        group_id: group_id.to_string(),
        node_id: node_id.to_string(),
        message: srad_client::Message {
            payload: Payload {
                timestamp: Some(0),
                metrics: vec![metric],
                seq: None,
                uuid: None,
                body: None,
            },
            kind: srad_client::MessageKind::Cmd,
        },
    }
}

#[tokio::test]
async fn rebirth() {
    let group_id = "foo";
    let node_id = "bar";
    let device1_name = "dev1";
    let device2_name = "dev2";

    let (channel_eventloop, client, mut broker) = ChannelEventLoop::new();
    let (eventloop, handle) = EoNBuilder::new(channel_eventloop, client)
        .with_group_id(group_id)
        .with_node_id(node_id)
        .with_rebirth_cmd_cooldown(Duration::from_millis(0))
        .build()
        .unwrap();

    tokio::spawn(async move { eventloop.run().await });
    test_node_online(&mut broker, group_id, node_id, 0).await;

    broker
        .tx_event
        .send(srad_client::Event::Node(create_rebirth_message(
            group_id, node_id,
        )))
        .unwrap();

    let node_rebirth = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    let (topic, payload) = match node_rebirth {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        _ => panic!(),
    };
    assert_eq!(
        topic,
        NodeTopic::new(group_id, srad_types::topic::NodeMessage::NBirth, node_id)
    );
    verify_nbirth_payload(payload, 0);

    // rebirth with multiple devices
    handle
        .register_device(device1_name, NoMetricManager::new())
        .unwrap()
        .enable();
    verify_device_birth(&mut broker, group_id, node_id, device1_name, 1).await;
    handle
        .register_device(device2_name, NoMetricManager::new())
        .unwrap()
        .enable();
    verify_device_birth(&mut broker, group_id, node_id, device2_name, 2).await;

    broker
        .tx_event
        .send(srad_client::Event::Node(create_rebirth_message(
            group_id, node_id,
        )))
        .unwrap();
    let node_rebirth = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    let (topic, payload) = match node_rebirth {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        _ => panic!(),
    };
    assert_eq!(
        topic,
        NodeTopic::new(group_id, srad_types::topic::NodeMessage::NBirth, node_id)
    );
    verify_nbirth_payload(payload, 0);

    let mut dev_rebirths = vec![];
    dev_rebirths.push(
        timeout(Duration::from_secs(1), broker.rx_outbound.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    dev_rebirths.push(
        timeout(Duration::from_secs(1), broker.rx_outbound.recv())
            .await
            .unwrap()
            .unwrap(),
    );

    let mut dev_rebirth_topics = vec![];
    let mut expected_seq = 1;
    for x in dev_rebirths {
        let (topic, payload) = match x {
            OutboundMessage::DeviceMessage { topic, payload } => (topic, payload),
            _ => panic!(),
        };
        dev_rebirth_topics.push(topic);
        verify_dbirth_payload(payload, expected_seq);
        expected_seq += 1;
    }

    let expected_dev_rebirth_topics = vec![
        DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device1_name),
        DeviceTopic::new(group_id, DeviceMessage::DBirth, node_id, device2_name),
    ];

    for expected in expected_dev_rebirth_topics {
        assert!(dev_rebirth_topics.contains(&expected))
    }
}
