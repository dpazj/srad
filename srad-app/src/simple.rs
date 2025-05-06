use std::{collections::HashMap, sync::{Arc, Mutex}};

use srad_types::MetricId;

use crate::{resequencer::Resequencer, AppEvent, AppEventLoop, NodeIdentifier};


struct Metric {

}

struct State {
    metrics: HashMap<MetricId, Metric>,
    stale: bool,
}

struct Device {
    state: State
}

struct Node {
    resequencer: Resequencer<AppEvent>,
    devices: HashMap<NodeIdentifier, Arc<Node>>,
    state: State
}

impl Node {

    fn new() -> Self {
        Self {
            resequencer: Resequencer::new(),
            devices: HashMap::new(),
            metrics: todo!(),
        }
    }

}

struct Application {

    nodes: HashMap<NodeIdentifier, Arc<Node>>,

    eventloop: AppEventLoop
}

impl Application {
    
    pub fn new(eventloop: AppEventLoop) {

    }

    pub async fn run(&mut self) {
        loop {
            match self.eventloop.poll().await {
                AppEvent::Online => todo!(),
                AppEvent::Offline => todo!(),
                AppEvent::NBirth(nbirth) => {
                    let node = match self.nodes.get(&nbirth.id) {
                        Some(_) => todo!(),
                        None => {
                            let node = Arc::new (Node::new());
                            self.nodes.insert(nbirth.id, node.clone());
                            node
                        },
                    };


                },
                AppEvent::NDeath(ndeath) => todo!(),
                AppEvent::NData(ndata) => todo!(),
                AppEvent::DBirth(dbirth) => todo!(),
                AppEvent::DDeath(ddeath) => todo!(),
                AppEvent::DData(ddata) => todo!(),
                AppEvent::InvalidPayload(payload_error_details) => todo!(),
                AppEvent::Cancelled => break,
            }
        }        
    }
}
