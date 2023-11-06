use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    sync::Mutex,
};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::channel_utils::*;

lazy_static! {
    pub static ref EVENT_AGGREGATOR: EventAggregator = EventAggregator::default();
}

type Receiver = dyn Any + Send + Sync;
type Sender = dyn Any + Send + Sync;
type ChannelHashMap = HashMap<TypeId, (Box<Sender>, Option<Box<Receiver>>)>;

pub struct EventAggregator {
    senders: Mutex<ChannelHashMap>,
}

impl Default for EventAggregator {
    fn default() -> Self {
        EventAggregator {
            senders: Mutex::new(HashMap::new()),
        }
    }
}

impl EventAggregator {
    fn with_entry<T: Any + Clone + Debug + Send, F, Ret>(&self, f: F) -> Ret
    where
        F: FnOnce(&mut (Box<Sender>, Option<Box<Receiver>>)) -> Ret,
    {
        let mut hash_map = self.senders.lock().unwrap();
        let entry = hash_map.entry(TypeId::of::<T>()).or_insert_with(|| {
            let (sender, receiver) = unbounded_channel::<T>();
            let logging_tx = LoggingTx::attach(sender, type_name::<T>().to_owned());
            (Box::new(logging_tx), Some(Box::new(receiver)))
        });
        f(entry)
    }

    fn get_sender<T: Any + Clone + Debug + Send>(&self) -> LoggingTx<T> {
        self.with_entry::<T, _, _>(|entry| {
            let sender = &entry.0;
            sender.downcast_ref::<LoggingTx<T>>().unwrap().clone()
        })
    }

    pub fn send<T: Any + Clone + Debug + Send>(&self, event: T) {
        let sender = self.get_sender::<T>();
        // Ignore errors due to the channel being closed (those are the only ones that can be generated)
        // That can happen during the shutdown process, or when some thread crashes
        let _ = sender.send(event);
    }

    pub fn register_event<T: Any + Clone + Debug + Send>(&self) -> UnboundedReceiver<T> {
        self.with_entry::<T, _, _>(|entry| {
            let receiver = entry.1.take().unwrap();
            *receiver.downcast::<UnboundedReceiver<T>>().unwrap()
        })
    }
}
