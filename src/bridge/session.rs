//! This module contains adaptations of the functions found in
//! https://github.com/KillTheMule/nvim-rs/blob/master/src/create/tokio.rs

#[cfg(debug_assertions)]
use core::fmt;
use std::{
    io::{Error, Result},
    process::Stdio,
};

use anyhow::Context;
use nvim_rs::{error::LoopError, neovim::Neovim, Handler};
use tokio::{
    io::{split, AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader},
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
    pub stderr_task: Option<JoinHandle<Vec<String>>>,
    #[cfg(not(target_os = "windows"))]
    pub stdin_fd: Option<rustix::fd::OwnedFd>,
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
    ) -> anyhow::Result<Self> {
        // This needs to be done before the process is spawned, since the file descriptors are
        // inherited on unix-like systems
        #[cfg(not(target_os = "windows"))]
        let stdin_fd = instance.forward_stdin();
        let (reader, writer, stderr_reader, neovim_process) = instance.connect().await?;
        // Spawn a background task to read from stderr
        let stderr_task = stderr_reader.map(|reader| {
            tokio::spawn(async move {
                let mut lines = Vec::new();
                let mut reader = BufReader::new(reader).lines();
                while let Some(line) = reader.next_line().await.unwrap_or_default() {
                    log::error!("{line}");
                    lines.push(line);
                }
                lines
            })
        });
        let handshake_message = "NeovideToNeovimMagicHandshakeMessage";

        let handshake_res = Neovim::<NeovimWriter>::handshake(
            reader.compat(),
            Box::new(writer.compat_write()),
            handler,
            handshake_message,
        )
        .await;
        match handshake_res {
            Err(err) => {
                if let Some(stderr_task) = stderr_task {
                    let stderr = "stderr output:\n".to_owned() + &stderr_task.await?.join("\n");
                    Err(err).context(stderr)
                } else {
                    Err(err.into())
                }
            }
            Ok((neovim, io)) => {
                let io_handle = spawn(io);

                Ok(Self {
                    neovim,
                    io_handle,
                    neovim_process,
                    stderr_task,
                    #[cfg(not(target_os = "windows"))]
                    stdin_fd,
                })
            }
        }
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
    async fn connect(
        self,
    ) -> Result<(BoxedReader, BoxedWriter, Option<BoxedReader>, Option<Child>)> {
        match self {
            NeovimInstance::Embedded(cmd) => Self::spawn_process(cmd).await,
            NeovimInstance::Server { address } => Self::connect_to_server(address)
                .await
                .map(|(reader, writer)| (reader, writer, None, None)),
        }
    }

    async fn spawn_process(
        mut cmd: Command,
    ) -> Result<(BoxedReader, BoxedWriter, Option<BoxedReader>, Option<Child>)> {
        log::debug!("Starting neovim with: {cmd:?}");
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let reader = Box::new(
            child
                .stdout
                .take()
                .ok_or_else(|| Error::other("Can't open stdout"))?,
        );
        let writer = Box::new(
            child
                .stdin
                .take()
                .ok_or_else(|| Error::other("Can't open stdin"))?,
        );

        let stderr_reader = Box::new(
            child
                .stderr
                .take()
                .ok_or_else(|| Error::other("Can't open stderr"))?,
        );

        Ok((reader, writer, Some(stderr_reader), Some(child)))
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
                    format!("\\\\.\\pipe\\{address}")
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

    #[cfg(not(target_os = "windows"))]
    fn forward_stdin(&self) -> Option<rustix::fd::OwnedFd> {
        use rustix::fs::{fstat, FileType};
        use std::os::fd::AsFd;

        // stdin should be forwarded only in embedded mode when stdio is piped or redirected
        match self {
            Self::Embedded(..) => {
                let stdin = std::io::stdin();
                let should_forward = fstat(stdin.as_fd())
                    .map(|stat| match FileType::from_raw_mode(stat.st_mode) {
                        FileType::RegularFile => true,
                        #[cfg(not(target_os = "wasi"))]
                        FileType::Fifo | FileType::Socket => true,
                        _ => false,
                    })
                    .unwrap_or(false);

                // We have to use rustix here, since the Rust standard library currently sets O_CLOEXEC
                // on all file handles. And there's no way to pass file handles to subprocesses.
                // See [Tracking Issue for std::os::fd::CommandExt::fd](https://github.com/rust-lang/rust/issues/144989)
                should_forward
                    .then(|| rustix::io::dup(stdin).ok())
                    .flatten()
            }
            Self::Server { .. } => None,
        }
    }

    fn split(
        stream: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
    ) -> (BoxedReader, BoxedWriter) {
        let (reader, writer) = split(stream);
        (Box::new(reader), Box::new(writer))
    }
}
