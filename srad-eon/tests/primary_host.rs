// The shared helper module contains helpers used by other integration tests.
#[allow(dead_code)]
mod utils;

use std::time::Duration;

use srad_client::{
    channel::{ChannelBroker, ChannelEventLoop, OutboundMessage},
    Event, StatePayload,
};
use srad_eon::{EoNBuilder, NoMetricManager};
use srad_types::topic::{
    DeviceMessage, DeviceTopic, NodeMessage, NodeTopic, QoS, StateTopic, Topic, TopicFilter,
};
use tokio::time::{sleep, timeout, Instant};
use utils::tester::{create_test_ndeath_payload, verify_device_birth, verify_node_birth};

const GROUP_ID: &str = "foo";
const NODE_ID: &str = "bar";
const HOST_ID: &str = "host1";
const DEVICE_ID: &str = "dev1";

async fn expect_subscription(broker: &mut ChannelBroker, state_topic: StateTopic) {
    let message = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .expect("timed out waiting for the node subscription")
        .expect("the node channel closed");

    let filters = match message {
        OutboundMessage::Subscribe(filters) => filters,
        message => panic!("expected a subscription, got {message:?}"),
    };

    assert_eq!(filters.len(), 3);
    assert!(
        filters.contains(&TopicFilter::new_with_qos(
            Topic::State(state_topic),
            QoS::AtLeastOnce,
        )),
        "the node did not subscribe to the configured state topic: {filters:?}"
    );
    assert!(filters.contains(&TopicFilter::new_with_qos(
        Topic::NodeTopic(NodeTopic::new(GROUP_ID, NodeMessage::NCmd, NODE_ID)),
        QoS::AtLeastOnce,
    )));
    assert!(filters.contains(&TopicFilter::new_with_qos(
        Topic::DeviceTopic(DeviceTopic::new(
            GROUP_ID,
            DeviceMessage::DCmd,
            NODE_ID,
            "+",
        )),
        QoS::AtLeastOnce,
    )));
}

async fn expect_no_message(broker: &mut ChannelBroker) {
    assert!(
        timeout(Duration::from_millis(200), broker.rx_outbound.recv())
            .await
            .is_err(),
        "the node published a message when it should have been waiting"
    );
}

fn start(primary_host_id: Option<&str>) -> (ChannelBroker, srad_eon::NodeHandle) {
    let (channel_eventloop, client, broker) = ChannelEventLoop::new();
    let mut builder = EoNBuilder::new(channel_eventloop, client)
        .with_group_id(GROUP_ID)
        .with_node_id(NODE_ID);
    if let Some(host_id) = primary_host_id {
        builder = builder.with_primary_host_id(host_id);
    }
    let (eventloop, handle) = builder.build().unwrap();
    tokio::spawn(async move { eventloop.run().await });
    (broker, handle)
}

#[tokio::test]
async fn waits_for_primary_host_online() {
    let (mut broker, handle) = start(Some(HOST_ID));
    handle
        .register_device(DEVICE_ID, NoMetricManager::new())
        .unwrap()
        .enable();

    broker.tx_event.send(Event::Online).unwrap();
    expect_subscription(&mut broker, StateTopic::new_host(HOST_ID)).await;

    // Connecting is not enough: the node waits for the primary host.
    expect_no_message(&mut broker).await;

    // A STATE for another host does not count.
    broker
        .tx_event
        .send(Event::State {
            host_id: "other".into(),
            payload: StatePayload::Online { timestamp: 1 },
        })
        .unwrap();
    expect_no_message(&mut broker).await;

    broker
        .tx_event
        .send(Event::State {
            host_id: HOST_ID.into(),
            payload: StatePayload::Online { timestamp: 10 },
        })
        .unwrap();

    verify_node_birth(&mut broker, GROUP_ID, NODE_ID, 0).await;
    verify_device_birth(&mut broker, GROUP_ID, NODE_ID, DEVICE_ID, 1).await;
}

#[tokio::test]
async fn primary_host_offline_terminates_session_and_rebirths() {
    let (mut broker, handle) = start(Some(HOST_ID));
    handle
        .register_device(DEVICE_ID, NoMetricManager::new())
        .unwrap()
        .enable();

    broker.tx_event.send(Event::Online).unwrap();
    expect_subscription(&mut broker, StateTopic::new_host(HOST_ID)).await;

    broker
        .tx_event
        .send(Event::State {
            host_id: HOST_ID.into(),
            payload: StatePayload::Online { timestamp: 10 },
        })
        .unwrap();
    verify_node_birth(&mut broker, GROUP_ID, NODE_ID, 0).await;
    verify_device_birth(&mut broker, GROUP_ID, NODE_ID, DEVICE_ID, 1).await;

    // An OFFLINE with an older timestamp must be ignored.
    broker
        .tx_event
        .send(Event::State {
            host_id: HOST_ID.into(),
            payload: StatePayload::Offline { timestamp: 5 },
        })
        .unwrap();
    expect_no_message(&mut broker).await;

    // A valid OFFLINE terminates the session: NDEATH then disconnect.
    broker
        .tx_event
        .send(Event::State {
            host_id: HOST_ID.into(),
            payload: StatePayload::Offline { timestamp: 10 },
        })
        .unwrap();

    let death = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    let (topic, payload) = match death {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        message => panic!("expected an NDEATH message, got {message:?}"),
    };
    assert_eq!(
        topic,
        NodeTopic::new(GROUP_ID, NodeMessage::NDeath, NODE_ID)
    );
    assert_eq!(payload, create_test_ndeath_payload(0));

    let disconnect = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(disconnect, OutboundMessage::Disconnect);

    // Simulate the client reporting the connection loss and reconnecting.
    broker.tx_event.send(Event::Offline).unwrap();
    wait_for_last_will(&broker, 1).await;

    broker.tx_event.send(Event::Online).unwrap();
    expect_subscription(&mut broker, StateTopic::new_host(HOST_ID)).await;

    // An ONLINE with an older timestamp must not restart the session.
    broker
        .tx_event
        .send(Event::State {
            host_id: HOST_ID.into(),
            payload: StatePayload::Online { timestamp: 5 },
        })
        .unwrap();
    expect_no_message(&mut broker).await;

    broker
        .tx_event
        .send(Event::State {
            host_id: HOST_ID.into(),
            payload: StatePayload::Online { timestamp: 20 },
        })
        .unwrap();
    verify_node_birth(&mut broker, GROUP_ID, NODE_ID, 1).await;
    verify_device_birth(&mut broker, GROUP_ID, NODE_ID, DEVICE_ID, 1).await;
}

async fn wait_for_last_will(broker: &ChannelBroker, expected_bdseq: i64) {
    let expected = Vec::<u8>::from(create_test_ndeath_payload(expected_bdseq));
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(will) = broker.last_will() {
            if will.payload == expected {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the last will was not updated for the new session"
        );
        sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn rejects_invalid_primary_host_id() {
    let (channel_eventloop, client, _broker) = ChannelEventLoop::new();
    let result = EoNBuilder::new(channel_eventloop, client)
        .with_group_id(GROUP_ID)
        .with_node_id(NODE_ID)
        .with_primary_host_id("invalid/host")
        .build();

    let Err(error) = result else {
        panic!("an invalid primary host id must be rejected");
    };
    assert!(
        error.contains("cannot contain"),
        "unexpected error: {error}"
    );
}
