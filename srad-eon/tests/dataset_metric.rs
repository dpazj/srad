use std::time::Duration;

use srad_client::channel::{ChannelEventLoop, OutboundMessage};
use srad_eon::{
    AliasConfig, BirthInitializer, BirthMetricDetails, EoNBuilder, MetricManager, NodeMetricManager,
};
use srad_types::{
    payload::DataType,
    topic::{NodeMessage, NodeTopic},
    DataSet, DataSetValue, MetricValue, MetricValueKind,
};
use tokio::time::timeout;

struct DataSetManager {
    data_set: DataSet,
}

impl MetricManager for DataSetManager {
    fn initialise_birth(&self, bi: &mut BirthInitializer) {
        bi.register_metric(
            BirthMetricDetails::new_with_initial_value("dataset", self.data_set.clone())
                .with_alias_config(AliasConfig::None),
        )
        .unwrap();
    }
}

impl NodeMetricManager for DataSetManager {}

#[tokio::test]
async fn data_set_metric_is_included_in_node_birth() {
    let mut data_set = DataSet::new();
    data_set
        .init(["Time", "Value"], [DataType::UInt64, DataType::Double])
        .add_row_values([DataSetValue::from(10_u64), DataSetValue::from(1.25_f64)])
        .add_row_values([DataSetValue::from(20_u64), DataSetValue::from(2.5_f64)]);

    let (channel_eventloop, client, mut broker) = ChannelEventLoop::new();
    let (eventloop, _node) = EoNBuilder::new(channel_eventloop, client)
        .with_group_id("group")
        .with_node_id("node")
        .with_metric_manager(DataSetManager {
            data_set: data_set.clone(),
        })
        .build()
        .unwrap();

    tokio::spawn(async move { eventloop.run().await });

    broker.tx_event.send(srad_client::Event::Online).unwrap();

    let subscription = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(subscription, OutboundMessage::Subscribe(_)));

    let birth = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
        .await
        .unwrap()
        .unwrap();

    let (topic, payload) = match birth {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        other => panic!("expected node birth, got {other:?}"),
    };
    assert_eq!(topic, NodeTopic::new("group", NodeMessage::NBirth, "node"));

    let metric = payload
        .metrics
        .iter()
        .find(|metric| metric.name.as_deref() == Some("dataset"))
        .expect("dataset metric missing from birth payload");

    assert_eq!(metric.datatype, Some(DataType::DataSet as u32));

    let value = MetricValue::new(metric.value.clone().expect("dataset metric has no value"));
    let kind = MetricValueKind::try_from_metric_value(DataType::DataSet, value).unwrap();

    match kind {
        MetricValueKind::DataSet(decoded) => {
            assert_eq!(decoded.columns(), data_set.columns());
            assert_eq!(decoded.types(), data_set.types());
            assert_eq!(decoded.rows(), data_set.rows());
        }
        other => panic!("expected a DataSet metric value, got {other:?}"),
    }
}
