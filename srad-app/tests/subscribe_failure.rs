use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use srad_app::{AppError, AppEvent, AppEventLoop, SubscriptionConfig};
use srad_client::{
    channel::{ChannelClient, ChannelEventLoop, OutboundMessage},
    Client, ClientError, Event,
};
use srad_types::{
    payload::Payload,
    topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter},
};
use tokio::time::timeout;

#[derive(Debug, thiserror::Error)]
#[error("injected client failure")]
struct InjectedFailure;

#[derive(Clone)]
struct FailingClient {
    inner: ChannelClient,
    fail_subscribe: Arc<AtomicBool>,
    fail_publish_state: Arc<AtomicBool>,
}

#[async_trait]
impl Client for FailingClient {
    async fn disconnect(&self) -> Result<(), ClientError> {
        self.inner.disconnect().await
    }

    async fn publish_state_message(
        &self,
        topic: StateTopic,
        payload: srad_client::StatePayload,
    ) -> Result<(), ClientError> {
        if self.fail_publish_state.load(Ordering::SeqCst) {
            return Err(ClientError::new(InjectedFailure));
        }
        self.inner.publish_state_message(topic, payload).await
    }

    async fn try_publish_state_message(
        &self,
        topic: StateTopic,
        payload: srad_client::StatePayload,
    ) -> Result<(), ClientError> {
        self.inner.try_publish_state_message(topic, payload).await
    }

    async fn publish_node_message(
        &self,
        topic: NodeTopic,
        payload: Payload,
    ) -> Result<(), ClientError> {
        self.inner.publish_node_message(topic, payload).await
    }

    async fn try_publish_node_message(
        &self,
        topic: NodeTopic,
        payload: Payload,
    ) -> Result<(), ClientError> {
        self.inner.try_publish_node_message(topic, payload).await
    }

    async fn publish_device_message(
        &self,
        topic: DeviceTopic,
        payload: Payload,
    ) -> Result<(), ClientError> {
        self.inner.publish_device_message(topic, payload).await
    }

    async fn try_publish_device_message(
        &self,
        topic: DeviceTopic,
        payload: Payload,
    ) -> Result<(), ClientError> {
        self.inner.try_publish_device_message(topic, payload).await
    }

    async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(), ClientError> {
        if self.fail_subscribe.load(Ordering::SeqCst) {
            return Err(ClientError::new(InjectedFailure));
        }
        self.inner.subscribe_many(topics).await
    }
}

fn drain_messages(broker: &mut srad_client::channel::ChannelBroker) -> Vec<OutboundMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = broker.rx_outbound.try_recv() {
        messages.push(message);
    }
    messages
}

#[tokio::test]
async fn subscribe_failure_keeps_the_app_offline_and_retries_on_reconnect() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let fail_subscribe = Arc::new(AtomicBool::new(true));
    let client = FailingClient {
        inner: client,
        fail_subscribe: fail_subscribe.clone(),
        fail_publish_state: Arc::new(AtomicBool::new(false)),
    };

    let (mut app, _client) =
        AppEventLoop::new("app", SubscriptionConfig::AllGroups, eventloop, client);

    broker.tx_event.send(Event::Online).unwrap();

    let result = timeout(Duration::from_secs(1), app.poll()).await.unwrap();
    assert!(
        matches!(result, Err(AppError::Subscribe(_))),
        "expected a subscribe error, got {result:?}"
    );

    let messages = drain_messages(&mut broker);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Disconnect)),
        "the app should disconnect after a failed subscription: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::StateMessage { .. })),
        "the app must not advertise itself online without subscriptions: {messages:?}"
    );

    // Simulate the client reconnecting and the subscription succeeding this time.
    fail_subscribe.store(false, Ordering::SeqCst);
    broker.tx_event.send(Event::Online).unwrap();

    let event = timeout(Duration::from_secs(1), app.poll())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, AppEvent::Online));

    let messages = drain_messages(&mut broker);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Subscribe(_))),
        "the app should subscribe before going online: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::StateMessage { .. })),
        "the app should publish its online state after subscribing: {messages:?}"
    );
}

#[tokio::test]
async fn publish_online_state_failure_keeps_the_app_offline_and_retries_on_reconnect() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let fail_publish_state = Arc::new(AtomicBool::new(true));
    let client = FailingClient {
        inner: client,
        fail_subscribe: Arc::new(AtomicBool::new(false)),
        fail_publish_state: fail_publish_state.clone(),
    };

    let (mut app, _client) =
        AppEventLoop::new("app", SubscriptionConfig::AllGroups, eventloop, client);

    broker.tx_event.send(Event::Online).unwrap();

    let result = timeout(Duration::from_secs(1), app.poll()).await.unwrap();
    assert!(
        matches!(result, Err(AppError::PublishOnlineState(_))),
        "expected a publish error, got {result:?}"
    );

    let messages = drain_messages(&mut broker);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Disconnect)),
        "the app should disconnect when it cannot advertise itself online: {messages:?}"
    );

    // Simulate the client reconnecting and publishing the online state succeeding.
    fail_publish_state.store(false, Ordering::SeqCst);
    broker.tx_event.send(Event::Online).unwrap();

    let event = timeout(Duration::from_secs(1), app.poll())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, AppEvent::Online));

    let messages = drain_messages(&mut broker);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::StateMessage { .. })),
        "the app should publish its online state after subscribing: {messages:?}"
    );
}
