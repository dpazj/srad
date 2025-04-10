use std::sync::{Arc, Mutex};

use crate::{Event, LastWill, StatePayload};
use async_trait::async_trait;
use srad_types::{
    payload::Payload,
    topic::{DeviceTopic, NodeTopic, StateTopic, TopicFilter},
};
use tokio::sync::mpsc;

/// A [Client](crate::Client) implementation that uses channels for message passing.
///
/// # Examples
///
/// See [ChannelEventLoop]
#[derive(Clone)]
pub struct ChannelClient {
    tx: mpsc::UnboundedSender<OutboundMessage>,
}

#[async_trait]
impl crate::Client for ChannelClient {
    async fn disconnect(&self) -> Result<(), ()> {
        match self.tx.send(OutboundMessage::Disconnect) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    async fn publish_state_message(
        &self,
        topic: StateTopic,
        payload: StatePayload,
    ) -> Result<(), ()> {
        match self
            .tx
            .send(OutboundMessage::StateMessage { topic, payload })
        {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    async fn try_publish_state_message(
        &self,
        topic: StateTopic,
        payload: StatePayload,
    ) -> Result<(), ()> {
        self.publish_state_message(topic, payload).await
    }

    async fn publish_node_message(&self, topic: NodeTopic, payload: Payload) -> Result<(), ()> {
        match self
            .tx
            .send(OutboundMessage::NodeMessage { topic, payload })
        {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    async fn try_publish_node_message(&self, topic: NodeTopic, payload: Payload) -> Result<(), ()> {
        self.publish_node_message(topic, payload).await
    }

    async fn publish_device_message(&self, topic: DeviceTopic, payload: Payload) -> Result<(), ()> {
        match self
            .tx
            .send(OutboundMessage::DeviceMessage { topic, payload })
        {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    async fn try_publish_device_message(
        &self,
        topic: DeviceTopic,
        payload: Payload,
    ) -> Result<(), ()> {
        self.publish_device_message(topic, payload).await
    }

    async fn subscribe_many(&self, topics: Vec<TopicFilter>) -> Result<(), ()> {
        match self.tx.send(OutboundMessage::Subscribe(topics)) {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }
}

/// An Enum representing different messages and requests a [ChannelClient] can send to the [ChannelBroker]
#[derive(Clone, Debug, PartialEq)]
pub enum OutboundMessage {
    Disconnect,
    StateMessage {
        topic: StateTopic,
        payload: StatePayload,
    },
    NodeMessage {
        topic: NodeTopic,
        payload: Payload,
    },
    DeviceMessage {
        topic: DeviceTopic,
        payload: Payload,
    },
    Subscribe(Vec<TopicFilter>),
}

/// A "broker" that manages the communication between a [ChannelClient] and an [ChannelEventLoop].
///
/// Used to send messages to the eventloop and inspect messages/requests produced by the client
///
/// # Examples
///
/// ```no_run
/// use srad_client::{Event, channel::{ChannelEventLoop, ChannelClient}};
/// use tokio::runtime::Runtime;
///
/// let rt = Runtime::new().unwrap();
/// rt.block_on(async {
///     let (mut eventloop, client, mut broker) = ChannelEventLoop::new();
///
///     //create application that uses the EventLoop and client
///     
///     //Send an event to the EventLoop
///     broker.tx_event.send(Event::Online).unwrap();
///
///     //Receive a message or request from the Client
///     let message = broker.rx_outbound.recv().await.unwrap();
/// });
/// ```
pub struct ChannelBroker {
    pub rx_outbound: mpsc::UnboundedReceiver<OutboundMessage>,
    pub tx_event: mpsc::UnboundedSender<Event>,
    last_will: Arc<Mutex<Option<LastWill>>>,
}

impl ChannelBroker {
    /// Retrieves the current last will message set by the EventLoop, if set.
    pub fn last_will(&self) -> Option<LastWill> {
        self.last_will.lock().unwrap().clone()
    }
}

/// An [EventLoop](crate::EventLoop) implementation that uses channels
///
/// # Examples
///
/// See [ChannelBroker]
pub struct ChannelEventLoop {
    rx: mpsc::UnboundedReceiver<Event>,
    last_will: Arc<Mutex<Option<LastWill>>>,
}

impl ChannelEventLoop {
    /// Creates a new event loop along with the corresponding client and broker.
    pub fn new() -> (Self, ChannelClient, ChannelBroker) {
        let (tx_event, rx_event) = mpsc::unbounded_channel();
        let (tx_outbound, rx_outbound) = mpsc::unbounded_channel();
        let last_will = Arc::new(Mutex::new(None));
        let el = Self {
            rx: rx_event,
            last_will: last_will.clone(),
        };
        (
            el,
            ChannelClient { tx: tx_outbound },
            ChannelBroker {
                rx_outbound,
                tx_event,
                last_will,
            },
        )
    }
}

#[async_trait]
impl crate::EventLoop for ChannelEventLoop {
    async fn poll(&mut self) -> Event {
        self.rx.recv().await.unwrap()
    }

    fn set_last_will(&mut self, will: LastWill) {
        let mut lw = self.last_will.lock().unwrap();
        *lw = Some(will)
    }
}
