use std::{collections::HashMap, future::Future, pin::Pin, sync::{Arc, Mutex}};

use log::{debug, info, warn};
use srad_client::{Client, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, MessageKind, NodeMessage};
use srad_types::{constants::NODE_CONTROL_REBIRTH, payload::{Payload, ToMetric}, topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter}, utils::timestamp, MetricId};
use tokio::task;

use crate::{config::SubscriptionConfig, metrics::{get_metric_birth_details_from_birth_metrics, get_metric_id_and_details_from_payload_metrics, MetricBirthDetails, MetricDetails, PublishMetric}};

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
   devices: HashMap<String, DeviceState>  
}

enum SeqState {
    OrderGood
}

impl NodeState {

    fn new(seq: u64, birth_timestamp: u64) -> Self {
        Self {
            seq, birth_timestamp, death_timestamp: 0, devices: HashMap::new()
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
    pub group: String,
    pub node_id: String
}

#[derive(Debug)]
pub enum RebirthReason {
    UnknownNode(NodeIdentifier),
    UnknownDevice{node_id: NodeIdentifier, device_id:String}
}

struct State {
    nodes: Mutex<HashMap<NodeIdentifier, NodeState>>,
}

impl State {

    fn new() -> Self {
        State { nodes: Mutex::new(HashMap::new()) }
    }

    fn handle_node_message(&self, message: NodeMessage, callbacks: &Callbacks) -> Option<RebirthReason> {
        let id = NodeIdentifier { group: message.group_id, node_id: message.node_id };
        let message_kind = message.message.kind;
        let payload = message.message.payload;
        let seq = match payload.seq {
            Some(seq) => seq,
            None => {
                warn!("Message did not contain a seq number - discarding. node = {:?}", id);
                return None
            },
        };

        let timestamp= match payload.timestamp{
            Some(ts) => ts,
            None => {
                warn!("Message did not contain a timestamp - discarding. node = {:?}", id);
                return None
            },
        };

        match message_kind {
            MessageKind::Birth => {
                let mut nodes = self.nodes.lock().unwrap();
                match nodes.get_mut(&id) {
                    Some(node) => {
                        node.validate_and_update_seq(seq);
                        if !node.validate_and_update_birth_timestamp(timestamp) {
                            debug!("Birth message was older than the most recent birth or death message - Discarding. node = {:?}", id);
                            return None
                        }
                    },
                    None => {
                        let node = NodeState::new(seq, timestamp);
                        nodes.insert(id.clone(), node);
                    },
                };

                let metric_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(e) => {
                        warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
                        return None
                    },
                };

                if let Some(callback) = &callbacks.nbirth { callback(id, timestamp, metric_details) };
            },
            MessageKind::Death => {
                let mut nodes = self.nodes.lock().unwrap();
                let node = match nodes.get_mut(&id) {
                    Some(node) => node,
                    None => return None,
                };

                node.validate_and_update_seq(seq);
                if !node.validate_and_update_death_timestamp(timestamp) { 
                    debug!("Death message was older than the most recent birth or death message - Discarding. node = {:?}", id);
                    return None 
                };
                if let Some(callback) = &callbacks.ndeath { callback(id, timestamp) };
            },
            MessageKind::Data => {
                let mut nodes = self.nodes.lock().unwrap();
                let node = match nodes.get_mut(&id) {
                    Some(node) => node,
                    None => return Some(RebirthReason::UnknownNode(id)),
                };

                node.validate_and_update_seq(seq);
                drop(nodes);

                let details = match get_metric_id_and_details_from_payload_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(e) => {
                        warn!("Message payload was invalid - {:?}. node = {:?}", e, id);
                        return None
                    },
                };

                if let Some(callback) = &callbacks.ndata { 
                    let callback =  callback.clone();
                    task::spawn(callback(id, timestamp, details));
                };
            },
            _ => ()  
        };
        return None
    }

