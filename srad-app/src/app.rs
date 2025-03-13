use std::{collections::HashMap, future::Future, pin::Pin, process::Output, sync::{Arc, Mutex}};

use srad_client::{Client, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, MessageKind, NodeMessage};
use srad_types::{payload::Metric, topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter}, MetricId};
use tokio::task;

use crate::{config::SubscriptionConfig, metrics::{get_metric_birth_details_from_birth_metrics, get_metric_id_and_details_from_payload_metrics, MetricBirthDetails, MetricDetails}};

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

    fn get_device(&mut self, device_name: &String) -> Option<&mut DeviceState> {
        self.devices.get_mut(device_name)
    }

    fn add_device(&mut self, name: String, device: DeviceState) {
        self.devices.insert(name, device);
    }

}

#[derive(Debug,PartialEq, Eq, Hash, Clone)]
pub struct NodeIdentifier {
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

    fn handle_node_message(&self, message: NodeMessage, callbacks: &Callbacks) {
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
            MessageKind::Birth => {
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
                        nodes.insert(id.clone(), node);
                    },
                };

                let metric_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(_) => todo!("rebirth"),
                };

                if let Some(callback) = &callbacks.nbirth { callback(id, timestamp, metric_details) };
            },
            MessageKind::Death => {
                let mut nodes = self.nodes.lock().unwrap();
                let node = match nodes.get_mut(&id) {
                    Some(node) => node,
                    None => return,
                };

                node.validate_and_update_seq(seq);
                if !node.validate_and_update_death_timestamp(timestamp) { return };
                if let Some(callback) = &callbacks.ndeath { callback(id, timestamp) };
            },
            MessageKind::Data => {
                let mut nodes = self.nodes.lock().unwrap();
                let node = match nodes.get_mut(&id) {
                    Some(node) => node,
                    None => return,
                };

                node.validate_and_update_seq(seq);
                drop(nodes);

                let details = match get_metric_id_and_details_from_payload_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(_) => todo!("rebirth"),
                };

                if let Some(callback) = &callbacks.ndata { 
                    let callback =  callback.clone();
                    task::spawn(callback(id, timestamp, details));
                };
            },
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
                       if !device_state.validate_and_update_birth_timestamp(timestamp) { return }
                    },
                    None => {
                        let device_state = DeviceState::new(timestamp); 
                        node.add_device(message.device_id, device_state);
                    },
                }

                println!("metrics: {0:?}", payload.metrics);
                let metric_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(_) => todo!("rebirth"),
                };
                //callback
            },
            MessageKind::Death => {
                let device_state = match node.get_device(&message.device_id) {
                    Some(state) => state,
                    None => return,
                };
                if !device_state.validate_and_update_death_timestamp(timestamp) { return };
                //callback;
            },
            MessageKind::Data => {
                let device_state = match node.get_device(&message.device_id) {
                    Some(state) => state,
                    None => todo!("rebirth"),
                };
                if !device_state.validate_and_update_death_timestamp(timestamp) { return };
                drop(nodes); 
                let details = match get_metric_id_and_details_from_payload_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(_) => todo!("rebirth"),
                };
                //callback;
            },
            _ => () 
        }

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

struct Callbacks {
    online: Option<Pin<Box<dyn Fn() -> ()>>>,
    offline: Option<Pin<Box<dyn Fn() -> ()>>>, 
    nbirth: Option<Pin<Box<dyn Fn(NodeIdentifier, u64, Vec<(MetricBirthDetails, MetricDetails)>) -> ()>>>, 
    ndeath: Option<Pin<Box<dyn Fn(NodeIdentifier, u64) -> ()>>>, 
    ndata: Option<Arc<dyn Fn(NodeIdentifier, u64, Vec<(MetricId, MetricDetails)>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>>, 
    dbirth: Option<Pin<Box<dyn Fn(NodeIdentifier, String, u64, Vec<(MetricBirthDetails, MetricDetails)>) -> ()>>>, 
    ddeath: Option<Pin<Box<dyn Fn(NodeIdentifier, String, u64) -> ()>>>, 
    ddata: Option<Arc<dyn Fn(NodeIdentifier, String, u64, Vec<(MetricId, MetricDetails)>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>>, 
}

pub struct App {
    host_id: String,
    subscription_config: SubscriptionConfig,
    client: Arc<DynClient>,
    eventloop: Box<DynEventLoop>,
    state: State,
    callbacks: Callbacks,
}

impl App {

    pub fn new<S: Into<String>, E: EventLoop + Send +'static, C: Client + Send + Sync + 'static>(
        host_id: S,
        subscription_config: SubscriptionConfig,
        eventloop: E,
        client: C
    ) -> Self {

        let callbacks = Callbacks {
            online: None,
            offline: None,
            nbirth: None,
            ndeath: None,
            ndata: None,
            dbirth: None,
            ddeath: None,
            ddata: None
        };

        Self {
            host_id: host_id.into(),
            client: Arc::new(client),
            eventloop: Box::new(eventloop),
            subscription_config,
            state: State::new(),
            callbacks
        }
    }

    pub fn on_online<F>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn() -> () + 'static
    {
        self.callbacks.online = Some(Box::pin(cb));
        self
    }

    pub fn on_offline<F>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn() -> () + 'static
    {
        self.callbacks.offline = Some(Box::pin(cb));
        self
    }

    pub fn on_nbirth<F>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(NodeIdentifier, u64, Vec<(MetricBirthDetails, MetricDetails)>) -> () + 'static
    {
        self.callbacks.nbirth = Some(Box::pin(cb));
        self
    }

    pub fn on_ndeath<F>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(NodeIdentifier, u64) -> () + 'static
    {
        self.callbacks.ndeath = Some(Box::pin(cb));
        self
    }

    pub fn on_ndata<F, Fut>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(NodeIdentifier, u64, Vec<(MetricId,MetricDetails)>) -> Fut + Send + 'static,
        Fut: Future<Output=()> + Send + 'static
    {
        let callback = Arc::new(move |id, time, data| {
            Box::pin(cb(id, time, data)) as Pin<Box<dyn Future<Output = ()> + Send>> 
        }); 
        self.callbacks.ndata = Some(callback);
        self
    }

    fn update_last_will(&mut self) {
        self.eventloop.set_last_will(srad_client::LastWill::new_app(&self.host_id));
    }

    fn handle_online(&self) {
        if let Some(callback) = &self.callbacks.online { callback() };
        let client = self.client.clone();
        let mut topics: Vec<TopicFilter> = self.subscription_config.clone().into();
        topics.push(TopicFilter::new_with_qos(Topic::State(StateTopic::new_host(&self.host_id)), QoS::AtMostOnce));
        task::spawn(async move {
           client.subscribe_many(topics).await
        });
    }

    fn handle_offline(&mut self) {
        if let Some(callback) = &self.callbacks.offline { callback() };
        self.update_last_will();
    }

    async fn handle_event(&mut self, event: Option<Event>) 
    {
        if let Some (event) = event {
            match event {
                Event::Online => self.handle_online(),
                Event::Offline => self.handle_offline(), 
                Event::Node(node_message) => self.state.handle_node_message(node_message, &self.callbacks),
                Event::Device(device_message) => self.state.handle_device_message(device_message),
                Event::State { host_id, payload } => (),
                Event::InvalidPublish { reason: _, topic: _, payload: _ } => (),
            }
        }
    }

    pub async fn run(&mut self) {
        self.update_last_will();
        loop {
            let event = self.eventloop.poll().await;
            self.handle_event(event).await;
        }
    } 

}
