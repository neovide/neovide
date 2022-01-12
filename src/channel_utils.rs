use std::{
    fmt::Debug,
    sync::mpsc::{SendError, Sender},
};

use log::trace;
use tokio::sync::mpsc::{error::SendError as TokioSendError, UnboundedSender};

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
    tx: UnboundedSender<T>,
    channel_name: String,
}

impl<T> LoggingTx<T>
where
    T: Debug,
{
    pub fn attach(tx: UnboundedSender<T>, channel_name: String) -> Self {
        Self { tx, channel_name }
    }

    pub fn send(&self, message: T) -> Result<(), TokioSendError<T>> {
        trace!("{} {:?}", self.channel_name, &message);
        self.tx.send(message)
    }
}
