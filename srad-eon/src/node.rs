use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use srad_client::{DeviceMessage, DynClient, DynEventLoop, MessageKind};
use srad_client::{Event, NodeMessage, Message};

use srad_types::constants::NODE_CONTROL_REBIRTH;
use srad_types::payload::metric::Value;
use srad_types::payload::ToMetric;
use srad_types::topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter};
use srad_types::utils::timestamp;
use srad_types::{
  constants,
  topic::{DeviceMessage as DeviceMessageType, NodeMessage as NodeMessageType},
  payload::Payload
};

use crate::builder::EoNBuilder;
use crate::device::{DeviceHandle, DeviceMap};
use crate::error::SpgError;
use crate::metric::{MessageMetrics, MetricPublisher, PublishError, PublishMetric};
use crate::metric_manager::birth::{BirthInitializer, BirthMetricDetails};
use crate::metric_manager::manager::{DeviceMetricManager, DynNodeMetricManager};
use crate::registry::{MetricRegistry, MetricValidToken};
use crate::BirthType;

use super::registry;

use tokio::{select, task};
use flume::{bounded, Sender, Receiver};

struct EoNShutdown;

#[derive(Clone)]
pub struct NodeHandle {
  node: Arc<Node>
}

impl NodeHandle {

  pub async fn cancel(&self){
    self.node.client.disconnect().await;
    self.node.stop_tx.send(EoNShutdown).unwrap();
  }

  pub async fn rebirth(&self){
    self.node.rebirth().await;
  }

  pub fn register_device<S, M>(&self, name: S, dev_impl: M) -> Result<DeviceHandle, SpgError> 
  where 
    S: Into<String>,
    M: DeviceMetricManager + Send + Sync + 'static 
  {
    let handle = self.node.devices.add_device(
      &self.node.state.group_id,
      &self.node.state.edge_node_id, 
      name.into(),
      Arc::new(dev_impl)
    )?;
    Ok(handle)
  }

  pub fn deregister_device(){}

}

impl MetricPublisher for NodeHandle
{
  async fn publish_metrics_unsorted(&self, metrics: Vec<PublishMetric>) -> Result<(), PublishError> {
    if metrics.len() == 0 { return Err(PublishError::NoMetrics) }

    let timestamp = timestamp();
    let metric_ptr = {
      let metrics_valid_tok = self.node.metrics_valid_token.lock().unwrap();
      if !metrics_valid_tok.is_valid() {
        return Err(PublishError::InvalidMetric)
      }
      metrics_valid_tok.token_ptr()
    };

    let mut payload_metrics = Vec::with_capacity(metrics.len());
    for x in metrics.into_iter() {
      if *x.get_token_ptr() != metric_ptr { return Err(PublishError::InvalidMetric) }
      payload_metrics.push(x.to_metric());
    }

    let payload = Payload { 
      timestamp: Some(timestamp), 
      metrics: payload_metrics, 
      seq: Some(self.node.state.get_seq()), 
      uuid: None, 
      body: None 
    };

    self.node.client.publish_node_message(self.node.state.ndata_topic.clone(), payload).await;
    Ok(())
  }
}

pub struct EoNState {
  bdseq: AtomicU8,
  seq: AtomicU8,
  online: AtomicBool,
  birthed: AtomicBool,
  pub group_id: String,
  pub edge_node_id: String,
  pub ndata_topic: NodeTopic,
}

impl EoNState{
  pub fn get_seq(&self) -> u64 {
    self.seq.fetch_add(1, Ordering::Relaxed) as u64
  }

  pub fn is_online(&self) -> bool {
    self.online.load(Ordering::SeqCst)
  }

  pub fn set_online(&self, online: bool)
  {
    self.online.store(online, Ordering::SeqCst)
  }

  pub fn birthed(&self) -> bool {
    self.birthed.load(Ordering::SeqCst)
  }

  pub fn set_birthed(&self, online: bool)
  {
    self.birthed.store(online, Ordering::SeqCst)
  }

