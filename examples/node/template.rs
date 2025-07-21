use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rand::Rng;
use srad::{
    client_rumqtt as rumqtt,
    eon::{
        BirthMetricDetails, EoNBuilder, MessageMetrics, MetricManager, MetricPublisher,
        MetricToken, NodeHandle, NodeMetricManager,
    },
    types::{Template, TemplateInstance, TemplateMetadata},
};

use async_trait::async_trait;
use log::{error, info, warn, LevelFilter};

#[derive(Template, Default, Clone)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
}

impl TemplateMetadata for Point {
    fn template_name() -> &'static str {
        "point"
    }
}

struct TemplateData {
    point: Point,
    point_token: Option<MetricToken<Point>>,
}

#[derive(Clone)]
struct TemplateManager(Arc<Mutex<TemplateData>>);

impl TemplateManager {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(TemplateData {
            point: Point::default(),
            point_token: None,
        })))
    }
}

impl MetricManager for TemplateManager {
    fn initialise_birth(&self, bi: &mut srad::eon::BirthInitializer) {
        let mut data = self.0.lock().unwrap();
        data.point_token = Some(
            bi.register_template_metric(BirthMetricDetails::new_template_metric(
                "PointInstance",
                data.point.clone(),
            ))
            .unwrap(),
        );
    }
}

#[async_trait]
impl NodeMetricManager for TemplateManager {
    async fn on_ncmd(&self, _node: NodeHandle, metrics: MessageMetrics) {
        let mut data = self.0.lock().unwrap();

        for x in metrics {
            let point_tok = match &data.point_token {
                Some(token) => token,
                None => return,
            };
            if x.id != point_tok.id {
                continue;
            }

            data.point = match x.value {
                Some(point_data) => match TemplateInstance::try_from(point_data) {
                    Ok(value) => match value.try_into() {
                        Ok(point) => point,
                        Err(e) => {
                            error!("Invalid Point template received: {e}");
                            continue;
                        }
                    },
                    Err(e) => {
                        error!("Invalid template metric value received: {e}");
                        continue;
                    }
                },
                None => Point::default(),
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
        .init();

    let opts = rumqtt::MqttOptions::new("node", "localhost", 1883);

    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);

    let manager = TemplateManager::new();

    let (eon, handle) = EoNBuilder::new(eventloop, client)
        .with_group_id("foo")
        .with_node_id("bar")
        .with_metric_manager(manager.clone())
        .register_template::<Point>()
        .build()
        .unwrap();

    let publish_handle = handle.clone();
    tokio::spawn(async move {
        loop {
            let publish = {
                let mut rng = rand::thread_rng();
                let mut inner = manager.0.lock().unwrap();
                inner.point = Point {
                    x: rng.gen(),
                    y: rng.gen(),
                    z: rng.gen(),
                };
                if let Some(token) = &inner.point_token {
                    Some(token.create_publish_template_metric(inner.point.clone()))
                } else {
                    None
                }
            };

            if let Some(publish) = publish {
                if let Err(e) = publish_handle.publish_metric(publish).await {
                    warn!("Unable to publish template metric: {e}");
                } else {
                    info!("Successfully published template metric point");
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            println!("Failed to register CTRL-C handler: {e}");
            return;
        }
        handle.cancel().await;
    });

    eon.run().await;
}
