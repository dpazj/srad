use async_trait::async_trait;
use srad_types::{payload::Payload, topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter}};

use crate::{Event, LastWill, StatePayload};

#[async_trait]
pub trait Client {

  /// Disconnects the client.
  async fn disconnect(&self) -> Result<(),()>; 

  /// Publishes a state message to the specified state topic.
  ///
  /// This method will yield to the async runtime until the message is accepted by the client 
  async fn publish_state_message(&self, topic: StateTopic, payload: StatePayload) -> Result<(),()>;

  /// Attempts to publish a state message to the specified state topic.
  ///
  /// Unlike `publish_state_message`, this method may return early if the client cannot process the message 
  /// e.g the message queue is full.
  async fn try_publish_state_message(&self, topic: StateTopic, payload: StatePayload) -> Result<(),()>;

  /// Publishes a message to a node-specific topic.
  ///
  /// This method will yield to the async runtime until the message is accepted by the client 
  async fn publish_node_message(&self, topic: NodeTopic, payload: Payload) -> Result<(),()>;

  /// Attempts to publish a message to a node-specific topic.
  ///
  /// Unlike `publish_node_message`, this method may return early if the client cannot process the message 
  /// e.g the message queue is full.
  async fn try_publish_node_message(&self, topic: NodeTopic, payload: Payload) -> Result<(),()>;

  /// Publishes a message to a device-specific topic.
  ///
  /// This method will yield to the async runtime until the message is accepted by the client 
  async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(),()>;

  /// Attempts to publish a message to a device-specific topic.
  ///
  /// Unlike `publish_device_message`, this method may return early if the client cannot process the message 
  /// e.g the message queue is full.
  async fn try_publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(),()>;
  
  /// Subscribes to a single topic.
  ///
  /// This is a convenience method that calls `subscribe_many` with a single topic.
  async fn subscribe(&self, topic: TopicFilter) -> Result<(),()> {self.subscribe_many(vec![topic]).await}

  /// Subscribes to multiple topics in a single operation.
  async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(),()>; 
}

pub type DynClient = dyn Client + Send + Sync;

#[async_trait]
pub trait EventLoop
{
  /// Poll the EventLoop for an event
  async fn poll(&mut self) -> Event;
  ///Set the last will to be used on the next connection 
  fn set_last_will(&mut self, will: LastWill);
} 

pub type DynEventLoop = dyn EventLoop + Send;
