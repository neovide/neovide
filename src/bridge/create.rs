//! This module contains adaptations of the functions found in
//! https://github.com/KillTheMule/nvim-rs/blob/master/src/create/tokio.rs

use std::{
    io::{self, Error, ErrorKind},
    process::Stdio,
};

use nvim_rs::{error::LoopError, neovim::Neovim, Handler};
use tokio::{
    io::{split, AsyncRead, AsyncWrite},
    net::{TcpStream, ToSocketAddrs},
    process::Command,
    spawn,
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub type NeovimWriter = Box<dyn futures::AsyncWrite + Send + Unpin + 'static>;

/// Connect to a neovim instance
async fn connect<H, R, W>(
    reader: R,
    writer: W,
    handler: H,
) -> io::Result<(Neovim<H::Writer>, JoinHandle<Result<(), Box<LoopError>>>)>
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

/// Connect to a neovim instance via tcp.
pub async fn new_tcp<A, H>(
    addr: A,
    handler: H,
) -> io::Result<(Neovim<H::Writer>, JoinHandle<Result<(), Box<LoopError>>>)>
where
    A: ToSocketAddrs,
    H: Handler<Writer = NeovimWriter>,
{
    let stream = TcpStream::connect(addr).await?;
    let (reader, writer) = split(stream);

    connect(reader, writer, handler).await
}

/// Connect to a neovim instance by spawning a new one
///
/// stdin/stdout will be rewritten to `Stdio::piped()`
pub async fn new_child_cmd<H>(
    cmd: &mut Command,
    handler: H,
) -> io::Result<(Neovim<H::Writer>, JoinHandle<Result<(), Box<LoopError>>>)>
where
    H: Handler<Writer = NeovimWriter>,
{
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let reader = child
        .stdout
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdout"))?;
    let writer = child
        .stdin
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdin"))?;

    connect(reader, writer, handler).await
}
