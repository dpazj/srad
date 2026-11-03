use std::time::Duration;

use srad_app::{
    generic_app::{ApplicationBuilder, MetricStore, StateUpdateError},
    MetricBirthDetails, MetricDetails, SubscriptionConfig,
};
use srad_client::{
    channel::ChannelEventLoop, Bytes, Message, MessageKind, MessagePayload, NodeMessage,
};
use srad_types::{
    payload::{DataType, Message as ProstMessage, Metric},
    utils::timestamp,
    DataSet, DataSetValue, MetricId, MetricValue, MetricValueKind,
};
use tokio::{sync::mpsc, time::timeout};

#[allow(dead_code)]
mod utils;

struct DataSetStore {
    tx: mpsc::UnboundedSender<Option<MetricValue>>,
}

impl MetricStore for DataSetStore {
    fn set_stale(&mut self) {}

    fn update_from_birth(
        &mut self,
        details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) -> Result<(), StateUpdateError> {
        let (_, details) = details
            .into_iter()
            .find(|(birth, _)| birth.name == "dataset")
            .expect("dataset metric missing from birth");
        self.tx.send(details.value).unwrap();
        Ok(())
    }

    fn update_from_data(
        &mut self,
        details: Vec<(MetricId, MetricDetails)>,
    ) -> Result<(), StateUpdateError> {
        let (_, details) = details
            .into_iter()
            .find(|(id, _)| id == &MetricId::Name("dataset".into()))
            .expect("dataset metric missing from data");
        self.tx.send(details.value).unwrap();
        Ok(())
    }
}

fn encoded(payload: srad_types::payload::Payload) -> MessagePayload {
    MessagePayload::Encoded(Bytes::from(payload.encode_to_vec()))
}

fn data_set_metric(first: u64, second: f64) -> Metric {
    let mut data_set = DataSet::new();
    data_set
        .init(["Time", "Value"], [DataType::UInt64, DataType::Double])
        .add_row_values([DataSetValue::from(first), DataSetValue::from(second)]);

    let mut metric = Metric::new();
    metric
        .set_name("dataset".into())
        .set_datatype(DataType::DataSet)
        .set_timestamp(timestamp())
        .set_value(MetricValue::from(data_set).into());
    metric
}

fn assert_is_data_set(value: Option<MetricValue>, expected: (u64, f64)) {
    let kind = MetricValueKind::try_from_metric_value(DataType::DataSet, value.unwrap()).unwrap();
    match kind {
        MetricValueKind::DataSet(data_set) => {
            assert_eq!(data_set.columns().to_vec(), vec!["Time", "Value"]);
            assert_eq!(
                data_set.types(),
                [DataType::UInt64 as u32, DataType::Double as u32]
            );
            assert_eq!(data_set.rows().len(), 1);
            assert_eq!(
                data_set.rows()[0].elements[0].value,
                Some(DataSetValue::from(expected.0).into())
            );
            assert_eq!(
                data_set.rows()[0].elements[1].value,
                Some(DataSetValue::from(expected.1).into())
            );
        }
        other => panic!("expected a DataSet metric value, got {other:?}"),
    }
}

#[tokio::test]
async fn decodes_data_set_metrics_in_birth_and_data() {
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
        node.register_metric_store(DataSetStore {
            tx: store_tx.clone(),
        })
    })
    .build();

    tokio::spawn(async move {
        application.run().await;
    });

    broker.tx_event.send(srad_client::Event::Online).unwrap();
    for _ in 0..2 {
        timeout(Duration::from_secs(1), broker.rx_outbound.recv())
            .await
            .unwrap()
            .unwrap();
    }

    let birth = utils::payloads::new_nbirth_payload(Some(vec![data_set_metric(10, 1.25)]));
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

    let birth_value = timeout(Duration::from_secs(1), store_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_is_data_set(birth_value, (10, 1.25));

    let data = utils::payloads::new_data_payload(1, vec![data_set_metric(20, 2.5)]);
    broker
        .tx_event
        .send(srad_client::Event::Node(NodeMessage {
            group_id: "group1".into(),
            node_id: "node1".into(),
            message: Message {
                payload: encoded(data),
                kind: MessageKind::Data,
            },
        }))
        .unwrap();

    let data_value = timeout(Duration::from_secs(1), store_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_is_data_set(data_value, (20, 2.5));
}
