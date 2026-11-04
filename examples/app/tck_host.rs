//! A Sparkplug Host Application for the Eclipse Sparkplug TCK `host` profile.
//!
//! The application itself is built entirely with `srad-app`'s generic
//! application. The only test-specific part is a small control MQTT client that
//! subscribes to the TCK's console prompt topic: when the TCK asks the operator
//! to send a command, this example publishes the requested NCMD or DCMD with
//! the generic application's [AppClient].
//!
//! The metric names used for the command payloads come from the metrics that
//! the TCK's own edge node utility publishes in its NBIRTH and DBIRTH. They can
//! be used for any edge node that has the same birth metrics, which is what the
//! TCK provides for `host:SendCommandTest`.
//!
//! `tck-runner` exports its test parameters as `SRAD_HOST_APP_ID`,
//! `SRAD_GROUP_ID`, `SRAD_NODE_ID`, `SRAD_DEVICE_ID`, `SRAD_CLIENT_ID`,
//! `SRAD_BROKER_HOST` and `SRAD_BROKER_PORT`. The example uses them when
//! present and falls back to the runner's defaults.
//!
//! The edge rebirth command uses `AppClient::publish_node_rebirth`, which omits
//! the metric datatype as recommended by the specification. The TCK requires a
//! Boolean datatype for `Node Control/Rebirth`, so
//! `operational-behavior-data-commands-ncmd-rebirth-value` is not executed and
//! `host:SendCommandTest` reports `PASS but INCOMPLETE`.
//!
//! Run it as the host application under test:
//!
//! ```shell
//! cargo build -p examples --example tck-host
//! ./target/debug/tck-runner host:SendCommandTest \
//!     --docker-image srad-tck \
//!     --node-cmd ./target/debug/examples/tck-host \
//!     --host-app-id foo \
//!     --client-id client \
//!     --console-reply PASS
//! ```

use std::sync::Arc;
use std::time::Duration;

use log::{info, warn, LevelFilter};
use rumqttc::v5::mqttbytes::v5::Packet;
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::{AsyncClient, Event};
use srad::app::generic_app::{MetricStore, StateUpdateError};
use srad::app::{
    generic_app, AppClient, MetricBirthDetails, MetricDetails, NodeIdentifier, PublishMetric,
    PublishTopic, SubscriptionConfig,
};
use srad::client_rumqtt as rumqtt;
use srad::types::{payload::DataType, MetricId};

const DEFAULT_HOST_ID: &str = "foo";
const DEFAULT_GROUP_ID: &str = "foo";
const DEFAULT_NODE_ID: &str = "bar";
const DEFAULT_DEVICE_ID: &str = "dev1";
const DEFAULT_CLIENT_ID: &str = "client";
const DEFAULT_BROKER_HOST: &str = "localhost";
const DEFAULT_BROKER_PORT: u16 = 1883;

const CONSOLE_PROMPT_TOPIC: &str = "SPARKPLUG_TCK/CONSOLE_PROMPT";

// Metrics published by the TCK edge node utility in its NBIRTH and DBIRTH.
const NODE_COMMAND_METRIC: &str = "TCK_metric/Int32";
const DEVICE_REBIRTH_METRIC: &str = "EdgeNode Control/Rebirth";
const DEVICE_COMMAND_METRIC: &str = "Outputs/1";

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Test parameters. `tck-runner` exports these to the node process so the
/// example follows its `--host-app-id`, `--group-id`, `--node-id`,
/// `--device-id`, `--client-id` and `--broker` options.
#[derive(Clone)]
struct Config {
    host_id: String,
    group_id: String,
    node_id: String,
    device_id: String,
    client_id: String,
    broker_host: String,
    broker_port: u16,
}

impl Config {
    fn from_env() -> Self {
        Self {
            host_id: env_or("SRAD_HOST_APP_ID", DEFAULT_HOST_ID),
            group_id: env_or("SRAD_GROUP_ID", DEFAULT_GROUP_ID),
            node_id: env_or("SRAD_NODE_ID", DEFAULT_NODE_ID),
            device_id: env_or("SRAD_DEVICE_ID", DEFAULT_DEVICE_ID),
            client_id: env_or("SRAD_CLIENT_ID", DEFAULT_CLIENT_ID),
            broker_host: env_or("SRAD_BROKER_HOST", DEFAULT_BROKER_HOST),
            broker_port: env_or("SRAD_BROKER_PORT", &DEFAULT_BROKER_PORT.to_string())
                .parse()
                .unwrap_or(DEFAULT_BROKER_PORT),
        }
    }

    fn control_client_id(&self) -> String {
        format!("{}-tck-control", self.client_id)
    }
}

/// Minimal metric store used to show the generic application receiving births
/// and data. It accepts every update so it does not cause spurious rebirths.
struct LoggingStore {
    node: Arc<NodeIdentifier>,
    device: Option<String>,
}

