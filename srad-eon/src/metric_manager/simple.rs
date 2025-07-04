use super::manager::{DeviceMetricManager, MetricManager, NodeMetricManager};
use crate::{
    birth::{BirthInitializer, BirthMetricDetails},
    device::DeviceHandle,
    metric::{
        MessageMetric, MessageMetrics, MetricPublisher, MetricToken, PublishError, PublishMetric,
    },
    NodeHandle,
};
use async_trait::async_trait;
use futures::future::join_all;
use srad_types::{traits, MetricId};
use std::{
    collections::HashMap,
    future::Future,
    ops::DerefMut,
    pin::Pin,
    sync::{Arc, Mutex},
};

type CmdCallback<T, H> = Arc<
    dyn Fn(
            SimpleMetricManager<H>,
            SimpleManagerMetric<T, H>,
            Option<T>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

struct MetricData<T, H> {
    value: T,
    token: Option<MetricToken<T>>,
    cb: Option<CmdCallback<T, H>>,
    use_alias: bool,
}

pub struct SimpleManagerPublishMetric(Option<PublishMetric>);
#[derive(Clone)]
pub struct SimpleManagerMetric<T, H> {
    data: Arc<Mutex<MetricData<T, H>>>,
}

impl<T, H> SimpleManagerMetric<T, H>
where
    T: traits::MetricValue + Clone,
{
    pub fn update<F1>(&self, f: F1) -> SimpleManagerPublishMetric
    where
        F1: Fn(&mut T),
    {
        let mut guard = self.data.lock().unwrap();
        let x = guard.deref_mut();
        f(&mut x.value);
        let option = match &x.token {
            Some(h) => Some(h.create_publish_metric(Some(x.value.clone()))),
            None => None,
        };
        SimpleManagerPublishMetric(option)
    }
}

#[async_trait]
trait Stored<H>: Send {
    fn birth_metric(&self, name: &str, bi: &mut BirthInitializer) -> MetricId;
    fn has_callback(&self) -> bool;
    async fn cmd_cb(&self, manager: SimpleMetricManager<H>, value: MessageMetric);
}

#[async_trait]
impl<T, H> Stored<H> for SimpleManagerMetric<T, H>
where
    T: traits::MetricValue + Clone + Send,
    H: Send + Sync,
{
    fn birth_metric(&self, name: &str, bi: &mut BirthInitializer) -> MetricId {
        let mut metric = self.data.lock().unwrap();
        let val = metric.value.clone();
        let token = bi
            .register_metric(
                BirthMetricDetails::new_with_initial_value(name, val).use_alias(metric.use_alias),
            )
            .unwrap();
        let id = token.id.clone();
        metric.token = Some(token);
        id
    }

    fn has_callback(&self) -> bool {
        self.data.lock().unwrap().cb.is_some()
    }

    async fn cmd_cb(&self, manager: SimpleMetricManager<H>, value: MessageMetric) {
        let cb = {
            let metric = self.data.lock().unwrap();
            match &metric.cb {
                Some(cb) => cb.clone(),
                None => return,
            }
        };

        let converted = match value.value {
            Some(v) => match v.try_into() {
                Ok(value) => Some(value),
                Err(_) => return,
            },
            None => None,
        };

        let x = SimpleManagerMetric {
            data: self.data.clone(),
        };
        cb(manager, x, converted).await
    }
}

/// A Metric Builder to configure a metric to add to a [SimpleMetricManager]
pub struct SimpleMetricBuilder<T, H> {
    name: String,
    initial_value: T,
    use_alias: bool,
    cmd_cb: Option<CmdCallback<T, H>>,
}

impl<T, H> SimpleMetricBuilder<T, H>
where
    T: traits::MetricValue + Clone + Send + 'static,
{
    /// Create a new metric builder instance
    pub fn new<S: Into<String>>(name: S, value: T) -> Self {
        Self {
            name: name.into(),
            initial_value: value,
            use_alias: true,
            cmd_cb: None,
        }
    }

    /// Set a handler which will be called when command messages for this metric are received.
    ///
    /// The command handler is an async function that receives the manager, the metric,
    /// and an optional new value for the metric. If the value is "None" that indicates the CMD metric's 'is_null'
    /// field was true
    pub fn with_cmd_handler<F, Fut>(mut self, cb: F) -> Self
    where
        F: Fn(SimpleMetricManager<H>, SimpleManagerMetric<T, H>, Option<T>) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.cmd_cb = Some(Arc::new(
            move |manager: SimpleMetricManager<H>,
                  metric: SimpleManagerMetric<T, H>,
                  value: Option<T>|
                  -> Pin<Box<dyn Future<Output = ()> + Send>> {
                Box::pin(cb(manager, metric, value))
            },
        ));
        self
    }

    /// Set if the metric should use an alias. Defaults to true.
    pub fn use_alias(mut self, use_alias: bool) -> Self {
        self.use_alias = use_alias;
        self
    }
}

struct SimpleMetricManagerInner<H> {
    handle: Option<H>,
    metrics: HashMap<String, Arc<dyn Stored<H> + Send + Sync>>,
    cmd_lookup: HashMap<MetricId, Arc<dyn Stored<H> + Send + Sync>>,
}

