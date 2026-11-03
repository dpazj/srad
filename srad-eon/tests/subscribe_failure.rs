use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use srad_client::{
    channel::{ChannelClient, ChannelEventLoop, OutboundMessage},
    Client, ClientError, Event,
};
use srad_eon::{EoNBuilder, EoNError};
use srad_types::{
    payload::Payload,
    topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter},
};
use tokio::time::timeout;

#[derive(Debug, thiserror::Error)]
#[error("injected subscribe failure")]
struct SubscribeFailure;

#[derive(Clone)]
struct FailingClient {
    inner: ChannelClient,
    fail_subscribe: Arc<AtomicBool>,
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
            return Err(ClientError::new(SubscribeFailure));
        }
        self.inner.subscribe_many(topics).await
    }
}

#[tokio::test]
async fn subscribe_failure_stops_the_node_without_publishing_a_birth() {
    let (eventloop, client, mut broker) = ChannelEventLoop::new();
    let client = FailingClient {
        inner: client,
        fail_subscribe: Arc::new(AtomicBool::new(true)),
    };

    let (eon, _handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("group")
        .with_node_id("node")
        .build()
        .unwrap();

    let run_task = tokio::spawn(async move { eon.run().await });

    broker.tx_event.send(Event::Online).unwrap();

    let result = timeout(Duration::from_secs(1), run_task)
        .await
        .unwrap()
        .unwrap();

    assert!(
        matches!(result, Err(EoNError::Subscribe(_))),
        "expected a subscribe error, got {result:?}"
    );

    let mut messages = Vec::new();
    while let Ok(message) = broker.rx_outbound.try_recv() {
        messages.push(message);
    }

    assert!(
        messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::Disconnect)),
        "the node should disconnect after a failed subscription: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, OutboundMessage::NodeMessage { .. })),
        "the node must not publish a birth without subscriptions: {messages:?}"
    );
}
