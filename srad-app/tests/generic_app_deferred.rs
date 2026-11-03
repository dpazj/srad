use std::time::Duration;

use srad_app::{
    generic_app::{ApplicationBuilder, MetricStore, StateUpdateError},
    MetricBirthDetails, MetricDetails, SubscriptionConfig,
};
use srad_client::{
    channel::ChannelEventLoop, Bytes, DeviceMessage, Message, MessageKind, MessagePayload,
    NodeMessage,
};
use srad_types::{
    payload::{DataType, Message as ProstMessage, Metric},
    utils::timestamp,
    MetricId, MetricValue,
};
use tokio::{sync::mpsc, time::timeout};

#[allow(dead_code)]
mod utils;

struct RecordingStore {
    tx: mpsc::UnboundedSender<&'static str>,
}

impl MetricStore for RecordingStore {
    fn set_stale(&mut self) {
        let _ = self.tx.send("stale");
    }

    fn update_from_birth(
        &mut self,
        _details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) -> Result<(), StateUpdateError> {
        let _ = self.tx.send("birth");
        Ok(())
    }

    fn update_from_data(
        &mut self,
        _details: Vec<(MetricId, MetricDetails)>,
    ) -> Result<(), StateUpdateError> {
        let _ = self.tx.send("data");
        Ok(())
    }
}

fn encoded(payload: srad_types::payload::Payload) -> MessagePayload {
    MessagePayload::Encoded(Bytes::from(payload.encode_to_vec()))
}

fn device_birth_payload(seq: u8) -> srad_types::payload::Payload {
    let mut metric = Metric::new();
    metric
        .set_name("A".into())
        .set_datatype(DataType::Int32)
        .set_value(MetricValue::from(1i32).into())
        .set_timestamp(timestamp());

    srad_types::payload::Payload {
        timestamp: Some(timestamp()),
        metrics: vec![metric],
        seq: Some(seq as u64),
        uuid: None,
        body: None,
    }
}

fn device_death_payload(seq: u8) -> srad_types::payload::Payload {
    srad_types::payload::Payload {
        timestamp: Some(timestamp()),
        metrics: vec![],
        seq: Some(seq as u64),
        uuid: None,
        body: None,
    }
}

#[tokio::test]
async fn decodes_payload_in_node_task() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let (store_tx, mut store_rx) = mpsc::unbounded_channel();

    let (application, _) = ApplicationBuilder::new(
        "app",
        eventloop,
        client,
        SubscriptionConfig::SingleGroup {
            group_id: "group1".into(),
        },
    )
    .on_node_created(move |node| {
        node.register_metric_store(RecordingStore {
            tx: store_tx.clone(),
        })
    })
    .build();

    tokio::spawn(async move {
        let _ = application.run().await;
    });

    broker.tx_event.send(srad_client::Event::Online).unwrap();
    for _ in 0..2 {
        timeout(Duration::from_secs(1), broker.rx_outbound.recv())
            .await
            .unwrap()
            .unwrap();
    }

    let birth = utils::payloads::new_nbirth_payload(None);
    broker
        .tx_event
        .send(srad_client::Event::Node(NodeMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            message: Message {
                payload: encoded(birth),
                kind: MessageKind::Birth,
            },
        }))
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), store_rx.recv())
            .await
            .unwrap(),
        Some("birth")
    );

    let mut metric = Metric::new();
    metric
        .set_name("A".into())
        .set_datatype(DataType::Int32)
        .set_value(MetricValue::from(1i32).into())
        .set_timestamp(timestamp());

    broker
        .tx_event
        .send(srad_client::Event::Node(NodeMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            message: Message {
                payload: encoded(utils::payloads::new_data_payload(1, vec![metric])),
                kind: MessageKind::Data,
            },
        }))
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), store_rx.recv())
            .await
            .unwrap(),
        Some("data")
    );
}

#[tokio::test]
async fn decodes_device_payloads_in_node_task() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let (node_store_tx, mut node_store_rx) = mpsc::unbounded_channel();
    let (device_store_tx, mut device_store_rx) = mpsc::unbounded_channel();

    let (application, _) = ApplicationBuilder::new(
        "app",
        eventloop,
        client,
        SubscriptionConfig::SingleGroup {
            group_id: "group1".into(),
        },
    )
    .on_node_created(move |node| {
        node.register_metric_store(RecordingStore {
            tx: node_store_tx.clone(),
        });

        let device_store_tx = device_store_tx.clone();
        node.on_device_created(move |device| {
            device.register_metric_store(RecordingStore {
                tx: device_store_tx.clone(),
            });
        });
    })
    .build();

    tokio::spawn(async move {
        let _ = application.run().await;
    });

    broker.tx_event.send(srad_client::Event::Online).unwrap();
    for _ in 0..2 {
        timeout(Duration::from_secs(1), broker.rx_outbound.recv())
            .await
            .unwrap()
            .unwrap();
    }

    broker
        .tx_event
        .send(srad_client::Event::Node(NodeMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            message: Message {
                payload: encoded(utils::payloads::new_nbirth_payload(None)),
                kind: MessageKind::Birth,
            },
        }))
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), node_store_rx.recv())
            .await
            .unwrap(),
        Some("birth")
    );

    broker
        .tx_event
        .send(srad_client::Event::Device(DeviceMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            device_id: "device1".into(),
            message: Message {
                payload: encoded(device_birth_payload(1)),
                kind: MessageKind::Birth,
            },
        }))
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), device_store_rx.recv())
            .await
            .unwrap(),
        Some("birth")
    );

    let mut metric = Metric::new();
    metric
        .set_name("A".into())
        .set_datatype(DataType::Int32)
        .set_value(MetricValue::from(2i32).into())
        .set_timestamp(timestamp());

    broker
        .tx_event
        .send(srad_client::Event::Device(DeviceMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            device_id: "device1".into(),
            message: Message {
                payload: encoded(utils::payloads::new_data_payload(2, vec![metric])),
                kind: MessageKind::Data,
            },
        }))
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), device_store_rx.recv())
            .await
            .unwrap(),
        Some("data")
    );

    broker
        .tx_event
        .send(srad_client::Event::Device(DeviceMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            device_id: "device1".into(),
            message: Message {
                payload: encoded(device_death_payload(3)),
                kind: MessageKind::Death,
            },
        }))
        .unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), device_store_rx.recv())
            .await
            .unwrap(),
        Some("stale")
    );
}
