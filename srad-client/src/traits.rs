use async_trait::async_trait;
use srad_types::{payload::Payload, topic::{DeviceTopic, NodeTopic, TopicFilter}};

use crate::{Event, LastWill};

#[async_trait]
pub trait Client {
  async fn disconnect(&self); 
  async fn publish_node_message(&self, topic: NodeTopic, payload: Payload);
  async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload);
  async fn subscribe(&self, topic: TopicFilter) {self.subscribe_many(vec![topic]).await}
  async fn subscribe_many(&self, topics: Vec<TopicFilter>); 
}

pub type DynClient = dyn Client + Send + Sync;

#[async_trait]
pub trait EventLoop
{
  async fn poll(&mut self) -> Option<Event>;
  fn set_last_will(&mut self, will: LastWill);
} 

pub type DynEventLoop = dyn EventLoop + Send;
