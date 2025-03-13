use std::{collections::HashMap, sync::{Arc, Mutex}};

use srad_client::{Client, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, Message, MessageKind, NodeMessage};
use srad_types::{payload::{Metric, Payload}, topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter}};
use tokio::select;

use crate::{config::AppSubscriptionConfig, metrics::{get_metric_birth_details_from_birth_metrics, get_metric_id_and_details_from_payload_metrics}};

struct DeviceState {
    birth_timestamp: u64,
    death_timestamp: u64,
}

impl DeviceState {
    fn new(birth_timestamp: u64) -> Self {
        Self {
            birth_timestamp,
            death_timestamp: 0,
        }
    }
}

struct NodeState {
   seq: u64, 
   birth_timestamp: u64,
   death_timestamp: u64,
   last_rebirth_request: u64,
   devices: HashMap<String, DeviceState>  
}

enum SeqState {
    OrderGood,
    OutOfOrder 
}

impl NodeState {

    fn new(seq: u64, birth_timestamp: u64) -> Self {
        Self {
            seq, birth_timestamp, death_timestamp: 0, last_rebirth_request: 0, devices: HashMap::new()
        } 
    }

    fn validate_and_update_seq(&mut self, seq: u64) -> SeqState {
        self.seq = seq;
        SeqState::OrderGood
    }

    fn validate_and_update_birth_timestamp(&mut self, birth_timestamp: u64) -> bool {
        if birth_timestamp <= self.birth_timestamp { return false }
        if birth_timestamp < self.death_timestamp { return false }
        self.birth_timestamp = birth_timestamp;
        true
    }

    fn validate_and_update_death_timestamp(&mut self, death_timestamp: u64) -> bool {
        if death_timestamp <= self.death_timestamp { return false }
        if death_timestamp < self.birth_timestamp { return false }
        self.death_timestamp = death_timestamp;
        true
    }

    fn validate_data_timestamp(&self, timestamp: u64) -> bool {
        if timestamp < self.death_timestamp { return false }
        if timestamp < self.birth_timestamp{ return false }
        return true
    }

    fn get_device(&self, device_name: &String) -> Option<DeviceState> {
        todo!()
    }

    fn add_device(&self, name: String, device: DeviceState) {

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

#[derive(PartialEq, Eq, Hash)]
struct NodeIdentifier {
    group: String,
    node_id: String
}

struct State {
    nodes: Mutex<HashMap<NodeIdentifier, NodeState>>,
}

impl State {

    fn new() -> Self {
        State { nodes: Mutex::new(HashMap::new()) }
    }

    fn handle_nbirth(&self, id: NodeIdentifier, seq: u64, timestamp: u64, metrics: Vec<Metric>) {
       

        let mut nodes = self.nodes.lock().unwrap();
        match nodes.get_mut(&id) {
            Some(node) => {
                node.validate_and_update_seq(seq);
                if !node.validate_and_update_birth_timestamp(seq) {
                    //timestamp not new
                    return
                }
            },
            None => {
                let node = NodeState::new(seq, timestamp);
                nodes.insert(id, node);
            },
        };

        let metric_details = get_metric_birth_details_from_birth_metrics(metrics);
        //callback
    }

    fn handle_ndeath(&self, id: NodeIdentifier, seq: u64, timestamp: u64) {
        let mut nodes = self.nodes.lock().unwrap();
        let node = match nodes.get_mut(&id) {
            Some(node) => node,
            None => return,
        };

        node.validate_and_update_seq(seq);
        if !node.validate_and_update_death_timestamp(timestamp) { return };
        //callback
    }

    fn handle_ndata(&self, id: NodeIdentifier, seq: u64, metrics: Vec<Metric>) {
        let mut nodes = self.nodes.lock().unwrap();
        let node = match nodes.get_mut(&id) {
            Some(node) => node,
            None => return,
        };

        node.validate_and_update_seq(seq);
        drop(nodes);

        let data_metric_details = get_metric_id_and_details_from_payload_metrics(metrics);
        //tokio::spawn(cb(timestamp, details));
    }

    fn handle_node_message(&self, message: NodeMessage) {
        let id = NodeIdentifier { group: message.group_id, node_id: message.node_id };
        let message_kind = message.message.kind;
        let payload = message.message.payload;
        let seq = match payload.seq {
            Some(seq) => seq,
            None => return,
        };

        let timestamp= match payload.timestamp{
            Some(ts) => ts,
            None => return,
        };

        match message_kind {
            MessageKind::Birth => self.handle_nbirth(id, seq, timestamp, payload.metrics),
            MessageKind::Death => self.handle_ndeath(id, seq, timestamp),
            MessageKind::Data => self.handle_ndata(id, seq, payload.metrics),
            _ => ()
        }
    }

    fn handle_device_message(&self, message: DeviceMessage) {
        let id = NodeIdentifier { group: message.group_id, node_id: message.node_id };
        let message_kind = message.message.kind;
        let payload = message.message.payload;
        let seq = match payload.seq {
            Some(seq) => seq,
            None => return,
        };

        let timestamp= match payload.timestamp{
            Some(ts) => ts,
            None => return,
        };
        let mut nodes = self.nodes.lock().unwrap();
        let node = match nodes.get_mut(&id) {
            Some(node) => node,
            None => {
                todo!("rebirth");
                return;
            },
        };
        node.validate_and_update_seq(seq);

        match message_kind {
            MessageKind::Birth => {
                match node.get_device(&message.device_id) {
                    Some(device_state) => {

                    },
                    None => {
                        let device_state = DeviceState::new(timestamp); 
                        node.add_device(message.device_id, device_state);
                    },
                }
            },
            MessageKind::Death => todo!(),
            MessageKind::Data => todo!(),
            _ => () 
        }

    }

}

pub struct App {
    host_id: String,
    subscription_config: AppSubscriptionConfig,
    client: Arc<DynClient>,
    eventloop: Box<DynEventLoop>,
    state: State 
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
            state: State::new()
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


    async fn handle_event(&mut self, event: Option<Event>) 
    {
        if let Some (event) = event {
            match event {
                Event::Online => self.on_online(),
                Event::Offline => self.on_offline(), 
                Event::Node(node_message) => {
                   self.state.handle_node_message(node_message); 
                },
                Event::Device(device_message) => {
                   self.state.handle_device_message(device_message); 
                },
                Event::State { host_id, payload } => (),
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
