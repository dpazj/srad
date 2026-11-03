//! An Edge Node that exercises the features checked by the Sparkplug TCK's
//! `SendComplexDataTest`.
//!
//! The node publishes a DataSet, a template definition (NBIRTH) and template
//! instance (DBIRTH), plus regular metrics, then keeps publishing data with
//! aliases. Use it as the node under test for the TCK:
//!
//! ```shell
//! # in one terminal
//! docker run --rm -it --network=host -e TCK_HEADLESS=1 srad-tck
//!
//! # in another
//! cargo run --example tck-node
//! ./target/debug/tck-runner edge:SendComplexDataTest
//! ```

use std::time::Duration;

use log::LevelFilter;
use srad::{
    client_rumqtt as rumqtt,
    eon::{
        simple_manager::{SimpleMetricBuilder, SimpleMetricManager},
        BirthInitializer, BirthMetricDetails, DeviceHandle, DeviceMetricManager, EoNBuilder,
        MetricManager, NodeHandle,
    },
    types::{DataSetRow, Template, TemplateMetadata, TypedDataSet},
};
use tokio::time;

/// A template with both metrics and a parameter. The definition is published
/// in the node birth and the instance in the device birth, which keeps the
/// TCK's template member checks valid.
#[derive(Template, Default, Clone)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
    #[template(parameter, default = 1.0)]
    scale: f64,
}

impl TemplateMetadata for Point {
    fn template_name() -> &'static str {
        "point"
    }
}

#[derive(DataSetRow, Clone, Debug, PartialEq)]
struct Sample {
    #[dataset(rename = "Time")]
    time: u32,
    #[dataset(rename = "Value")]
    value: f64,
    #[dataset(rename = "Label")]
    label: String,
}

impl Sample {
    fn new(time: u32, value: f64, label: &str) -> Self {
        Self {
            time,
            value,
            label: label.to_string(),
        }
    }
}

fn dataset(time: u32, value: f64) -> TypedDataSet<Sample> {
    let mut dataset = TypedDataSet::new();
    dataset.add_row(Sample::new(time, value, "one"));
    dataset.add_row(Sample::new(time + 1, value + 1.0, "two"));
    dataset
}

/// Adds the template instance metric to the device's birth on top of the
/// metrics managed by the [SimpleMetricManager].
///
/// The template definition is registered on the node, so it is published in
/// the node birth. Template instances are published in a device birth because
/// the TCK rejects instances in NBIRTH when the definition has metrics.
struct TckDeviceMetrics {
    inner: SimpleMetricManager<DeviceHandle>,
}

impl MetricManager for TckDeviceMetrics {
    fn initialise_birth(&self, bi: &mut BirthInitializer) {
        MetricManager::initialise_birth(&self.inner, bi);
        bi.register_template_metric(BirthMetricDetails::new_template_metric(
            "DeviceTemplate",
            Point::default(),
        ))
        .unwrap();
    }
}

impl DeviceMetricManager for TckDeviceMetrics {
    fn init(&self, handle: &DeviceHandle) {
        DeviceMetricManager::init(&self.inner, handle);
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let opts = rumqtt::MqttOptions::new("tck-node", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 64);

    let node_metrics = SimpleMetricManager::<NodeHandle>::new();
    let node_counter = node_metrics
        .register_metric(SimpleMetricBuilder::new("NodeCounter", 0_u64))
        .unwrap();
    let node_dataset = node_metrics
        .register_metric(SimpleMetricBuilder::new("NodeDataSet", dataset(1, 1.5)))
        .unwrap();

    let (eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("foo")
        .with_node_id("bar")
        // The Sparkplug TCK runs a primary host application with this ID and
        // expects the node to wait for its online STATE before birthing.
        .with_primary_host_id("SCADA_HOST")
        .with_metric_manager(node_metrics.clone())
        .register_template::<Point>()
        .build()
        .unwrap();

    let device_metrics = SimpleMetricManager::<DeviceHandle>::new();
    let device = handle
        .register_device(
            "dev1",
            TckDeviceMetrics {
                inner: device_metrics.clone(),
            },
        )
        .unwrap();
    let device_counter = device_metrics
        .register_metric(SimpleMetricBuilder::new("DeviceCounter", 0_u64))
        .unwrap();
    let device_dataset = device_metrics
        .register_metric(SimpleMetricBuilder::new("DeviceDataSet", dataset(1, 2.5)))
        .unwrap();
    device.enable();

    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            println!("Failed to register CTRL-C handler: {e}");
            return;
        }
        handle.cancel().await;
    });

    // Publish data that uses aliases, which the TCK checks for in DATA messages.
    tokio::spawn(async move {
        let mut counter = 0_u64;
        loop {
            counter = counter.wrapping_add(1);

            _ = node_metrics
                .publish_metrics(vec![
                    node_counter.update(|value| *value = counter),
                    node_dataset.update(|value| *value = dataset(counter as u32, counter as f64)),
                ])
                .await;

            _ = device_metrics
                .publish_metrics(vec![
                    device_counter.update(|value| *value = counter),
                    device_dataset.update(|value| *value = dataset(counter as u32, counter as f64)),
                ])
                .await;

            time::sleep(Duration::from_secs(1)).await;
        }
    });

    eon.run().await;
}
