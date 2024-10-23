//! This module contains adaptations of the functions found in
//! https://github.com/KillTheMule/nvim-rs/blob/master/src/create/tokio.rs

#[cfg(debug_assertions)]
use core::fmt;
use std::{
    io::{Error, ErrorKind, Result},
    process::Stdio,
};

use nvim_rs::{error::LoopError, neovim::Neovim, Handler};
use tokio::{
    io::{split, AsyncRead, AsyncWrite},
    net::TcpStream,
    process::{Child, Command},
    spawn,
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub type NeovimWriter = Box<dyn futures::AsyncWrite + Send + Unpin + 'static>;

type BoxedReader = Box<dyn AsyncRead + Send + Unpin + 'static>;
type BoxedWriter = Box<dyn AsyncWrite + Send + Unpin + 'static>;

pub struct NeovimSession {
    pub neovim: Neovim<NeovimWriter>,
    pub io_handle: JoinHandle<std::result::Result<(), Box<LoopError>>>,
    pub neovim_process: Option<Child>,
}

#[cfg(debug_assertions)]
impl fmt::Debug for NeovimSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NeovimSession")
            .field("io_handle", &self.io_handle)
            .finish()
    }
}

impl NeovimSession {
    pub async fn new(
        instance: NeovimInstance,
        handler: impl Handler<Writer = NeovimWriter>,
    ) -> Result<Self> {
        let (reader, writer, neovim_process) = instance.connect().await?;
        let (neovim, io) =
            Neovim::<NeovimWriter>::new(reader.compat(), Box::new(writer.compat_write()), handler);
        let io_handle = spawn(io);

        Ok(Self {
            neovim,
            io_handle,
            neovim_process,
        })
    }
}

/// An existing or future Neovim instance along with a means for establishing a connection.
#[derive(Debug)]
pub enum NeovimInstance {
    /// A new embedded instance to be spawned by the given command.
    Embedded(Command),

    /// An existing instance listening on `address`.
    ///
    /// Interprets `address` in the same way as `:help --server`: If it contains a `:` it's
    /// interpreted as a TCP/IPv4/IPv6 address. Otherwise it's interpreted as a named pipe or Unix
    /// domain socket path. Spawns and connects to an embedded Neovim instance.
    Server { address: String },
}

impl NeovimInstance {
    async fn connect(self) -> Result<(BoxedReader, BoxedWriter, Option<Child>)> {
        match self {
            NeovimInstance::Embedded(cmd) => Self::spawn_process(cmd).await,
            NeovimInstance::Server { address } => Self::connect_to_server(address)
                .await
                .map(|(reader, writer)| (reader, writer, None)),
        }
    }

    async fn spawn_process(mut cmd: Command) -> Result<(BoxedReader, BoxedWriter, Option<Child>)> {
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let reader = Box::new(
            child
                .stdout
                .take()
                .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdout"))?,
        );
        let writer = Box::new(
            child
                .stdin
                .take()
                .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdin"))?,
        );

        Ok((reader, writer, Some(child)))
    }

    async fn connect_to_server(address: String) -> Result<(BoxedReader, BoxedWriter)> {
        if address.contains(':') {
            Ok(Self::split(TcpStream::connect(address).await?))
        } else {
            #[cfg(unix)]
            return Ok(Self::split(tokio::net::UnixStream::connect(address).await?));

            #[cfg(windows)]
            {
                // Fixup the address if the pipe on windows does not start with \\.\pipe\.
                let address = if address.starts_with("\\\\.\\pipe\\") {
                    address
                } else {
                    format!("\\\\.\\pipe\\{}", address)
                };
                Ok(Self::split(
                    tokio::net::windows::named_pipe::ClientOptions::new().open(address)?,
                ))
            }

            #[cfg(not(any(unix, windows)))]
            Err(Error::new(
                ErrorKind::Unsupported,
                "Unix Domain Sockets and Named Pipes are not supported on this platform",
            ))
        }
    }

    fn split(
        stream: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
    ) -> (BoxedReader, BoxedWriter) {
        let (reader, writer) = split(stream);
        (Box::new(reader), Box::new(writer))
    }
}
