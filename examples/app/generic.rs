use std::collections::HashMap;
use std::sync::Arc;

use log::{info, LevelFilter};
use srad::app::generic_app::{MetricStore, StateUpdateError};
use srad::app::{generic_app, NodeIdentifier, SubscriptionConfig};
use srad::client_rumqtt as rumqtt;
use srad::types::payload::DataType;
use srad::types::{MetricId, MetricValueKind};

struct MetricStoreImpl {
    node: Arc<NodeIdentifier>,
    device: Option<String>,
    metric_types: HashMap<MetricId, DataType>,
}

impl MetricStoreImpl {
    fn new(node: Arc<NodeIdentifier>, device: Option<String>) -> Self {
        Self {
            node,
            device,
            metric_types: HashMap::new(),
        }
    }
}

impl MetricStore for MetricStoreImpl {
    fn set_stale(&mut self) {
        info!("Node ({:?}) Device ({:?}) Stale", self.node, self.device);
    }

    fn update_from_birth(
        &mut self,
        details: Vec<(srad::app::MetricBirthDetails, srad::app::MetricDetails)>,
    ) -> Result<(), generic_app::StateUpdateError> {
        self.metric_types.clear();
        for (birth_details, value_details) in details {
            let id = birth_details.get_metric_id();
            let value = match value_details.value {
                Some(val) => {
                    match MetricValueKind::try_from_metric_value(birth_details.datatype, val) {
                        Ok(val) => Some(val),
                        Err(e) => {
                            match e {
                                srad::types::FromMetricValueError::ValueDecodeError(_) => (),
                                srad::types::FromMetricValueError::InvalidDataType => (),
                                srad::types::FromMetricValueError::UnsupportedDataType(_) => {
                                    continue
                                }
                            }
                            return Err(StateUpdateError::InvalidValue);
                        }
                    }
                }
                None => None,
            };
            // info!(
            //     "Node ({:?}) Device ({:?}) Got birth metric {:?} with value {:?}",
            //     self.node, self.device, id, value
            // );
            if let Some(_) = self.metric_types.insert(id, birth_details.datatype) {
                return Err(StateUpdateError::InvalidValue);
            }
        }
        Ok(())
    }

    fn update_from_data(
        &mut self,
        details: Vec<(srad::types::MetricId, srad::app::MetricDetails)>,
    ) -> Result<(), generic_app::StateUpdateError> {
        for (id, value_details) in details {
            let datatype = match self.metric_types.get(&id) {
                Some(datatype) => datatype,
                None => return Err(generic_app::StateUpdateError::UnknownMetric),
            };
            let value = match value_details.value {
                Some(val) => match MetricValueKind::try_from_metric_value(*datatype, val) {
                    Ok(val) => Some(val),
                    Err(_) => return Err(StateUpdateError::InvalidValue),
                },
                None => None,
            };
            // info!(
            //     "Node ({:?}) Device ({:?}) Got data metric {:?} with value {:?}",
            //     self.node, self.device, id, value
            // );
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let opts = rumqtt::MqttOptions::new("client", "localhost", 1883);
    let (eventloop, client) = rumqtt::EventLoop::new(opts, 0);
    let (application, client) =
        generic_app::Application::new("foo", eventloop, client, SubscriptionConfig::AllGroups);

    tokio::spawn(async move {
        application
            .on_node_created(|node| {
                info!("Node created {:?}", node.id());
                node.register_metric_store(MetricStoreImpl::new(node.id().clone(), None));
                let node_id = node.id().clone();
                node.on_device_created(move |dev| {
                    info!("Device created {} node {:?}", dev.name(), node_id);
                    dev.register_metric_store(MetricStoreImpl::new(
                        node_id.clone(),
                        Some(dev.name().to_string()),
                    ));
                });
            })
            .run()
            .await;
    });

    if let Err(e) = tokio::signal::ctrl_c().await {
        println!("Failed to register CTRL-C handler: {e}");
        return;
    }
    client.cancel().await;
}
