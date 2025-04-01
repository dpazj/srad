use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use srad_client::{Event, LastWill, StatePayload};
use srad_types::{payload::Payload, topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter}};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub enum OutboundMessage {
  Disconnect,
  StateMessage {topic: StateTopic, payload: StatePayload},
  NodeMessage{topic: NodeTopic, payload: Payload},
  DeviceMessage{topic: DeviceTopic, payload: Payload},
  Subscribe (Vec<TopicFilter>)
}

#[derive(Clone)]
pub struct Client {
  tx: mpsc::UnboundedSender<OutboundMessage>
}

#[async_trait]
impl srad_client::Client for Client {

  async fn disconnect(&self) -> Result<(),()> {
    match self.tx.send(OutboundMessage::Disconnect) {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }

  async fn publish_state_message(&self, topic: StateTopic, payload: StatePayload) -> Result<(),()> {
    match self.tx.send(OutboundMessage::StateMessage { topic,  payload }) {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }

  async fn publish_node_message(&self, topic: NodeTopic, payload: Payload) -> Result<(),()> {
    match self.tx.send(OutboundMessage::NodeMessage { topic, payload }) {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }

  async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(),()> {
    match self.tx.send(OutboundMessage::DeviceMessage { topic, payload }) {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }

  async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(),()> {
    match self.tx.send(OutboundMessage::Subscribe(topics)) {
      Ok(_) => Ok(()),
      Err(_) => Err(()),
    }
  }
}

pub struct Broker {
  pub rx_outbound: mpsc::UnboundedReceiver<OutboundMessage>,
  pub tx_event: mpsc::UnboundedSender<Option<Event>>,
  last_will: Arc<Mutex<Option<LastWill>>>
}

impl Broker {
  pub fn last_will(&self) -> Option<LastWill> {
    self.last_will.lock().unwrap().clone()
  } 
}

pub struct EventLoop{
  rx: mpsc::UnboundedReceiver<Option<Event>>,
  last_will: Arc<Mutex<Option<LastWill>>>
}

impl EventLoop {
  pub fn new() -> (Self, Client, Broker) {
    let (tx_event, rx_event) = mpsc::unbounded_channel();
    let (tx_outbound, rx_outbound) = mpsc::unbounded_channel();
    let last_will = Arc::new(Mutex::new(None));
    let el = Self {rx: rx_event, last_will: last_will.clone()};
    (el, Client{tx: tx_outbound}, Broker{ rx_outbound, tx_event, last_will })
  }
}

#[async_trait]
impl srad_client::EventLoop for EventLoop {

  async fn poll(&mut self) -> Option<Event> {
    self.rx.recv().await.unwrap() 
  }

  fn set_last_will(&mut self, will: LastWill) {
    let mut lw = self.last_will.lock().unwrap();
    *lw = Some(will)
  }
}

