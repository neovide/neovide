use std::fmt::Debug;

use log::trace;
use tokio::sync::mpsc::{error::SendError as TokioSendError, UnboundedSender};

use crate::profiling::tracy_dynamic_zone;

#[derive(Clone)]
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