    fn handle_device_message(&self, message: DeviceMessage, callbacks: &Callbacks) -> Option<RebirthReason> {
        let id = NodeIdentifier { group: message.group_id, node_id: message.node_id };
        let device_id = message.device_id;
        let message_kind = message.message.kind;
        let payload = message.message.payload;
        let seq = match payload.seq {
            Some(seq) => seq,
            None => {
                warn!("Message did not contain a seq number - discarding. device = {:?} node = {:?}", device_id, id);
                return None
            },
        };

        let timestamp= match payload.timestamp{
            Some(ts) => ts,
            None => {
                warn!("Message did not contain a timestamp - discarding. device = {:?} node = {:?}", device_id, id);
                return None
            },
        };

        let mut nodes = self.nodes.lock().unwrap();
        let node = match nodes.get_mut(&id) {
            Some(node) => node,
            None => {
                return Some(RebirthReason::UnknownNode(id));
            },
        };
        node.validate_and_update_seq(seq);

        match message_kind {
            MessageKind::Birth => {
                match node.get_device(&device_id) {
                    Some(device_state) => {
                        if !device_state.validate_and_update_birth_timestamp(timestamp) { 
                            debug!("Birth message was older than the most recent birth or death message - Discarding. node = {:?}, device = {}", id, device_id);
                            return None 
                        }
                    },
                    None => {
                        let device_state = DeviceState::new(timestamp); 
                        node.add_device(device_id.clone(), device_state);
                    },
                }
                let metric_details = match get_metric_birth_details_from_birth_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(e) => {
                        warn!("Message payload was invalid - {:?}. node = {:?}, device = {}", e, id, device_id);
                        return None
                    },
                };
                if let Some(callback) = &callbacks.dbirth {
                   callback(id, device_id, timestamp, metric_details); 
                }
            },
            MessageKind::Death => {
                let device_state = match node.get_device(&device_id) {
                    Some(state) => state,
                    None => return None,
                };
                if !device_state.validate_and_update_death_timestamp(timestamp) { 
                    debug!("Death message was older than the most recent birth or death message - Discarding. node = {:?}, device = {}", id, device_id);
                    return None 
                };
                if let Some(callback) = &callbacks.ddeath{
                   callback(id, device_id, timestamp); 
                }
            },
            MessageKind::Data => {
                let device_state = match node.get_device(&device_id) {
                    Some(state) => state,
                    None => return Some(RebirthReason::UnknownDevice{node_id: id, device_id}),
                };
                if !device_state.validate_and_update_death_timestamp(timestamp) { return None };
                drop(nodes); 
                let details = match get_metric_id_and_details_from_payload_metrics(payload.metrics) {
                    Ok(details) => details,
                    Err(e) => {
                        warn!("Message payload was invalid - {:?}. node = {:?}, device = {}", e, id, device_id);
                        return None
                    },
                };

                if let Some(callback) = &callbacks.ddata { 
                    let callback =  callback.clone();
                    task::spawn(callback(id, device_id,timestamp, details));
                };
            },
            _ => () 
        }
        return None;
    }

}

#[derive(Clone)]
pub struct AppClient(Arc<DynClient>);

#[derive(Debug, Clone)]
enum PublishTopicKind {
    NodeTopic(NodeTopic),
    DeviceTopic(DeviceTopic) 
}

#[derive(Debug, Clone)]
pub struct PublishTopic(PublishTopicKind);

impl PublishTopic {

    pub fn new_device_cmd(group_id: &String, node_id: &String, device_id: &String) -> Self {
        PublishTopic(PublishTopicKind::DeviceTopic(DeviceTopic::new(&group_id, srad_types::topic::DeviceMessage::DCmd, node_id, device_id)))
    }

    pub fn new_node_cmd(group_id: &String, node_id: &String) -> Self {
        PublishTopic(PublishTopicKind::NodeTopic(NodeTopic::new(&group_id, srad_types::topic::NodeMessage::NCmd, node_id)))
    }

}

impl AppClient {
    pub async fn publish_node_rebirth(&self, group_id: &String, node_id: &String) 
    {
        let topic = PublishTopic::new_node_cmd(group_id, node_id);
        let rebirth_cmd = PublishMetric::new(MetricId::Name(NODE_CONTROL_REBIRTH.into()), true);
        self.publish_metrics(topic, vec![rebirth_cmd]).await
    }

    pub async fn publish_metrics(&self, topic: PublishTopic, metrics: Vec<PublishMetric>) {
        let mut payload_metrics = Vec::with_capacity(metrics.len());
        for x in metrics.into_iter() {
            payload_metrics.push(x.to_metric());
        }
        let payload = Payload {
            timestamp: Some(timestamp()),
            metrics: payload_metrics,
            seq: None,
            uuid: None,
            body: None,
        };
        match topic.0 {
            PublishTopicKind::NodeTopic(topic) => self.0.publish_node_message(topic, payload).await,
            PublishTopicKind::DeviceTopic(topic) => self.0.publish_device_message(topic, payload).await,
        };
    }
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
    evaluate_rebirth_reason: Option<Arc<dyn Fn(RebirthReason) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>>, 
}