impl MetricStore for LoggingStore {
    fn set_stale(&mut self) {
        info!("Node ({:?}) Device ({:?}) is stale", self.node, self.device);
    }

    fn update_from_birth(
        &mut self,
        details: Vec<(MetricBirthDetails, MetricDetails)>,
    ) -> Result<(), StateUpdateError> {
        for (birth, _) in details {
            info!(
                "Node ({:?}) Device ({:?}) birth metric {:?}",
                self.node, self.device, birth.name
            );
        }
        Ok(())
    }

    fn update_from_data(
        &mut self,
        details: Vec<(MetricId, MetricDetails)>,
    ) -> Result<(), StateUpdateError> {
        info!(
            "Node ({:?}) Device ({:?}) received data for {} metric(s)",
            self.node,
            self.device,
            details.len()
        );
        Ok(())
    }
}

/// Publish the command requested by a TCK console prompt.
async fn handle_prompt(app: &AppClient, prompt: &str, config: &Config) {
    info!("TCK prompt: {prompt}");

    // `publish_node_rebirth` omits the metric datatype, as Sparkplug
    // recommends for command messages. The TCK requires a Boolean datatype for
    // its `ncmd-rebirth-value` assertion, so that assertion is not executed
    // here; this example prefers the specification-correct payload.
    let result = if prompt.contains("Send an edge rebirth") {
        app.publish_node_rebirth(&config.group_id, &config.node_id)
            .await
    } else if prompt.contains("Send an edge command") {
        app.publish_metrics(
            PublishTopic::new_node_cmd(&config.group_id, &config.node_id),
            vec![
                PublishMetric::new(MetricId::Name(NODE_COMMAND_METRIC.into()), 1i32)
                    .with_datatype(DataType::Int32),
            ],
        )
        .await
    } else if prompt.contains("Send a device rebirth") {
        app.publish_metrics(
            PublishTopic::new_device_cmd(&config.group_id, &config.node_id, &config.device_id),
            vec![
                PublishMetric::new(MetricId::Name(DEVICE_REBIRTH_METRIC.into()), true)
                    .with_datatype(DataType::Boolean),
            ],
        )
        .await
    } else if prompt.contains("Send a device command") {
        app.publish_metrics(
            PublishTopic::new_device_cmd(&config.group_id, &config.node_id, &config.device_id),
            vec![
                PublishMetric::new(MetricId::Name(DEVICE_COMMAND_METRIC.into()), 1i32)
                    .with_datatype(DataType::Int32),
            ],
        )
        .await
    } else {
        return;
    };

    match result {
        Ok(()) => info!("Published command for prompt: {prompt}"),
        Err(error) => warn!("Unable to publish command for prompt `{prompt}`: {error}"),
    }
}

/// Subscribe to the TCK console prompts and turn them into command publishes.
async fn run_control_client(app: AppClient, config: Config) {
    let mut options = rumqtt::MqttOptions::new(
        config.control_client_id(),
        &config.broker_host,
        config.broker_port,
    );
    options.set_keep_alive(Duration::from_secs(30));
    let (client, mut eventloop) = AsyncClient::new(options, 16);

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                info!("Control client connected");
                if let Err(error) = client
                    .subscribe(CONSOLE_PROMPT_TOPIC, QoS::AtLeastOnce)
                    .await
                {
                    warn!("Unable to subscribe to {CONSOLE_PROMPT_TOPIC}: {error}");
                }
            }
            Ok(Event::Incoming(Packet::Publish(publish)))
                if publish.topic == CONSOLE_PROMPT_TOPIC =>
            {
                let prompt = String::from_utf8_lossy(&publish.payload);
                handle_prompt(&app, prompt.trim(), &config).await;
            }
            Ok(_) => {}
            Err(error) => {
                warn!("Control client error: {error}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let config = Config::from_env();

    let opts = rumqtt::MqttOptions::new(&config.client_id, &config.broker_host, config.broker_port);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 64);

    let (application, client) = generic_app::ApplicationBuilder::new(
        &config.host_id,
        eventloop,
        client,
        SubscriptionConfig::AllGroups,
    )
    .on_node_created(|node| {
        info!("Node created {:?}", node.id());
        node.register_metric_store(LoggingStore {
            node: node.id().clone(),
            device: None,
        });
        let node_id = node.id().clone();
        node.on_device_created(move |device| {
            info!("Device created {} for node {:?}", device.name(), node_id);
            device.register_metric_store(LoggingStore {
                node: node_id.clone(),
                device: Some(device.name().to_string()),
            });
        });
    })
    .build();

    tokio::spawn(run_control_client(client.clone(), config.clone()));

    let app = tokio::spawn(async move {
        application.run().await;
    });

    if let Err(error) = tokio::signal::ctrl_c().await {
        info!("Failed to register CTRL-C handler: {error}");
        return;
    }
    client.cancel().await;

    // Wait for the event loop to flush the offline STATE message and the
    // disconnect before the process exits.
    let _ = app.await;
}
