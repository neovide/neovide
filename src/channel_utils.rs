use std::{fmt::Debug, sync::Arc};

use log::trace;
use tokio::sync::{
    mpsc::{
        error::{SendError as TokioSendError, TryRecvError},
        UnboundedReceiver, UnboundedSender,
    },
    Mutex,
};

use crate::profiling::tracy_dynamic_zone;

#[derive(Clone, Debug)]
pub struct LoggingSender<T>
where
    T: Debug + AsRef<str>,
{
    tx: UnboundedSender<T>,
    channel_name: String,
}

impl<T> LoggingSender<T>
where
    T: Debug + AsRef<str>,
{
    pub fn attach(tx: UnboundedSender<T>, channel_name: &str) -> Self {
        Self {
            tx,
            channel_name: channel_name.to_string(),
        }
    }

    pub fn send(&self, message: T) -> Result<(), TokioSendError<T>> {
        tracy_dynamic_zone!(&format!("{}::{}", self.channel_name, message.as_ref()));
        trace!("{} {:?}", self.channel_name, &message);
        self.tx.send(message)
    }
}

#[derive(Clone, Debug)]
pub struct LoggingReceiver<T>
where
    T: Debug + AsRef<str>,
{
    rx: Arc<Mutex<UnboundedReceiver<T>>>,
    channel_name: String,
}

impl<T> LoggingReceiver<T>
where
    T: Debug + AsRef<str>,
{
    pub fn attach(rx: UnboundedReceiver<T>, channel_name: &str) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
            channel_name: channel_name.to_string(),
        }
    }

    pub async fn recv(&mut self) -> Option<T> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(message) => {
                tracy_dynamic_zone!(&format!("{}::{}", self.channel_name, message.as_ref()));
                trace!("{} {:?}", self.channel_name, &message);
                Some(message)
            }
            None => None,
        }
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let mut rx = self.rx.try_lock().expect("Could not lock receiver");
        match rx.try_recv() {
            Ok(message) => {
                tracy_dynamic_zone!(&format!("{}::{}", self.channel_name, message.as_ref()));
                trace!("{} {:?}", self.channel_name, &message);
                Ok(message)
            }
            Err(e) => Err(e),
        }
    }
}