pub struct App {
    host_id: String,
    subscription_config: SubscriptionConfig,
    client: AppClient,
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
    ) -> (Self, AppClient) {

        let callbacks = Callbacks {
            online: None,
            offline: None,
            nbirth: None,
            ndeath: None,
            ndata: None,
            dbirth: None,
            ddeath: None,
            ddata: None,
            evaluate_rebirth_reason: None,
        };

        let client = AppClient(Arc::new(client));
        let app = Self {
            host_id: host_id.into(),
            client: client.clone(),
            eventloop: Box::new(eventloop),
            subscription_config,
            state: State::new(),
            callbacks
        };
        (app, client)
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
    
    pub fn on_dbirth<F>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(NodeIdentifier, String, u64, Vec<(MetricBirthDetails, MetricDetails)>) -> () + 'static
    {
        self.callbacks.dbirth = Some(Box::pin(cb));
        self
    }

    pub fn on_ddeath<F>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(NodeIdentifier, String, u64) -> () + 'static
    {
        self.callbacks.ddeath = Some(Box::pin(cb));
        self
    }

    pub fn on_ddata<F, Fut>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(NodeIdentifier, String, u64, Vec<(MetricId,MetricDetails)>) -> Fut + Send + 'static,
        Fut: Future<Output=()> + Send + 'static
    {
        let callback = Arc::new(move |id, device, time, data| {
            Box::pin(cb(id, device, time, data)) as Pin<Box<dyn Future<Output = ()> + Send>> 
        }); 
        self.callbacks.ddata = Some(callback);
        self
    }

    pub fn register_evaluate_rebirth_reason_fn<F, Fut>(&mut self, cb: F) -> &mut Self
    where 
        F: Fn(RebirthReason) -> Fut + Send + 'static,
        Fut: Future<Output=()> + Send + 'static
    {
        let callback = Arc::new(move |reason| {
            Box::pin(cb(reason)) as Pin<Box<dyn Future<Output = ()> + Send>> 
        }); 
        self.callbacks.evaluate_rebirth_reason = Some(callback);
        self
    }

    fn update_last_will(&mut self) {
        self.eventloop.set_last_will(srad_client::LastWill::new_app(&self.host_id));
    }

    fn handle_online(&self) {
        info!("App Online");
        if let Some(callback) = &self.callbacks.online { callback() };
        let client = self.client.0.clone();
        let mut topics: Vec<TopicFilter> = self.subscription_config.clone().into();
        topics.push(TopicFilter::new_with_qos(Topic::State(StateTopic::new_host(&self.host_id)), QoS::AtMostOnce));
        task::spawn(async move {
           client.subscribe_many(topics).await
        });
    }

    fn handle_offline(&mut self) {
        info!("App Offline");
        if let Some(callback) = &self.callbacks.offline { callback() };
        self.update_last_will();
    }

    async fn handle_event(&mut self, event: Option<Event>) 
    {
        if let Some (event) = event {
            match event {
                Event::Online => self.handle_online(),
                Event::Offline => self.handle_offline(), 
                Event::Node(node_message) => {
                    if let Some(reason) = self.state.handle_node_message(node_message, &self.callbacks) {
                        if let Some(cb) = &self.callbacks.evaluate_rebirth_reason {
                            let cb = cb.clone();
                            task::spawn(cb(reason));
                        }
                    }
                },
                Event::Device(device_message) => {
                    if let Some(reason) = self.state.handle_device_message(device_message, &self.callbacks) {
                        if let Some(cb) = &self.callbacks.evaluate_rebirth_reason {
                            let cb = cb.clone();
                            task::spawn(cb(reason));
                        }
                    }
                },
                Event::State { host_id, payload } => (),
                Event::InvalidPublish { reason: _, topic: _, payload: _ } => (),
            }
        }
    }

    pub async fn run(&mut self) {
        info!("App Started");
        self.update_last_will();
        loop {
            let event = self.eventloop.poll().await;
            self.handle_event(event).await;
        }
        info!("App Stopped");
    } 

}
