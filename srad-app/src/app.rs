use std::{collections::HashMap, sync::{Arc, Mutex}};

use srad_client::{Client, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, Message, NodeMessage};
use srad_types::{metric::MetricValidToken, payload::Payload, topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter}};
use tokio::select;

use crate::{config::AppSubscriptionConfig, store::{MetricDetails, MetricToken}};

struct NodeState {
   seq: u64,  
   last_rebirth_request: u64,  
   metrics_valid_token: MetricValidToken,  
}
struct Node {
    state: Mutex<NodeState>,
    client: Arc<DynClient>,
}

struct NodeHandle {
    node: Arc<Node>
}

impl Node {

    fn new_from_birth_payload(payload: Payload) -> Result<(Self, Vec<(MetricToken, MetricDetails)>), ()> {
        Err(())
    }

    fn handle_payload_seq() -> Result<(), ()> {
        Ok(())
    }

    fn update_from_birth_payload(&self, payload: Payload)-> Result<Vec<(MetricToken, MetricDetails)>, ()> {
        Err(())
    }



}

pub struct AppClient {
    client: Arc<DynClient>
}

enum AppPublishTopicKind {
    NodeTopic(NodeTopic),
    DeviceTopic(DeviceTopic) 
}

struct AppPublishTopic(AppPublishTopicKind);



impl AppClient {
    async fn publish_cmd(topic: AppPublishTopic, ){}
}

struct Group {
    id: Arc<String>,
    nodes: Mutex<HashMap<Arc<String>, Arc<Node>>>
}

impl Group {

    fn new(id: Arc<String>) -> Self {
        Self {
            id,
            nodes: Mutex::new(HashMap::new())
        }
    }

    fn get_node(&self, node_id: &String) -> Option<Arc<Node>> {
        let nodes = self.nodes.lock().unwrap();
        nodes.get(node_id).map(Arc::clone)
    }

    async fn handle_node_message(&self, node_id: String, message: Message) {
        
        match message {
            Message::Birth { payload } => {

                let node = {
                    let mut nodes = self.nodes.lock().unwrap();
                    match nodes.get(&node_id) {
                        Some(node) => {
                            match node.update_from_birth_payload(payload) {
                                Ok(v) => Ok((node.clone(), v)),
                                Err(e) => Err(e),
                            }
                        },
                        None => {
                            let node_id = Arc::new(node_id);
                            match Node::new_from_birth_payload(payload) {
                                Ok((node,metrics)) => {
                                    let arc_node = Arc::new(node);
                                    nodes.insert(node_id, arc_node.clone());
                                    Ok((arc_node, metrics))
                                },
                                Err(e) => Err(e),
                            }
                        },
                    }
                };
                //nbirth_cb (node, metrics).await
            },
            Message::Data { payload } => todo!(),
            Message::Death { payload } => todo!(),
            _ => (),
        }
    }



}

pub struct App {
    host_id: String,
    subscription_config: AppSubscriptionConfig,
    client: Arc<DynClient>,
    eventloop: Box<DynEventLoop>,
    groups: Mutex<HashMap<Arc<String>, Arc<Group>>>
}

impl App {

    pub fn new<S: Into<String>, S1: Into<String>, E: EventLoop + Send +'static, C: Client + Send + Sync + 'static>(
        host_id: S1,
        subscription_config: AppSubscriptionConfig,
        eventloop: E,
        client: C
    ) -> Self {
        Self {
            host_id: host_id.into(),
            client: Arc::new(client),
            eventloop: Box::new(eventloop),
            subscription_config,
            groups: Mutex::new(HashMap::new())
        }
    }


    fn update_last_will(&mut self) {
        self.eventloop.set_last_will(
            srad_client::LastWill::new_app(
               &self.host_id 
            )
        );
    }

    fn on_online(&self) {
        let client = self.client.clone();
        let mut topics: Vec<TopicFilter> = self.subscription_config.clone().into();
        topics.push(TopicFilter::new_with_qos(Topic::State(StateTopic::new_host(&self.host_id)), QoS::AtMostOnce));

        tokio::spawn(async move {
           client.subscribe_many(topics).await
        });
    }

    fn on_offline(&mut self) {
        
    }

    fn get_or_create_group(&self, group_id: String) -> Arc<Group> {
        let mut groups= self.groups.lock().unwrap();
        let group = groups.get(&group_id);
        if let Some(group) = group {
            return group.clone()
        }

        let group_id= Arc::new(group_id);
        let group = Arc::new(Group::new(group_id.clone()));
        groups.insert(group_id, group.clone());
        group
    }

    fn handle_node_message(&self, message: NodeMessage) {
        let group = self.get_or_create_group(message.group_id);
        tokio::spawn(async move { group.handle_node_message(message.node_id, message.message).await } );
      //get or create node
      //
    }

    fn handle_device_message(&self, message: DeviceMessage) {
        // let group = self.get_or_create_group(message.group_id);
        // tokio::spawn(async move { group.handle_device_message(message.node_id, message.device_id, message.message).await } );
    }

    async fn handle_event(&mut self, event: Option<Event>) 
    {
        if let Some (event) = event {
            match event {
                Event::Online => self.on_online(),
                Event::Offline => self.on_offline(), 
                Event::Node(node_message) => {
                   self.handle_node_message(node_message); 
                },
                Event::Device(device_message) => {
                   self.handle_device_message(device_message); 
                },
                Event::State{ host_id, payload } => (),
                Event::InvalidPublish { reason: _, topic: _, payload: _ } => (),
            }
        }
    }

    pub async fn run(&mut self) {
        self.update_last_will();
        loop {
            select!{
                event = self.eventloop.poll() => self.handle_event(event).await
            }
        }
    } 
}