  pub fn birth_topic(&self) -> NodeTopic {
    NodeTopic::new(&self.group_id, NodeMessageType::NBirth, &self.edge_node_id)
  }

  pub fn death_topic(&self) -> NodeTopic {
    NodeTopic::new(&self.group_id, NodeMessageType::NDeath, &self.edge_node_id)
  }

  pub fn sub_topics(&self) -> Vec<TopicFilter> {
    vec![
      TopicFilter::new_with_qos(Topic::NodeTopic(NodeTopic::new(&self.group_id, NodeMessageType::NCmd, &self.edge_node_id)), QoS::AtLeastOnce),
      TopicFilter::new_with_qos(Topic::DeviceTopic(DeviceTopic::new(&self.group_id, DeviceMessageType::DCmd, &self.edge_node_id, "+")), QoS::AtLeastOnce),
      TopicFilter::new_with_qos(Topic::State(StateTopic::new()), QoS::AtLeastOnce)
    ]
  }
}

pub struct Node {
  state: Arc<EoNState>, 
  metric_manager: Box<DynNodeMetricManager>,
  metrics_valid_token: Mutex<MetricValidToken>,
  devices: DeviceMap,
  registry: Arc<Mutex<registry::MetricRegistry>>, 
  client: Arc<DynClient>,
  stop_tx: Sender<EoNShutdown>
}

impl Node {

  fn generate_birth_payload(&self, bdseq: i64, seq: u64) -> Payload {
    let timestamp = timestamp();
    let mut reg = self.registry.lock().unwrap();
    reg.clear();

    let mut birth_initializer = BirthInitializer::new(registry::MetricRegistryInserterType::Node, &mut reg);
    birth_initializer.create_metric(
      BirthMetricDetails::new_with_initial_value(constants::BDSEQ,  bdseq).use_alias(false)
    ).unwrap();
    birth_initializer.create_metric(
      BirthMetricDetails::new_with_initial_value(constants::NODE_CONTROL_REBIRTH,  false).use_alias(false)
    ).unwrap();

    self.metric_manager.initialize_birth(&mut birth_initializer);
    let (metrics, metric_valid_token) = birth_initializer.finish();

    {
      let mut valid_token = self.metrics_valid_token.lock().unwrap();
      *valid_token = metric_valid_token 
    }

    Payload {
      seq: Some(0),
      timestamp: Some (timestamp),
      metrics: metrics,
      uuid : None,
      body: None
    }
  }

  async fn node_birth(&self) {

    /* [tck-id-topics-nbirth-seq-num] The NBIRTH MUST include a sequence number in the payload and it MUST have a value of 0. */
    self.state.seq.store(0, Ordering::SeqCst);
    let bdseq= self.state.bdseq.load(Ordering::SeqCst) as i64;

    let payload = self.generate_birth_payload(bdseq, 0);
    let topic = self.state.birth_topic();
    self.state.seq.store(1, Ordering::SeqCst);
    self.client.publish_node_message(topic, payload).await;
  }

  async fn birth(&self, birth:BirthType) {
    self.node_birth().await;
    self.state.set_birthed(true);
    self.devices.birth_devices(birth).await;
  }

  async fn rebirth(&self) 
  {
    /* while node state birthed is false device handle and node handle will not publish any metrics */
    self.state.set_birthed(false);
    self.birth(BirthType::Rebirth).await
  }
}

pub struct EoN 
{
  node: Arc<Node>,
  eventloop: Box<DynEventLoop>,
  stop_rx: Receiver<EoNShutdown>
}

impl EoN 
{

