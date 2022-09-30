//! This module contains adaptations of the functions found in
//! https://github.com/KillTheMule/nvim-rs/blob/master/src/create/tokio.rs

use std::{
    io::{self, Error, ErrorKind},
    process::Stdio,
};

use nvim_rs::{error::LoopError, neovim::Neovim, Handler};
use tokio::{
    io::{split, AsyncRead, AsyncWrite},
    net::{self, TcpStream},
    process::Command,
    spawn,
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub type ConnectionResult<W> = io::Result<(Neovim<W>, JoinHandle<Result<(), Box<LoopError>>>)>;
pub type NeovimWriter = Box<dyn futures::AsyncWrite + Send + Unpin + 'static>;

/// Connects to an existing Neovim instance.
///
/// Interprets `address` in the same way as `:help --server`: If it contains a `:` it's interpreted
/// as a TCP/IPv4/IPv6 address. Otherwise it's interpreted as a named pipe or Unix domain socket
/// path.
pub async fn connect<H: Handler<Writer = NeovimWriter>>(
    address: String,
    handler: H,
) -> io::Result<(Neovim<H::Writer>, JoinHandle<Result<(), Box<LoopError>>>)> {
    if address.contains(":") {
        connect_stream(TcpStream::connect(address).await?, handler).await
    } else {
        connect_ipc_socket(address, handler).await
    }
}

/// Spawns and connects to an embedded Neovim instance.
///
/// stdin/stdout will be rewritten to `Stdio::piped()`
pub async fn embed<H: Handler<Writer = NeovimWriter>>(
    cmd: &mut Command,
    handler: H,
) -> ConnectionResult<H::Writer> {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let reader = child
        .stdout
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdout"))?;
    let writer = child
        .stdin
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdin"))?;

    connect_neovim(reader, writer, handler).await
}

async fn connect_stream<H, S>(stream: S, handler: H) -> ConnectionResult<H::Writer>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    H: Handler<Writer = NeovimWriter>,
{
    let (reader, writer) = split(stream);
    connect_neovim(reader, writer, handler).await
}

async fn connect_neovim<H, R, W>(reader: R, writer: W, handler: H) -> ConnectionResult<H::Writer>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
    H: Handler<Writer = NeovimWriter>,
{
    let (neovim, io) =
        Neovim::<H::Writer>::new(reader.compat(), Box::new(writer.compat_write()), handler);
    let io_handle = spawn(io);

    Ok((neovim, io_handle))
}

#[cfg(windows)]
async fn connect_ipc_socket<H: Handler<Writer = NeovimWriter>>(
    address: String,
    handler: H,
) -> ConnectionResult<H::Writer> {
    connect_stream(
        net::windows::named_pipe::ClientOptions::new().open(address)?,
        handler,
    )
    .await
}

#[cfg(unix)]
async fn connect_ipc_socket<H: Handler<Writer = NeovimWriter>>(
    address: String,
    handler: H,
) -> ConnectionResult<H::Writer> {
    connect_stream(net::UnixStream::connect(address).await?, handler).await
}

#[cfg(not(any(unix, windows)))]
async fn connect_ipc_socket<H: Handler<Writer = NeovimWriter>>(
    _address: String,
    _handler: H,
) -> ConnectionResult<H::Writer> {
    Err(Error::new(
        ErrorKind::Unsupported,
        "IPC sockets are not supported on this platform",
    ))
}
