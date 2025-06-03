use std::collections::HashMap;

use log::{info, LevelFilter};
use srad::app::generic::{MetricStore, StateUpdateError};
use srad::app::{generic, NodeIdentifier, SubscriptionConfig};
use srad::client_rumqtt as rumqtt;
use srad::types::payload::DataType;
use srad::types::{MetricId, MetricValueKind};

struct MetricStoreImpl {
    node: NodeIdentifier,
    device: Option<String>,
    metric_types: HashMap<MetricId, DataType>
}

impl MetricStoreImpl {
    fn new(node: NodeIdentifier, device: Option<String>) -> Self {
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

    fn update_from_birth(&mut self, details: Vec<(srad::app::MetricBirthDetails, srad::app::MetricDetails)>) -> Result<(), generic::StateUpdateError> {
        self.metric_types.clear();
        for (birth_details, value_details) in details {
            let id = if let Some(alias) = birth_details.alias { MetricId::Alias(alias) } else { MetricId::Name(birth_details.name)};
            let value = match value_details.value {
                Some(val) => match MetricValueKind::try_from_metric_value(birth_details.datatype, val) {
                    Ok(val) => Some(val),
                    Err(_) => {
                        return Err(StateUpdateError::InvalidValue)
                    },
                },
                None => None,
            };
            info!("Node ({:?}) Device ({:?}) Got birth metric {:?} with value {:?}", self.node, self.device, id, value);
            if let Some(_) = self.metric_types.insert(id, birth_details.datatype) { return Err(StateUpdateError::InvalidValue) }
        }
        Ok (())
    }

    fn update_from_data(&mut self, details: Vec<(srad::types::MetricId, srad::app::MetricDetails)>) -> Result<(), generic::StateUpdateError> {
        for (id, value_details) in details {
            let datatype = match self.metric_types.get(&id) {
                Some(datatype) => datatype,
                None => return Err(generic::StateUpdateError::UnknownMetric)
            };
            let value = match value_details.value {
                Some(val) => match MetricValueKind::try_from_metric_value(*datatype, val) {
                    Ok(val) => Some(val),
                    Err(_) => return Err(StateUpdateError::InvalidValue),
                },
                None => None,
            };
            info!("Node ({:?}) Device ({:?}) Got data metric {:?} with value {:?}", self.node, self.device, id, value);
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
    let (application, _) = generic::Application::new("foo", eventloop, client, SubscriptionConfig::AllGroups);
    application.on_node_created(|id, node| {
            let id= id.clone();
            info!("Node created {:?}", id);
            node.set_metric_store(MetricStoreImpl::new(id.clone(), None));
            node.on_device_created(move |dev| {
                info!("Device created {} node {:?}", dev.name(), id);
                dev.set_metric_store(MetricStoreImpl::new(id.clone(), Some(dev.name().to_string())));
            });
    })
    .run().await;
}
