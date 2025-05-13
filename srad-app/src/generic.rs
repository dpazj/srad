use std::{collections::HashMap, sync::{Arc, Mutex}};

use srad_types::{payload::DataType, MetricId, MetricValueKind};

use crate::{events::{DBirth, DData, DDeath, NBirth, NData, NDeath}, resequencer::{self, Resequencer}, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier};

struct Metric {
    stale: bool,
    datatype: DataType,
    value: Option<MetricValueKind>
}

impl Metric {

    fn new(datatype: DataType, value: Option<MetricValueKind>) -> Self {
        Self {
            stale: false,
            datatype,
            value
        }
    }
    
}

struct State {
    metrics: HashMap<MetricId, Metric>,
    stale: bool,
}

impl State {

    fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            stale: true,
        }
    }

    fn update_from_birth(&mut self, metrics: Vec<(MetricBirthDetails, MetricDetails)>) -> Result<(), ()> {

        self.stale = false;

        self.metrics.clear();

        for (x, y) in metrics {

            let id = if let Some(alias) = x.alias {
                MetricId::Alias(alias)
            } else {
                MetricId::Name(x.name.clone())
            };

            let value = match y.value {
                Some(val) => match MetricValueKind::try_from_metric_value(x.datatype, val) {
                    Ok(value) => Some(value),
                    Err(_) => return Err(()),
                },
                None => None,
            };

            let metric = Metric::new(x.datatype, value);
            self.metrics.insert(id, metric);
        }

        Ok(())
    }

    fn update_from_data(&mut self, metrics: Vec<(MetricId, MetricDetails)>) -> Result<(), ()> {
        for (id, details) in metrics {
            let metric = match self.metrics.get_mut(&id) {
                Some(metric) => metric,
                None => return Err(()),
            };

            let value = match details.value {
                Some(val) => match MetricValueKind::try_from_metric_value(metric.datatype, val) {
                    Ok(value) => Some(value),
                    Err(_) => return Err(()),
                },
                None => None,
            };

            metric.value = value;
        }

        Ok(())
    }

}

enum ResequenceableEvent {
    NData(NData),
    DBirth(DBirth),
    DDeath(DDeath),
    DData(DData)
}

struct Device {
    state: State
}

struct NodeInner {
    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<String, Device>,
    state: State,
    bdseq: u8,
    last_birth_timestamp: u64
}


impl NodeInner {

    fn new() -> Self {
        Self {
            resequencer: Resequencer::new(),
            devices: HashMap::new(),
            bdseq: 0,
            last_birth_timestamp: 0,
            state: State::new()
        }
    }

    fn handle_birth(&mut self, details: NBirth){

        if details.timestamp <= self.last_birth_timestamp { return }
        if self.bdseq == details.bdseq { return }
        match self.state.update_from_birth(details.metrics_details) {
            Ok(_) => todo!(),
            Err(_) => todo!(),
        };

        self.bdseq = details.bdseq;
        self.resequencer.reset()


    }

    fn handle_death(&mut self, details: NDeath){

        if details.bdseq != self.bdseq {
            todo!()
        }

        for x in self.devices.values() {

        }
    }

    fn process_resequencable_message(&mut self, message: ResequenceableEvent) {

        match message {
            ResequenceableEvent::NData(ndata) => {
                match self.state.update_from_data(ndata.metrics_details) {
                    Ok(_) => todo!(),
                    Err(_) => todo!(),
                }
            },
            ResequenceableEvent::DBirth(dbirth) => {
                let device = match self.devices.get(&dbirth.device_name) {
                    Some(_) => todo!(),
                    None => todo!(),
                };

            },
            ResequenceableEvent::DDeath(ddeath) => {

            },
            ResequenceableEvent::DData(ddata) => todo!(),
        }

    }

    fn handle_resequencable_message(&mut self, seq: u8, message: ResequenceableEvent) {
        let message = match self.resequencer.process(seq, message) {
            resequencer::ProcessResult::MessageNextInSequence(message) => message,
            resequencer::ProcessResult::OutOfSequenceMessageInserted => return,
            resequencer::ProcessResult::DuplicateMessageSequence => todo!("rebirth"),
        };

        self.process_resequencable_message(message);

        loop {
            match self.resequencer.drain() {
                resequencer::DrainResult::Message(message) => self.process_resequencable_message(message),
                resequencer::DrainResult::Empty => break,
                resequencer::DrainResult::SequenceMissing => break,
            }
        }

    }

}

#[derive(Clone)]
struct Node {
    inner: Arc<Mutex<NodeInner>>
}

struct Application {
    nodes: HashMap<NodeIdentifier, Node>,
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
                        Some(n) => n.clone(),
                        None => {
                            let node = Node { inner: Arc::new (Mutex::new(NodeInner::new())) };
                            self.nodes.insert(nbirth.id.clone(), node.clone());
                            node
                        },
                    };
                    tokio::spawn(async move {
                        let mut n = node.inner.lock().unwrap();
                        n.handle_birth(nbirth);
                    });
                },
                AppEvent::NDeath(ndeath) => {
                    let node = match self.nodes.get(&ndeath.id) {
                        Some(node) => node.clone(),
                        None => continue,
                    };
                    tokio::spawn(async move {
                        let mut n = node.inner.lock().unwrap();
                        n.handle_death(ndeath);
                    });
                },
                AppEvent::NData(ndata) => {
                    let node = match self.nodes.get(&ndata.id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.inner.lock().unwrap();
                        n.handle_resequencable_message(ndata.seq, ResequenceableEvent::NData(ndata));
                    });
                },
                AppEvent::DBirth(dbirth) => {
                    let node = match self.nodes.get(&dbirth.node_id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.inner.lock().unwrap();
                        n.handle_resequencable_message(dbirth.seq, ResequenceableEvent::DBirth(dbirth));
                    });
                },
                AppEvent::DDeath(ddeath) => {
                    let node = match self.nodes.get(&ddeath.node_id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.inner.lock().unwrap();
                        n.handle_resequencable_message(ddeath.seq, ResequenceableEvent::DDeath(ddeath));
                    });
                },
                AppEvent::DData(ddata) => {
                    let node = match self.nodes.get(&ddata.node_id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.inner.lock().unwrap();
                        n.handle_resequencable_message(ddata.seq, ResequenceableEvent::DData(ddata));
                    });
                },
                AppEvent::InvalidPayload(_) => (),
                AppEvent::Cancelled => break,
            }
        }        
    }
}
