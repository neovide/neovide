use std::fmt::Debug;
use std::sync::mpsc::{SendError, Sender};

use log::trace;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct LoggingSender<T>
where
    T: Debug,
{
    sender: Sender<T>,
    channel_name: String,
}

impl<T> LoggingSender<T>
where
    T: Debug,
{
    pub fn attach(sender: Sender<T>, channel_name: String) -> Self {
        Self {
            sender,
            channel_name,
        }
    }

    pub fn send(&self, message: T) -> Result<(), SendError<T>> {
        trace!("{} {:?}", self.channel_name, &message);
        self.sender.send(message)
    }
}

#[derive(Clone)]
pub struct LoggingTx<T>
where
    T: Debug,
{
    tx: mpsc::UnboundedSender<T>,
    channel_name: String,
}

impl<T> LoggingTx<T>
where
    T: Debug,
{
    pub fn attach(tx: mpsc::UnboundedSender<T>, channel_name: String) -> Self {
        Self { tx, channel_name }
    }

    pub fn send(&self, message: T) -> Result<(), mpsc::error::SendError<T>> {
        trace!("{} {:?}", self.channel_name, &message);
        self.tx.send(message)
    }
}
