use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use srad_app::{AppEvent, AppEventLoop, SubscriptionConfig};
use srad_client::{
    channel::{ChannelBroker, ChannelClient, ChannelEventLoop, OutboundMessage},
    Client, ClientError, Event, StatePayload,
};
use srad_types::{
    payload::Payload,
    topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter},
};
use tokio::time::timeout;

#[derive(Debug, thiserror::Error)]
#[error("injected client failure")]
struct InjectedFailure;

/// A client that fails a configurable number of subscribe and online state publishes.
#[derive(Clone)]
struct FailingClient {
    inner: ChannelClient,
    failures_left: Arc<AtomicUsize>,
    publish_failures_left: Arc<AtomicUsize>,
}

#[async_trait]
impl Client for FailingClient {
    async fn disconnect(&self) -> Result<(), ClientError> {
        self.inner.disconnect().await
    }

    async fn publish_state_message(
        &self,
        topic: StateTopic,
        payload: StatePayload,
    ) -> Result<(), ClientError> {
        if self
            .publish_failures_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(ClientError::new(InjectedFailure));
        }
        self.inner.publish_state_message(topic, payload).await
    }

    async fn try_publish_state_message(
        &self,
        topic: StateTopic,
        payload: StatePayload,
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
        if self
            .failures_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(ClientError::new(InjectedFailure));
        }
        self.inner.subscribe_many(topics).await
    }
}

fn drain_messages(broker: &mut ChannelBroker) -> Vec<OutboundMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = broker.rx_outbound.try_recv() {
        messages.push(message);
    }
    messages
}

#[tokio::test]
async fn subscribe_failure_is_retried_until_the_app_is_online() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let client = FailingClient {
        inner: client,
        failures_left: Arc::new(AtomicUsize::new(2)),
        publish_failures_left: Arc::new(AtomicUsize::new(0)),
    };

    let (mut app, app_client) =
        AppEventLoop::new("app", SubscriptionConfig::AllGroups, eventloop, client);
    app.set_retry_delay(Duration::from_millis(100));

    broker.tx_event.send(Event::Online).unwrap();

    // The first attempt fails and the application stays offline instead of returning.
    assert!(timeout(Duration::from_millis(50), app.poll())
        .await
        .is_err());

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

    // A later retry succeeds and only then publishes the online state.
    let event = timeout(Duration::from_secs(2), app.poll())
        .await
        .expect("the app did not retry");
    assert!(matches!(event, AppEvent::Online));

    let messages = drain_messages(&mut broker);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Subscribe(_))),
        "the app should subscribe before going online: {messages:?}"
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            OutboundMessage::StateMessage {
                payload: StatePayload::Online { .. },
                ..
            }
        )),
        "the app should publish its online state after subscribing: {messages:?}"
    );

    // Only cancelling stops the application.
    app_client.cancel().await;
    let event = timeout(Duration::from_secs(3), app.poll())
        .await
        .expect("the app did not stop after being cancelled");
    assert!(matches!(event, AppEvent::Cancelled));
}

#[tokio::test]
async fn online_state_publish_failure_is_retried() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let client = FailingClient {
        inner: client,
        failures_left: Arc::new(AtomicUsize::new(0)),
        publish_failures_left: Arc::new(AtomicUsize::new(1)),
    };

    let (mut app, app_client) =
        AppEventLoop::new("app", SubscriptionConfig::AllGroups, eventloop, client);
    app.set_retry_delay(Duration::from_millis(100));

    broker.tx_event.send(Event::Online).unwrap();

    assert!(timeout(Duration::from_millis(50), app.poll())
        .await
        .is_err());

    let messages = drain_messages(&mut broker);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Subscribe(_))),
        "the app should subscribe before publishing its online state: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Disconnect)),
        "the app should disconnect when it cannot publish its online state: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::StateMessage { .. })),
        "the failed online state publish must not be advertised: {messages:?}"
    );

    // The retry succeeds and publishes the online state exactly once.
    let event = timeout(Duration::from_secs(2), app.poll())
        .await
        .expect("the app did not retry");
    assert!(matches!(event, AppEvent::Online));

    let messages = drain_messages(&mut broker);
    let online_states = messages
        .iter()
        .filter(|message| {
            matches!(
                message,
                OutboundMessage::StateMessage {
                    payload: StatePayload::Online { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(online_states, 1);

    app_client.cancel().await;
}
