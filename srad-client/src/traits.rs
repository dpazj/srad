use async_trait::async_trait;
use srad_types::{payload::Payload, topic::{DeviceTopic, NodeTopic, QoS, StateTopic, TopicFilter}};

use crate::{Event, LastWill, StatePayload};

#[async_trait]
pub trait Client {
  async fn disconnect(&self) -> Result<(),()>; 
  async fn publish_state_message(&self, topic: StateTopic, payload: StatePayload) -> Result<(),()>;
  async fn publish_node_message(&self, topic: NodeTopic, payload: Payload) -> Result<(),()>;
  async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(),()>;
  async fn subscribe(&self, topic: TopicFilter) -> Result<(),()> {self.subscribe_many(vec![topic]).await}
  async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(),()>; 
}

pub type DynClient = dyn Client + Send + Sync;

#[async_trait]
pub trait EventLoop
{
  async fn poll(&mut self) -> Option<Event>;
  fn set_last_will(&mut self, will: LastWill);
} 

pub type DynEventLoop = dyn EventLoop + Send;