  pub fn new_from_builder(builder: EoNBuilder) -> Result<(Self, NodeHandle), String>
  {
    let group_id = builder.group_id.ok_or("group id must be provided".to_string())?;
    let node_id = builder.node_id.ok_or("node id must be provided".to_string())?;
    let metric_manager = builder.metric_manager;
    let (eventloop, client) = builder.eventloop_client;

    let (stop_tx, stop_rx) = bounded(1);

    let state = Arc::new(EoNState{
      seq: AtomicU8::new(0), 
      bdseq: AtomicU8::new(0),
      online: AtomicBool::new(false), 
      birthed: AtomicBool::new(false),
      ndata_topic:NodeTopic::new(&group_id, NodeMessageType::NData, &node_id),
      group_id: group_id,
      edge_node_id: node_id,
    });

    let registry = Arc::new(Mutex::new(MetricRegistry::new()));
    let node = Arc::new(Node {
      metric_manager,
      client: client.clone(),
      registry: registry.clone(),
      devices: DeviceMap::new(state.clone(), registry.clone(), client),
      metrics_valid_token: Mutex::new(MetricValidToken::new()),
      state,
      stop_tx
    });

    let mut eon = Self {
      node: node,
      eventloop,
      stop_rx
    };
    let handle = NodeHandle {
      node: eon.node.clone(),
    };
    eon.update_last_will();
    Ok((eon, handle))
  }

  fn update_last_will(&mut self){
    self.eventloop.set_last_will(
      srad_client::LastWill::new_node(
        &self.node.state.group_id, 
        &self.node.state.edge_node_id, 
        self.node.state.bdseq.load(Ordering::SeqCst) 
      )
    );
  }

  fn on_online(&self) {
    self.node.state.set_online(true);

    let sub_topics = self.node.state.sub_topics();
    let node = self.node.clone();

    tokio::spawn(async move {
      node.client.subscribe_many(sub_topics).await;
      node.birth(BirthType::Birth).await
    });
  }

  fn on_offline(&mut self) {
    self.node.state.set_online(false);
    self.node.devices.death_devices();
    self.node.state.bdseq.fetch_add(1, Ordering::SeqCst);
    self.update_last_will();
  }

  fn on_node_message(&self, message: NodeMessage) {
    let payload = message.message.payload;
    let message_kind = message.message.kind;
    match message_kind {
      MessageKind::Cmd => {

        let mut rebirth = false;
        for x in &payload.metrics {
          if x.alias.is_some() {continue;}
          let metric_name = match &x.name  {
            Some(name) => name,
            None => continue,
          };

          if metric_name != NODE_CONTROL_REBIRTH {continue;} 

          match &x.value {
            Some(value) => {
              if let Value::BooleanValue(val) = value {
                if *val == true { rebirth = true; } 
                else {
                  //todo err
                }
              } else {
                //todo err
              }
              break;
            },
            None => {
              //todo error
              break;
            },
          }
        } 

        let node= self.node.clone();
        let message_metrics:MessageMetrics = match payload.try_into() {
          Ok(metrics) => metrics,
          Err(_) => todo!(),
        };

        task::spawn( async move {
          node.metric_manager.on_ncmd(NodeHandle { node: node.clone() }, message_metrics).await;
          if rebirth { node.rebirth().await }
        });
      },
      _ => ()
    }
  }

  fn on_device_message(&self, message: DeviceMessage)
  {
    let node = self.node.clone();
    task::spawn(async move {
      node.devices.handle_device_message(message).await;
    });
  }

  async fn handle_event(&mut self, event: Option<Event>) 
  {
    if let Some (event) = event {
      match event {
        Event::Online => self.on_online(),
        Event::Offline => self.on_offline(), 
        Event::Node(node_message) => self.on_node_message(node_message),
        Event::Device(device_message) => self.on_device_message(device_message),
        Event::State{ host_id, payload } => (),
        Event::InvalidPublish { reason: _, topic: _, payload: _ } => (),
      }
    }
  }

  pub async fn run(&mut self) {
    self.update_last_will();
    loop {
      select!{
        event = self.eventloop.poll() => self.handle_event(event).await,
        Ok(_) = self.stop_rx.recv_async() => break,
      }
    }
  } 

}
