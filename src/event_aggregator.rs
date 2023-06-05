use std::{
    any::{type_name, Any, TypeId},
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
};

use parking_lot::RwLock;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::channel_utils::*;

lazy_static! {
    pub static ref EVENT_AGGREGATOR: EventAggregator = EventAggregator::default();
}

pub struct EventAggregator {
    parent_senders: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    unclaimed_receivers: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl Default for EventAggregator {
    fn default() -> Self {
        EventAggregator {
            parent_senders: RwLock::new(HashMap::new()),
            unclaimed_receivers: RwLock::new(HashMap::new()),
        }
    }
}

impl EventAggregator {
    fn get_sender<T: Any + Clone + Debug + Send>(&self) -> LoggingTx<T> {
        match self.parent_senders.write().entry(TypeId::of::<T>()) {
            Entry::Occupied(entry) => {
                let sender = entry.get();
                sender.downcast_ref::<LoggingTx<T>>().unwrap().clone()
            }
            Entry::Vacant(entry) => {
                let (sender, receiver) = unbounded_channel();
                let logging_tx = LoggingTx::attach(sender, type_name::<T>().to_owned());
                entry.insert(Box::new(logging_tx.clone()));
                self.unclaimed_receivers
                    .write()
                    .insert(TypeId::of::<T>(), Box::new(receiver));
                logging_tx
            }
        }
    }

    pub fn send<T: Any + Clone + Debug + Send>(&self, event: T) {
        let sender = self.get_sender::<T>();
        sender.send(event).unwrap();
    }

    pub fn register_event<T: Any + Clone + Debug + Send>(&self) -> UnboundedReceiver<T> {
        let type_id = TypeId::of::<T>();

        if let Some(receiver) = self.unclaimed_receivers.write().remove(&type_id) {
            *receiver.downcast::<UnboundedReceiver<T>>().unwrap()
        } else {
            let (sender, receiver) = unbounded_channel();
            let logging_sender = LoggingTx::attach(sender, type_name::<T>().to_owned());

            match self.parent_senders.write().entry(type_id) {
                Entry::Occupied(_) => panic!("EventAggregator: type already registered"),
                Entry::Vacant(entry) => {
                    entry.insert(Box::new(logging_sender));
                }
            }

            receiver
        }
    }
}
