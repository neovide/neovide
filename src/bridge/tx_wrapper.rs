use futures::AsyncWrite;

pub type TxWrapper = Box<dyn AsyncWrite + Send + Unpin + 'static>;
