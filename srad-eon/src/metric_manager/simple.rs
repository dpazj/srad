use std::{collections::HashMap, future::Future, ops::DerefMut, pin::Pin, sync::{Arc, Mutex}};
use async_trait::async_trait;
use futures::future::join_all;
use srad_types::{traits, MetricId};
use crate::{device::DeviceHandle, metric::{MessageMetric, MessageMetrics, MetricPublisher, MetricToken, PublishMetric}, NodeHandle};
use super::{birth::{BirthInitializer, BirthMetricDetails}, manager::{DeviceMetricManager, MetricManager, NodeMetricManager}};

type CmdCallback<T, H> = Arc<dyn Fn(H, SimpleManagerMetric<T, H>, Option<T>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

struct MetricData<T, H> {
  value: T,
  token: Option<MetricToken<T>>,
  cb: Option<CmdCallback<T, H>>
}

#[derive(Clone)]
pub struct SimpleManagerMetric<T, H>{
  data: Arc<Mutex<MetricData<T, H>>>
}

impl<T, H> SimpleManagerMetric<T, H> 
where T:  traits::MetricValue + Clone
{
  pub fn update<F1>(&self, f: F1) -> Option<PublishMetric>
  where 
    F1: Fn(&mut T) 
  {
    let mut guard = self.data.lock().unwrap();
    let x = guard.deref_mut();
    f(&mut x.value);
    match &x.token {
      Some(h) => Some(h.create_publish_metric(Some(x.value.clone()))),
      None => None,
    }
  }

  pub async fn update_and_publish<F1, M: MetricPublisher>(&self, f: F1, publisher: &M) 
  where 
    F1: Fn(&mut T) 
  { 
    let metric = self.update(f);
    if let Some(metric) = metric {
      publisher.publish_metrics(vec![metric]).await.unwrap();
    }
  }

}

#[async_trait]
trait Stored<H>: Send {
  fn birth_metric(&self, name:&String, bi: &mut BirthInitializer) -> MetricId;
  fn has_callback(&self) -> bool;
  async fn cmd_cb(&self, handle:H, value: MessageMetric);
}

#[async_trait]
impl<T, H> Stored<H> for SimpleManagerMetric<T, H>
where 
  T: traits::MetricValue + Clone + Send,
  H: Send + Sync
{

  fn birth_metric(&self, name: &String, bi: &mut BirthInitializer) -> MetricId {
    let mut metric = self.data.lock().unwrap();
    let val = metric.value.clone(); 
    let token = bi.create_metric(
      BirthMetricDetails::new_with_initial_value(name, val).use_alias(true)
    ).unwrap();
    let id = token.id().clone();
    metric.token = Some(token);
    id
  }

  fn has_callback(&self) -> bool {
    self.data.lock().unwrap().cb.is_some()
  }

  async fn cmd_cb(&self, handle:H, value: MessageMetric) {
    let (cb, converted) = {
      let metric = self.data.lock().unwrap();
      let cb = match &metric.cb {
        Some(cb) => cb.clone(),
        None => return,
      };

      let converted = match value.value {
        Some(v) => match v.try_into() {
          Ok(value) => Some(value),
          Err(_) => todo!(),
        },
        None => todo!(),
      };

      (cb, converted)
    };
    let x = SimpleManagerMetric { data: self.data.clone() };
    cb(handle, x, converted).await
  }
}

pub struct SimpleMetricManagerInner<H> {
  metrics : HashMap<String, Arc<dyn Stored<H> + Send + Sync>>,
  cmd_lookup: HashMap<MetricId, Arc<dyn Stored<H> + Send + Sync>>
}

#[derive(Clone)]
pub struct SimpleMetricManager<H> {
  inner: Arc<Mutex<SimpleMetricManagerInner<H>>>
}

impl<H> SimpleMetricManager<H> 
where 
  H: Clone + Send + Sync + 'static
{

  pub fn new() -> Self {
    Self {
      inner: Arc::new(Mutex::new(SimpleMetricManagerInner {
        metrics: HashMap::new(),
        cmd_lookup: HashMap::new()
      }))
    }
  }

  fn register<T>(&self, name : String, value: T, cb: Option<CmdCallback<T, H>>) -> Option<SimpleManagerMetric<T, H>>
  where 
    T : traits::MetricValue + Clone + Send + 'static
  {
    let key  = name.into();
    let mut manager= self.inner.lock().unwrap();
    if manager.metrics.contains_key(&key){
      return None;
    }

    let metric= SimpleManagerMetric { 
      data: Arc::new(Mutex::new(MetricData {
        value, 
        token: None,
        cb: cb
      }))
    };
    let metric_insert = Arc::new(metric.clone());
    manager.metrics.insert(key, metric_insert);
    Some(metric)
  }

  pub fn register_metric<S, T>(&self, name : S, value: T) -> Option<SimpleManagerMetric<T, H>>
  where 
    S : Into<String>,
    T : traits::MetricValue + Clone + Send + 'static, 
  {
    self.register::<T>(name.into(), value, None) 
  }

  pub fn register_metric_with_cmd_handler<S, T, F, Fut>(&self, name : S, value: T, cmd_handler: F) -> Option<SimpleManagerMetric<T,H>>
  where 
    S : Into<String>,
    T : traits::MetricValue + Clone + Send + 'static, 
    F : Fn(H, SimpleManagerMetric<T,H>, Option<T>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static
  {
    let cmd_handler = Arc::new(move |handle: H, metric:SimpleManagerMetric<T, H>, value: Option<T>| -> Pin<Box<dyn Future<Output = ()> + Send>> {
      Box::pin(cmd_handler(handle, metric, value))
    });
    self.register::<T>(name.into(), value, Some(cmd_handler)) 
  }

  fn get_callbacks_from_cmd_message_metrics(&self, metrics: MessageMetrics) -> Vec<(Arc<dyn Stored<H> + Send + Sync>, MessageMetric)> {
    let manager = self.inner.lock().unwrap();
    let mut cbs = Vec::with_capacity(metrics.len());
    for metric in metrics {
      match manager.cmd_lookup.get(&metric.id) {
        Some(res) => cbs.push((res.clone(), metric)),
        None => continue ,
      };
    }
    cbs
  }

  async fn handle_cmd_metrics(&self, handle:H, metrics: MessageMetrics) {
    let callbacks = self.get_callbacks_from_cmd_message_metrics(metrics);
    let futures: Vec<Pin<Box<dyn Future<Output = ()> + Send>>> = callbacks.into_iter().map(|(stored, value)| {
      Box::pin({
        let handle= handle.clone();
        async move { stored.cmd_cb(handle, value).await }
      }) as Pin<Box<dyn Future<Output = ()> + Send>>
    }).collect();
    join_all(futures).await;
  }

}


impl<H> MetricManager for SimpleMetricManager<H> {

  fn initialize_birth(&self, bi: &mut super::birth::BirthInitializer) {
    let mut manager = self.inner.lock().unwrap();

    let mut cmd_lookup = vec![];
    manager.metrics.iter_mut().for_each(|(i, x)| {
      let id = x.birth_metric(i, bi);
      if x.has_callback() { cmd_lookup.push((id, x.clone())); }
    });
    manager.cmd_lookup = cmd_lookup.into_iter().collect(); 
  }

}


#[async_trait]
impl NodeMetricManager for SimpleMetricManager<NodeHandle> {
  async fn on_ncmd(&self, node: NodeHandle, metrics: MessageMetrics) {
    self.handle_cmd_metrics(node, metrics).await
  }
}

#[async_trait]
impl DeviceMetricManager for SimpleMetricManager<DeviceHandle> {
  async fn on_dcmd(&self, device: DeviceHandle, metrics: MessageMetrics) {
    self.handle_cmd_metrics(device, metrics).await
  }
}