/// A [MetricManager] implementation that provides simple metric registration and handling.
///
/// `SimpleMetricManager` provides a simple way to manage metrics with support for
/// command callbacks and metric publishing.
/// # Example
/// ```no_run
/// use srad_eon::{SimpleMetricManager, SimpleMetricBuilder};
/// # use srad_eon::DeviceHandle;
///
/// # fn create_device_with_manager(manager: &SimpleMetricManager<DeviceHandle>) {
/// #   unimplemented!()
/// # }
/// //Create a new simple metric manager
/// let manager = SimpleMetricManager::new();
///
/// // Assume we successfully create a device with a SimpleMetricManager as it's metrics manager
/// create_device_with_manager(&manager);
///
/// // Register a simple metric
/// let counter = manager.register_metric(SimpleMetricBuilder::new("Counter", 0 as i32)).unwrap();
///
/// // Register a metric with a command handler
/// let temperature = manager.register_metric(
///     SimpleMetricBuilder::new("temperature", 25.5)
///     .with_cmd_handler(
///         |mgr, metric, new_value| async move {
///             if let Some(value) = new_value {
///                 mgr.publish_metric(metric.update(|x|{ *x = value })).await;
///             }
///         }
///     )
/// );
/// ```
#[derive(Clone)]
pub struct SimpleMetricManager<H> {
    inner: Arc<Mutex<SimpleMetricManagerInner<H>>>,
}

impl<H> SimpleMetricManager<H>
where
    H: MetricPublisher + Clone + Send + Sync + 'static,
{
    /// Creates a new empty `SimpleMetricManager`.
    ///
    /// This initialises a new metric manager with no registered metrics.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SimpleMetricManagerInner {
                handle: None,
                metrics: HashMap::new(),
                cmd_lookup: HashMap::new(),
            })),
        }
    }

    /// Registers a new metric from the builders settings.
    ///
    /// Returns `None` if a metric with the same name already exists, otherwise
    /// returns the newly created metric.
    pub fn register_metric<T>(
        &self,
        builder: SimpleMetricBuilder<T, H>,
    ) -> Option<SimpleManagerMetric<T, H>>
    where
        T: traits::MetricValue + Clone + Send + 'static,
    {
        let mut manager = self.inner.lock().unwrap();
        if manager.metrics.contains_key(&builder.name) {
            return None;
        }

        let metric = SimpleManagerMetric {
            data: Arc::new(Mutex::new(MetricData {
                value: builder.initial_value,
                token: None,
                cb: builder.cmd_cb,
                use_alias: builder.use_alias,
            })),
        };
        let metric_insert = Arc::new(metric.clone());
        manager.metrics.insert(builder.name, metric_insert);
        Some(metric)
    }

    fn get_callbacks_from_cmd_message_metrics(
        &self,
        metrics: MessageMetrics,
    ) -> Vec<(Arc<dyn Stored<H> + Send + Sync>, MessageMetric)> {
        let manager = self.inner.lock().unwrap();
        let mut cbs = Vec::with_capacity(metrics.len());
        for metric in metrics {
            match manager.cmd_lookup.get(&metric.id) {
                Some(res) => cbs.push((res.clone(), metric)),
                None => continue,
            };
        }
        cbs
    }

    async fn handle_cmd_metrics(&self, metrics: MessageMetrics) {
        let callbacks = self.get_callbacks_from_cmd_message_metrics(metrics);
        let futures: Vec<Pin<Box<dyn Future<Output = ()> + Send>>> = callbacks
            .into_iter()
            .map(|(stored, value)| {
                Box::pin({
                    let handle = self.clone();
                    async move { stored.cmd_cb(handle, value).await }
                }) as Pin<Box<dyn Future<Output = ()> + Send>>
            })
            .collect();
        join_all(futures).await;
    }

    /// Publishes a single metric.
    ///
    /// Returns an error if the metric was not published.
    pub async fn publish_metric(
        &self,
        metric: SimpleManagerPublishMetric,
    ) -> Result<(), PublishError> {
        self.publish_metrics(vec![metric]).await
    }

    /// Publishes a multiple metric in a single batch.
    ///
    /// Returns an error if the metrics were not published.
    pub async fn publish_metrics(
        &self,
        metrics: Vec<SimpleManagerPublishMetric>,
    ) -> Result<(), PublishError> {
        let handle = {
            match &self.inner.lock().unwrap().handle {
                Some(handle) => handle.clone(),
                None => return Err(PublishError::UnBirthed),
            }
        };

        let publish_metrics = metrics.into_iter().filter_map(|x| x.0).collect();
        handle.publish_metrics(publish_metrics).await
    }
}

impl<H> Default for SimpleMetricManager<H>
where
    H: MetricPublisher + Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<H> MetricManager for SimpleMetricManager<H> {
    fn initialise_birth(&self, bi: &mut BirthInitializer) {
        let mut manager = self.inner.lock().unwrap();

        let mut cmd_lookup = vec![];
        manager.metrics.iter_mut().for_each(|(i, x)| {
            let id = x.birth_metric(i, bi);
            if x.has_callback() {
                cmd_lookup.push((id, x.clone()));
            }
        });
        manager.cmd_lookup = cmd_lookup.into_iter().collect();
    }
}

#[async_trait]
impl NodeMetricManager for SimpleMetricManager<NodeHandle> {
    fn init(&self, handle: &NodeHandle) {
        self.inner.lock().unwrap().handle = Some(handle.clone())
    }

    async fn on_ncmd(&self, _: NodeHandle, metrics: MessageMetrics) {
        self.handle_cmd_metrics(metrics).await
    }
}

#[async_trait]
impl DeviceMetricManager for SimpleMetricManager<DeviceHandle> {
    fn init(&self, handle: &DeviceHandle) {
        self.inner.lock().unwrap().handle = Some(handle.clone())
    }

    async fn on_dcmd(&self, _: DeviceHandle, metrics: MessageMetrics) {
        self.handle_cmd_metrics(metrics).await
    }
}
