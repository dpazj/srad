use std::{time::Duration};

use srad_client::{channel::{ChannelEventLoop, OutboundMessage}, Message, NodeMessage};
use srad_app::{generic_app::{Application, RebirthConfig}, SubscriptionConfig};
use srad_types::{payload::{DataType, Metric}, topic::NodeTopic, utils::timestamp, MetricValue};
use tokio::time::timeout;
use utils::payloads::assert_payload_is_rebirth_request;

mod utils;

#[tokio::test]
async fn reorder_rebirths() {
    let reorder_timeout = 100;
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let (application, _) = Application::new(
        "foo",
        eventloop,
        client,
        SubscriptionConfig::SingleGroup {
            group_id: "test".into(),
        },
    );

    tokio::spawn(async move {
        application.with_rebirth_config(
            RebirthConfig { 
                invalid_payload: true, 
                out_of_sync_bdseq: true,
                unknown_node: true, 
                unknown_device: true,
                unknown_metric: true,
                reorder_timeout: Some(Duration::from_millis(reorder_timeout)), 
                reorder_failure: true,
                recorded_state_stale: true,
                rebirth_cooldown: Duration::from_secs(1)
            }
        )
        .run().await;
    });
    
    broker.tx_event.send(srad_client::Event::Online).unwrap();
    timeout(Duration::from_secs(1), broker.rx_outbound.recv()).await.unwrap().unwrap();
    timeout(Duration::from_secs(1), broker.rx_outbound.recv()).await.unwrap().unwrap();

    let mut metric_a= Metric::new();
    metric_a.set_name("A".into())
        .set_datatype(DataType::Int32)
        .set_value(MetricValue::from(0i32).into())
        .set_timestamp(timestamp());
        
    //Send a valid nbirth
    broker.tx_event.send(srad_client::Event::Node(NodeMessage { 
        group_id:"group1".to_string(), 
        node_id: "node1".to_string(), 
        message: Message { 
            payload: utils::payloads::new_nbirth_payload(Some(vec![metric_a])),
            kind: srad_client::MessageKind::Birth
        }})).unwrap();
    

    //Send an out of order data message
    let mut metric_a_1= Metric::new();
    metric_a_1.set_name("A".into())
        .set_value(MetricValue::from(1i32).into())
        .set_timestamp(timestamp());

    broker.tx_event.send(srad_client::Event::Node(NodeMessage { 
        group_id:"group1".to_string(), 
        node_id: "node1".to_string(), 
        message: Message { 
            payload: utils::payloads::new_data_payload(2,vec![metric_a_1]),
            kind: srad_client::MessageKind::Data
        }})).unwrap();

    let rebirth = timeout(Duration::from_millis(1000 + reorder_timeout), broker.rx_outbound.recv()).await.unwrap().unwrap();
    let (topic, payload) = match rebirth {
        OutboundMessage::NodeMessage { topic, payload } => (topic, payload),
        message => panic!("got {message:?}"),
    };
    assert_eq!(topic, NodeTopic::new("group1", srad_types::topic::NodeMessage::NCmd, "node1"));
    assert_payload_is_rebirth_request (&payload);
}