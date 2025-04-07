use srad_app::{App, SubscriptionConfig};
use srad_client::{channel::{ChannelBroker, ChannelEventLoop, OutboundMessage}, StatePayload};
use srad_types::{payload::StateBirthDeathCertificate, topic::{QoS, StateTopic, Topic, TopicFilter}};
use std::time::Duration;
use tokio::time::timeout;

async fn get_subscriptions_from_broker(broker: &mut ChannelBroker) -> Vec<TopicFilter> 
{
  let subscription = timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();
  match subscription {
    OutboundMessage::Subscribe(filters) => filters,
    message => panic!("got {message:?}")
  }
}

fn assert_filters_eq(a: Vec<TopicFilter>, b: Vec<TopicFilter>) {
  assert_eq!(a.len(), b.len());
  for x in a{
    assert!(b.contains(&x), "Sub filters did not contain expected filter: {x:?}")
  }
}

#[tokio::test]
async fn app_states() {
  let app_id= "foo";

  let (eventloop, client, mut broker) = ChannelEventLoop::new();
  let (mut application, app_client) = App::new("foo", SubscriptionConfig::SingleGroup { group_id: "test".into() }, eventloop, client);

  tokio::spawn(async move {
    application.run().await
  });

  broker.tx_event.send(srad_client::Event::Online).unwrap();
  let filters = get_subscriptions_from_broker(&mut broker).await;
  let expected_filters = vec![
    TopicFilter::new_with_qos(Topic::State(StateTopic::new_host(app_id)), QoS::AtMostOnce),
    TopicFilter::new_with_qos(Topic::Group{ id: "test".into() }, QoS::AtMostOnce)
  ];
  assert_filters_eq(filters, expected_filters);

  let will = broker.last_will().unwrap();
  assert_eq!(will.retain, true);
  assert_eq!(will.qos, QoS::AtLeastOnce);
  assert_eq!(will.topic, StateTopic::new_host(&app_id).topic);
  let payload = StateBirthDeathCertificate::try_from(will.payload.as_slice()).unwrap();
  assert_eq!(payload.online, false);
  let will_payload_timestamp = payload.timestamp;

  let state_publish= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();
  let (topic, payload) = match state_publish {
    OutboundMessage::StateMessage { topic, payload } => (topic, payload),
    message => panic!("got {message:?}")
  };
  assert_eq!(topic, StateTopic::new_host(&app_id));
  //timestamp in will must equal timestamp of online message
  assert_eq!(payload, StatePayload::Online { timestamp: will_payload_timestamp } );

  //if an offline message for the application is received, republish online
  broker.tx_event.send(srad_client::Event::State { host_id: app_id.into(), payload: StatePayload::Offline { timestamp: 0 } }).unwrap();
  let state_publish= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();
  let (topic, payload) = match state_publish {
    OutboundMessage::StateMessage { topic, payload } => (topic, payload),
    message => panic!("got {message:?}")
  };
  assert_eq!(topic, StateTopic::new_host(&app_id));
  //timestamp in will must equal timestamp of online message
  assert_eq!(payload, StatePayload::Online { timestamp: will_payload_timestamp } );

  //Offline message published on graceful shutdown
  app_client.cancel().await;
  let state_publish= timeout(Duration::from_secs(1), broker.rx_outbound.recv())
      .await
      .unwrap()
      .unwrap();
  let (topic, payload) = match state_publish {
    OutboundMessage::StateMessage { topic, payload } => (topic, payload),
    message => panic!("got {message:?}")
  };
  assert_eq!(topic, StateTopic::new_host(&app_id));
  assert!(matches!(payload, StatePayload::Offline { timestamp: _ } ));

}
