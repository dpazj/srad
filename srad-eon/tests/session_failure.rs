use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use srad_client::{
    channel::{ChannelBroker, ChannelClient, ChannelEventLoop, OutboundMessage},
    Client, ClientError, Event,
};
use srad_eon::EoNBuilder;
use srad_types::{
    payload::Payload,
    topic::{DeviceTopic, NodeMessage, NodeTopic, StateTopic, TopicFilter},
};
use tokio::time::timeout;

#[derive(Debug, thiserror::Error)]
#[error("injected client failure")]
struct InjectedFailure;

/// A client that fails a configurable number of subscribe and publish requests.
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
        payload: srad_client::StatePayload,
    ) -> Result<(), ClientError> {
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
        if self
            .publish_failures_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(ClientError::new(InjectedFailure));
        }
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

async fn next_outbound(broker: &mut ChannelBroker) -> OutboundMessage {
    timeout(Duration::from_secs(2), broker.rx_outbound.recv())
        .await
        .expect("timed out waiting for a client request")
        .expect("broker closed")
}

#[tokio::test]
async fn subscribe_failure_retries_until_subscribed() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let client = FailingClient {
        inner: client,
        failures_left: Arc::new(AtomicUsize::new(2)),
        publish_failures_left: Arc::new(AtomicUsize::new(0)),
    };

    let (eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("group")
        .with_node_id("node")
        .with_retry_delay(Duration::from_millis(20))
        .build()
        .unwrap();

    let run_task = tokio::spawn(async move { eon.run().await });

    broker.tx_event.send(Event::Online).unwrap();

    // Every failed attempt disconnects instead of birthing.
    for _ in 0..2 {
        assert_eq!(
            next_outbound(&mut broker).await,
            OutboundMessage::Disconnect
        );
    }

    // The next attempt subscribes and only then publishes the birth.
    assert!(matches!(
        next_outbound(&mut broker).await,
        OutboundMessage::Subscribe(_)
    ));
    let message = next_outbound(&mut broker).await;
    match message {
        OutboundMessage::NodeMessage { topic, .. } => {
            assert_eq!(topic.message_type, NodeMessage::NBirth)
        }
        other => panic!("expected a node birth message, got {other:?}"),
    }

    // The node keeps running until it is cancelled.
    assert!(!run_task.is_finished());
    handle.cancel().await;
    timeout(Duration::from_secs(3), run_task)
        .await
        .expect("the node did not stop after being cancelled")
        .unwrap();
}

#[tokio::test]
async fn birth_failure_disconnects_and_retries() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let client = FailingClient {
        inner: client,
        failures_left: Arc::new(AtomicUsize::new(0)),
        publish_failures_left: Arc::new(AtomicUsize::new(1)),
    };

    let (eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("group")
        .with_node_id("node")
        .with_retry_delay(Duration::from_millis(20))
        .build()
        .unwrap();

    let run_task = tokio::spawn(async move { eon.run().await });

    broker.tx_event.send(Event::Online).unwrap();

    // The first attempt subscribes but the birth publish is rejected.
    assert!(matches!(
        next_outbound(&mut broker).await,
        OutboundMessage::Subscribe(_)
    ));
    assert_eq!(
        next_outbound(&mut broker).await,
        OutboundMessage::Disconnect
    );

    // The retry subscribes and births; no death is published for the failed session.
    assert!(matches!(
        next_outbound(&mut broker).await,
        OutboundMessage::Subscribe(_)
    ));
    match next_outbound(&mut broker).await {
        OutboundMessage::NodeMessage { topic, .. } => {
            assert_eq!(topic.message_type, NodeMessage::NBirth)
        }
        other => panic!("expected a node birth message, got {other:?}"),
    }

    assert!(!run_task.is_finished());
    handle.cancel().await;
    timeout(Duration::from_secs(3), run_task)
        .await
        .expect("the node did not stop after being cancelled")
        .unwrap();
}
