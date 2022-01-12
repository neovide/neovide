use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::{
    io::{AsyncWrite, WriteHalf},
    net::TcpStream,
    process::ChildStdin,
};

#[pin_project(project = TxProj)]
pub enum TxWrapper {
    Child(#[pin] ChildStdin),
    Tcp(#[pin] WriteHalf<TcpStream>),
}

impl futures::io::AsyncWrite for TxWrapper {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.project() {
            TxProj::Child(inner) => inner.poll_write(cx, buf),
            TxProj::Tcp(inner) => inner.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.project() {
            TxProj::Child(inner) => inner.poll_flush(cx),
            TxProj::Tcp(inner) => inner.poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.project() {
            TxProj::Child(inner) => inner.poll_shutdown(cx),
            TxProj::Tcp(inner) => inner.poll_shutdown(cx),
        }
    }
}

pub trait WrapTx {
    fn wrap_tx(self) -> TxWrapper;
}

impl WrapTx for ChildStdin {
    fn wrap_tx(self) -> TxWrapper {
        TxWrapper::Child(self)
    }
}

impl WrapTx for WriteHalf<TcpStream> {
    fn wrap_tx(self) -> TxWrapper {
        TxWrapper::Tcp(self)
    }
}
