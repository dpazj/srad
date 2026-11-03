use async_trait::async_trait;
use srad_types::{
    payload::Payload,
    topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter},
};

use crate::{ClientError, Event, LastWill, StatePayload};

/// A trait for implementing a type that acts as a Sparkplug Client
#[async_trait]
pub trait Client {
    /// Disconnects the client.
    ///
    /// This uses a normal MQTT disconnect, so the server discards the client's
    /// Last Will message.
    async fn disconnect(&self) -> Result<(), ClientError>;

    /// Attempts to disconnect in a way that makes the MQTT server publish the
    /// client's Last Will message.
    ///
    /// Returns `true` when the implementation handled the disconnect without a
    /// normal MQTT `DISCONNECT` packet. The caller must close the connection,
    /// for example by dropping the event loop, so the server publishes the
    /// will. The caller may still publish its own death message, for example
    /// because brokers do not expose server-published wills to extensions.
    ///
    /// Returns `false` when the implementation did nothing. The caller must
    /// publish its own death message and perform a normal [Client::disconnect].
    ///
    /// The default implementation does nothing and returns `false`.
    async fn disconnect_with_will(&self) -> Result<bool, ClientError> {
        Ok(false)
    }

    /// Publishes a state message to the specified state topic.
    ///
    /// This method will yield to the async runtime until the message is accepted by the client
    async fn publish_state_message(
        &self,
        topic: StateTopic,
        payload: StatePayload,
    ) -> Result<(), ClientError>;

    /// Attempts to publish a state message to the specified state topic.
    ///
    /// Unlike `publish_state_message`, this method may return early if the client cannot process the message
    /// e.g the message queue is full.
    async fn try_publish_state_message(
        &self,
        topic: StateTopic,
        payload: StatePayload,
    ) -> Result<(), ClientError>;

    /// Publishes a message to a node-specific topic.
    ///
    /// This method will yield to the async runtime until the message is accepted by the client
    async fn publish_node_message(
        &self,
        topic: NodeTopic,
        payload: Payload,
    ) -> Result<(), ClientError>;

    /// Attempts to publish a message to a node-specific topic.
    ///
    /// Unlike `publish_node_message`, this method may return early if the client cannot process the message
    /// e.g the message queue is full.
    async fn try_publish_node_message(
        &self,
        topic: NodeTopic,
        payload: Payload,
    ) -> Result<(), ClientError>;

    /// Publishes a message to a device-specific topic.
    ///
    /// This method will yield to the async runtime until the message is accepted by the client
    async fn publish_device_message(
        &self,
        topic: DeviceTopic,
        payload: Payload,
    ) -> Result<(), ClientError>;

    /// Attempts to publish a message to a device-specific topic.
    ///
    /// Unlike `publish_device_message`, this method may return early if the client cannot process the message
    /// e.g the message queue is full.
    async fn try_publish_device_message(
        &self,
        topic: DeviceTopic,
        payload: Payload,
    ) -> Result<(), ClientError>;

    /// Subscribes to a single topic.
    ///
    /// This is a convenience method that calls `subscribe_many` with a single topic.
    async fn subscribe(&self, topic: TopicFilter) -> Result<(), ClientError> {
        self.subscribe_many(vec![topic]).await
    }

    /// Subscribes to multiple topics in a single operation.
    async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(), ClientError>;
}

pub type DynClient = dyn Client + Send + Sync;

/// A trait for implementing a type that acts as a Sparkplug Client's EventLoop
#[async_trait]
pub trait EventLoop {
    /// Poll the EventLoop for an event
    /// Continuing to poll will reconnect if there is
    /// a disconnection.
    /// **NOTE** Don't block this while iterating
    async fn poll(&mut self) -> Event;
    ///Set the last will to be used on the next connection
    fn set_last_will(&mut self, will: LastWill);
}

pub type DynEventLoop = dyn EventLoop + Send;
