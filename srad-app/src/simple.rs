use std::{collections::HashMap, sync::{Arc, Mutex}};

use srad_types::{payload::DataType, MetricId};

use crate::{events::{DBirth, DData, DDeath, NBirth, NData, NDeath}, resequencer::{self, Resequencer}, AppEvent, AppEventLoop, MetricBirthDetails, MetricDetails, NodeIdentifier};


struct Metric {
    stale: bool,
    datatype: DataType
}

impl Metric {

    fn new(datatype: DataType) -> Self {
        Self {
            stale: false,
            datatype,
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

    fn update_from_birth(&mut self, metrics: Vec<(MetricBirthDetails, MetricDetails)>) {

        self.stale = false;

        self.metrics.clear();

        for (x, y) in metrics {

            let id = if let Some(alias) = x.alias {
                MetricId::Alias(alias)
            } else {
                MetricId::Name(x.name.clone())
            };

            let metric = Metric::new(x.datatype);
            self.metrics.insert(id, metric);
        }

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

struct Node {
    resequencer: Resequencer<ResequenceableEvent>,
    devices: HashMap<NodeIdentifier, Arc<Node>>,
    state: State,
    bdseq: u8,
    last_birth_timestamp: u64
}

impl Node {

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
        self.bdseq = details.bdseq;
        self.state.update_from_birth(details.metrics_details);
        //self.resequencer.reset()

    }

    fn handle_death(&mut self, details: NDeath){

    }

    fn process_resequencable_message(&mut self, message: ResequenceableEvent) {

        match message {
            ResequenceableEvent::NData(ndata) => todo!(),
            ResequenceableEvent::DBirth(dbirth) => todo!(),
            ResequenceableEvent::DDeath(ddeath) => todo!(),
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

struct Application {
    nodes: HashMap<NodeIdentifier, Arc<Mutex<Node>>>,
    eventloop: AppEventLoop
}

impl Application {
    
    pub fn new(eventloop: AppEventLoop) {

    }

    fn handle_resequencable_message(&self, seq: u8, message: ResequenceableEvent) {
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
                            let node = Arc::new (Mutex::new(Node::new()));
                            self.nodes.insert(nbirth.id.clone(), node.clone());
                            node
                        },
                    };
                    tokio::spawn(async move {
                        let mut n = node.lock().unwrap();
                        n.handle_birth(nbirth);
                    });
                },
                AppEvent::NDeath(ndeath) => {
                    let node = match self.nodes.get(&ndeath.id) {
                        Some(node) => node.clone(),
                        None => continue,
                    };
                    tokio::spawn(async move {
                        let mut n = node.lock().unwrap();
                        n.handle_death(ndeath);
                    });
                },
                AppEvent::NData(ndata) => {
                    let node = match self.nodes.get(&ndata.id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.lock().unwrap();
                        n.handle_resequencable_message(ndata.seq, ResequenceableEvent::NData(ndata));
                    });
                },
                AppEvent::DBirth(dbirth) => {
                    let node = match self.nodes.get(&dbirth.node_id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.lock().unwrap();
                        n.handle_resequencable_message(dbirth.seq, ResequenceableEvent::DBirth(dbirth));
                    });
                },
                AppEvent::DDeath(ddeath) => {
                    let node = match self.nodes.get(&ddeath.node_id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.lock().unwrap();
                        n.handle_resequencable_message(ddeath.seq, ResequenceableEvent::DDeath(ddeath));
                    });
                },
                AppEvent::DData(ddata) => {
                    let node = match self.nodes.get(&ddata.node_id) {
                        Some(node) => node.clone(),
                        None => todo!("issue rebirth"),
                    };
                    tokio::spawn(async move {
                        let mut n = node.lock().unwrap();
                        n.handle_resequencable_message(ddata.seq, ResequenceableEvent::DData(ddata));
                    });
                },
                AppEvent::InvalidPayload(_) => (),
                AppEvent::Cancelled => break,
            }
        }        
    }
}
