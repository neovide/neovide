//! Establishes RPC sessions with embedded or already running Neovim servers. The
//! transport comes from `NeovimInstance`. `NeovimSession::new` performs the
//! handshake and owns the RPC loop. After UI attachment, `bridge::run` watches
//! that loop and any child. For an embedded server, it transfers the child and
//! stderr reader to a supervisor that can outlive UI detachment after
//! `:connect`. The RPC setup adapts the Tokio creation code from nvim-rs.
//! https://github.com/KillTheMule/nvim-rs/blob/master/src/create/tokio.rs

#[cfg(debug_assertions)]
use core::fmt;
use std::collections::VecDeque;
use std::io::Error;
#[cfg(target_os = "windows")]
use std::io::ErrorKind;
use std::io::Result;
#[cfg(target_os = "windows")]
use std::process::Child;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

use anyhow::Context;
use nvim_rs::Handler;
use nvim_rs::Value;
use nvim_rs::call_args;
use nvim_rs::error::CallError;
use nvim_rs::error::LoopError;
use nvim_rs::neovim::Neovim;
use nvim_rs::rpc::IntoVal;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::BufReader;
use tokio::io::split;
use tokio::net::TcpStream;
#[cfg(not(target_os = "windows"))]
use tokio::process::Child;
use tokio::process::Command;
use tokio::spawn;
use tokio::task::JoinHandle;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncWriteCompatExt;

pub type NeovimWriter = Box<dyn futures::AsyncWrite + Send + Unpin + 'static>;

/// Stderr lines retained for reporting a failed handshake. The reader logs
/// every line and keeps draining after an ordinary `:connect` leaves the
/// embedded child running.
const MAX_STDERR_LINES: usize = 128;

type BoxedReader = Box<dyn AsyncRead + Send + Unpin + 'static>;
type BoxedWriter = Box<dyn AsyncWrite + Send + Unpin + 'static>;

fn is_windows_drive_path(address: &str) -> bool {
    let bytes = address.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Classify an address as TCP using `socket_address_tcp_host_end` from Neovim.
/// A colon after the first character marks TCP. A drive path such as
/// `C:/nvim/socket` is local. An address beginning with `:` is local too.
fn is_tcp_address(address: &str) -> bool {
    if is_windows_drive_path(address) {
        return false;
    }
    address.rfind(':').is_some_and(|index| index > 0)
}

/// Turn a bare pipe name or one using forward slashes into a Windows pipe path.
/// Canonical `\\.\pipe\nvim.123` names and drive paths such as `C:/nvim/socket`
/// pass through unchanged.
#[cfg(any(target_os = "windows", test))]
fn normalize_windows_pipe_address(address: String) -> String {
    const PIPE_PREFIX: &str = r"\\.\pipe\";
    const FORWARD_SLASH_PIPE_PREFIX: &str = "//./pipe/";

    if address.starts_with(PIPE_PREFIX) || is_windows_drive_path(&address) {
        address
    } else if let Some(name) = address.strip_prefix(FORWARD_SLASH_PIPE_PREFIX) {
        format!("{PIPE_PREFIX}{name}")
    } else {
        format!("{PIPE_PREFIX}{address}")
    }
}

/// A handshaken RPC connection, before or after `nvim_ui_attach`. `NeovimSession::new`
/// creates it. `bridge::run` consumes it after attachment. A server connection
/// has no child or stderr reader. An embedded connection holds both until
/// `supervise_process` transfers them to a separate task.
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
        f.debug_struct("NeovimSession").field("io_handle", &self.io_handle).finish()
    }
}

/// Keep an embedded child and its stderr reader alive beyond the UI channel.
/// `bridge::run` can drop the returned join handle when RPC closes. While the
/// runtime lives, the task continues waiting to reap the child. Once the child
/// exits, the task gives the stderr reader 500 ms to finish, then aborts it if
/// it is still running.
fn spawn_process_supervisor(
    mut child: Child,
    stderr_task: Option<JoinHandle<Vec<String>>>,
) -> JoinHandle<Result<ExitStatus>> {
    tokio::spawn(async move {
        // On Windows, this is std::process::Child. Polling avoids tying up a
        // blocking thread while an embedded server continues after :connect.
        // The supervisor remains alive to reap it when it eventually exits.
        #[cfg(target_os = "windows")]
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };

        #[cfg(not(target_os = "windows"))]
        let status = child.wait().await;

        if let Some(mut stderr_task) = stderr_task
            && tokio::time::timeout(Duration::from_millis(500), &mut stderr_task).await.is_err()
        {
            stderr_task.abort();
        }

        status
    })
}

impl NeovimSession {
    /// Transfer an embedded child and stderr reader to their supervisor.
    /// Returns `None` for a connection to an existing server, which Neovide
    /// does not own. Dropping the join handle detaches the task, not the child.
    pub fn supervise_process(&mut self) -> Option<JoinHandle<Result<ExitStatus>>> {
        let child = self.neovim_process.take()?;
        let stderr_task = self.stderr_task.take();
        Some(spawn_process_supervisor(child, stderr_task))
    }

    pub async fn new(
        instance: NeovimInstance,
        handler: impl Handler<Writer = NeovimWriter>,
        channel_config: Option<&[(Value, Value)]>,
    ) -> anyhow::Result<Self> {
        // This needs to be done before the process is spawned, since the file descriptors are
        // inherited on unix-like systems
        #[cfg(not(target_os = "windows"))]
        let stdin_fd = instance.forward_stdin();
        let (reader, writer, stderr_reader, neovim_process) = instance.connect().await?;
        // Drain stderr from the embedded child until the pipe closes. Only the
        // last few lines are retained for a failed handshake. The supervisor
        // owns the task later.
        let stderr_task = stderr_reader.map(|reader| {
            tokio::spawn(async move {
                let mut lines = VecDeque::with_capacity(MAX_STDERR_LINES);
                let mut reader = BufReader::new(reader).lines();
                while let Some(line) = reader.next_line().await.unwrap_or_default() {
                    log::error!("{line}");
                    if lines.len() == MAX_STDERR_LINES {
                        lines.pop_front();
                    }
                    lines.push_back(line);
                }
                lines.into_iter().collect::<Vec<_>>()
            })
        });
        let handshake_message = "HandshakeMessage";

        let handshake_res = Neovim::<NeovimWriter>::handshake(
            reader.compat(),
            Box::new(writer.compat_write()),
            handler,
            handshake_message,
        )
        .await;
        match handshake_res {
            Err(err) => {
                if let Some(mut child) = neovim_process {
                    #[cfg(target_os = "windows")]
                    {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                    }
                }
                if let Some(stderr_task) = stderr_task {
                    let stderr = "stderr output:\n".to_owned() + &stderr_task.await?.join("\n");
                    Err(err).context(stderr)
                } else {
                    Err(err.into())
                }
            }
            Ok((neovim, io)) => {
                let io_handle = spawn(io);

                if let Some(channel_config) = channel_config {
                    let restored = neovim
                        .call("nvim_chan_set", call_args![0, Value::Map(channel_config.to_vec())])
                        .await
                        .and_then(|response| response.map_err(Box::<CallError>::from));

                    if let Err(error) = restored {
                        io_handle.abort();
                        return Err(error).context("Could not restore Neovim channel properties");
                    }
                }

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

/// The transport requested for a new Neovim RPC session. Startup may spawn an
/// embedded child. `--server` and session transitions use `Server` to attach to
/// an already listening instance.
#[derive(Debug)]
pub enum NeovimInstance {
    /// A new embedded instance to be spawned by the given command.
    Embedded(Command),

    /// A native Windows child whose UI connects through its named pipe.
    /// The child retains an open stdin pipe so a detached server can live on.
    #[cfg(target_os = "windows")]
    SpawnedServer { command: Command, address: String },

    /// An address expected to name a listening instance. This variant never
    /// spawns a child. `is_tcp_address` selects TCP or a local endpoint. Local
    /// endpoints are Unix sockets on Unix and named pipes on Windows.
    Server { address: String },
}

impl NeovimInstance {
    async fn connect(
        self,
    ) -> Result<(BoxedReader, BoxedWriter, Option<BoxedReader>, Option<Child>)> {
        match self {
            NeovimInstance::Embedded(cmd) => Self::spawn_process(cmd).await,
            #[cfg(target_os = "windows")]
            NeovimInstance::SpawnedServer { command, address } => {
                Self::spawn_listening_process(command, address).await
            }
            NeovimInstance::Server { address } => Self::connect_to_server(address)
                .await
                .map(|(reader, writer)| (reader, writer, None, None)),
        }
    }

    #[cfg(target_os = "windows")]
    async fn spawn_listening_process(
        command: Command,
        address: String,
    ) -> Result<(BoxedReader, BoxedWriter, Option<BoxedReader>, Option<Child>)> {
        use std::os::windows::io::IntoRawHandle;

        use tokio::net::windows::named_pipe::NamedPipeServer;

        log::debug!("Starting listening neovim with: {command:?}");
        let mut child = command
            .into_std()
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        let stderr = child.stderr.take().ok_or_else(|| Error::other("Can't open stderr"))?;
        let deadline = Instant::now() + Duration::from_secs(10);

        let (reader, writer) = loop {
            match Self::connect_to_server(address.clone()).await {
                Ok(streams) => break streams,
                Err(error) => {
                    if let Some(status) = child.try_wait()? {
                        return Err(Error::other(format!(
                            "Neovim exited before listening at {address:?} with {status}: {error}"
                        )));
                    }

                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(Error::new(
                            ErrorKind::TimedOut,
                            format!("Neovim did not listen at {address:?}: {error}"),
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        };

        let stderr =
            Box::new(unsafe { NamedPipeServer::from_raw_handle(stderr.into_raw_handle()) }?)
                as BoxedReader;

        Ok((reader, writer, Some(stderr), Some(child)))
    }

    async fn spawn_process(
        #[cfg(not(target_os = "windows"))] mut cmd: Command,
        #[cfg(target_os = "windows")] cmd: Command,
    ) -> Result<(BoxedReader, BoxedWriter, Option<BoxedReader>, Option<Child>)> {
        log::debug!("Starting neovim with: {cmd:?}");

        // On Windows, the stdio pipes we get for a spawned child are overlapped
        // handles. if we pass those handles through tokio's process wrapper, it
        // turns them into Blocking<ArcFile>, which can eventually call
        // std::sys::pal::windows::handle::Handle::synchronous_read.
        //
        // Rust intentionally aborts on that path when an overlapped operation is
        // still pending instead of completing synchronously.
        //
        // See https://github.com/rust-lang/rust/issues/81357
        //
        // We avoid that path by using std::process::Command directly on
        // Windows, taking the raw pipe handles, and wrapping them in
        // NamedPipeServer, which supports overlapped pipe I/O.
        #[cfg(target_os = "windows")]
        let mut cmd = cmd.into_std();

        let mut child =
            cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let reader_inner = child.stdout.take().ok_or_else(|| Error::other("Can't open stdout"))?;
        let writer_inner = child.stdin.take().ok_or_else(|| Error::other("Can't open stdin"))?;
        let stderr_reader_inner =
            child.stderr.take().ok_or_else(|| Error::other("Can't open stderr"))?;

        let reader: BoxedReader;
        let writer: BoxedWriter;
        let stderr_reader: BoxedReader;

        #[cfg(not(target_os = "windows"))]
        {
            reader = Box::new(reader_inner);
            writer = Box::new(writer_inner);
            stderr_reader = Box::new(stderr_reader_inner);
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::io::IntoRawHandle;

            use tokio::net::windows::named_pipe::NamedPipeServer;
            reader = Box::new(unsafe {
                NamedPipeServer::from_raw_handle(reader_inner.into_raw_handle())
            }?);
            writer = Box::new(unsafe {
                NamedPipeServer::from_raw_handle(writer_inner.into_raw_handle())
            }?);
            stderr_reader = Box::new(unsafe {
                NamedPipeServer::from_raw_handle(stderr_reader_inner.into_raw_handle())
            }?);
        }

        Ok((reader, writer, Some(stderr_reader), Some(child)))
    }

    async fn connect_to_server(address: String) -> Result<(BoxedReader, BoxedWriter)> {
        if is_tcp_address(&address) {
            Ok(Self::split(TcpStream::connect(address).await?))
        } else {
            #[cfg(unix)]
            return Ok(Self::split(tokio::net::UnixStream::connect(address).await?));

            #[cfg(windows)]
            {
                let address = normalize_windows_pipe_address(address);
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
        use std::os::fd::AsFd;

        use rustix::fs::FileType;
        use rustix::fs::fstat;

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
                should_forward.then(|| rustix::io::dup(stdin).ok()).flatten()
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

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use std::io::Cursor;
    #[cfg(windows)]
    use std::pin::Pin;
    #[cfg(not(target_os = "windows"))]
    use std::process::Stdio;
    #[cfg(windows)]
    use std::sync::Arc;
    #[cfg(windows)]
    use std::sync::Mutex;
    #[cfg(windows)]
    use std::task::Context;
    #[cfg(windows)]
    use std::task::Poll;
    use std::time::Duration;

    use nvim_rs::Handler;
    use nvim_rs::Neovim;
    use nvim_rs::UiAttachOptions;
    use rmpv::Value;
    #[cfg(not(target_os = "windows"))]
    use tokio::io::AsyncBufReadExt;
    #[cfg(windows)]
    use tokio::io::AsyncRead;
    #[cfg(not(target_os = "windows"))]
    use tokio::io::BufReader;
    #[cfg(windows)]
    use tokio::io::ReadBuf;
    use tokio::process::Command;
    #[cfg(not(target_os = "windows"))]
    use tokio::sync::oneshot;
    #[cfg(windows)]
    use tokio_util::compat::TokioAsyncReadCompatExt;
    #[cfg(windows)]
    use tokio_util::compat::TokioAsyncWriteCompatExt;

    #[cfg(windows)]
    use super::BoxedReader;
    use super::NeovimInstance;
    #[cfg(not(target_os = "windows"))]
    use super::NeovimSession;
    use super::NeovimWriter;
    use super::is_tcp_address;
    use super::normalize_windows_pipe_address;
    #[cfg(not(target_os = "windows"))]
    use super::spawn_process_supervisor;

    #[cfg(windows)]
    struct CaptureReader {
        inner: BoxedReader,
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    #[cfg(windows)]
    impl AsyncRead for CaptureReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let before = buf.filled().len();
            let result = Pin::new(&mut self.inner).poll_read(cx, buf);
            if matches!(result, Poll::Ready(Ok(()))) {
                self.bytes.lock().unwrap().extend_from_slice(&buf.filled()[before..]);
            }
            result
        }
    }

    #[cfg(windows)]
    #[derive(Clone)]
    struct ProbeHandler(tokio::sync::mpsc::UnboundedSender<String>);

    #[cfg(windows)]
    impl Handler for ProbeHandler {
        type Writer = NeovimWriter;

        async fn handle_notify(&self, name: String, args: Vec<Value>, _nvim: Neovim<NeovimWriter>) {
            if name == "redraw" {
                for item in args {
                    if let Value::Array(parts) = item
                        && let Some(name) = parts.first().and_then(Value::as_str)
                        && matches!(name, "connect" | "error_exit")
                    {
                        let _ = self.0.send(name.to_owned());
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    fn has_connect(value: &Value) -> bool {
        match value {
            Value::Array(items) => {
                (items.first().and_then(Value::as_str) == Some("connect")
                    && items
                        .get(1)
                        .and_then(Value::as_array)
                        .and_then(|args| args.first())
                        .and_then(Value::as_str)
                        == Some("hey"))
                    || items.iter().any(has_connect)
            }
            _ => false,
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn embedded_connect_reaches_handler_after_stdio_eof() {
        let mut command = Command::new("nvim");
        command.args(["--clean", "--embed"]);
        let (reader, writer, _stderr, child) =
            NeovimInstance::spawn_process(command).await.unwrap();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let reader = CaptureReader { inner: reader, bytes: bytes.clone() };
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (nvim, io) = Neovim::<NeovimWriter>::handshake(
            reader.compat(),
            Box::new(writer.compat_write()),
            ProbeHandler(sender),
            "HandshakeMessage",
        )
        .await
        .unwrap();
        let mut io_handle = tokio::spawn(io);
        let mut options = UiAttachOptions::new();
        options.set_linegrid_external(true);
        nvim.ui_attach(80, 24, &options).await.unwrap();
        // The command may close its own RPC channel before replying to input.
        let _ = nvim.input(":connect hey<CR>").await;
        let io_result = tokio::time::timeout(Duration::from_secs(3), &mut io_handle).await;
        if io_result.is_err() {
            io_handle.abort();
        }
        let captured = bytes.lock().unwrap().clone();
        let mut cursor = Cursor::new(captured.as_slice());
        let mut raw_connect = false;
        while let Ok(value) = rmpv::decode::read_value(&mut cursor) {
            raw_connect |= has_connect(&value);
        }
        let mut delivered = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            delivered.push(event);
        }
        let mut child = child.unwrap();
        let _ = child.kill();
        let _ = child.wait();
        assert!(io_result.is_ok(), "stdio RPC did not close after :connect");
        assert!(raw_connect);
        assert!(delivered.iter().any(|event| event == "connect"));
        assert!(delivered.iter().any(|event| event == "error_exit"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn listening_embedded_connect_bang_reaches_handler_before_exit() {
        let address =
            format!(r"\\.\pipe\neovide-test.{}.{}", std::process::id(), rand::random::<u64>());
        let mut command = Command::new("nvim");
        command.args(["--clean", "--headless", "--listen", &address]);
        let (reader, writer, _stderr, child) =
            NeovimInstance::spawn_listening_process(command, address).await.unwrap();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let reader = CaptureReader { inner: reader, bytes: bytes.clone() };
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let (nvim, io) = Neovim::<NeovimWriter>::handshake(
            reader.compat(),
            Box::new(writer.compat_write()),
            ProbeHandler(sender),
            "HandshakeMessage",
        )
        .await
        .unwrap();
        let mut io_handle = tokio::spawn(io);
        let mut options = UiAttachOptions::new();
        options.set_linegrid_external(true);
        nvim.ui_attach(80, 24, &options).await.unwrap();
        let _ = nvim.input(":connect! hey<CR>").await;
        let io_result = tokio::time::timeout(Duration::from_secs(3), &mut io_handle).await;
        if io_result.is_err() {
            io_handle.abort();
        }
        let captured = bytes.lock().unwrap().clone();
        let mut cursor = Cursor::new(captured.as_slice());
        let mut raw_connect = false;
        while let Ok(value) = rmpv::decode::read_value(&mut cursor) {
            raw_connect |= has_connect(&value);
        }
        let mut delivered = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            delivered.push(event);
        }
        let mut child = child.unwrap();
        if io_result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait().unwrap();
        assert!(io_result.is_ok(), "RPC did not close after :connect!");
        assert!(status.success(), "old server exited with {status}");
        assert!(raw_connect);
        assert!(delivered.iter().any(|event| event == "connect"));
        assert!(delivered.iter().any(|event| event == "error_exit"));
    }

    #[cfg(not(target_os = "windows"))]
    #[derive(Clone)]
    struct TestUiHandler;

    #[cfg(not(target_os = "windows"))]
    impl Handler for TestUiHandler {
        type Writer = NeovimWriter;

        async fn handle_notify(
            &self,
            _event_name: String,
            _arguments: Vec<Value>,
            _neovim: Neovim<NeovimWriter>,
        ) {
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn failed_restart_clears_exitreason_on_surviving_server() {
        let bin = std::env::var("NEOVIM_BIN").unwrap_or_else(|_| "nvim".to_owned());
        let mut command = Command::new(bin);
        command.args(["--clean", "--embed"]);
        let mut session =
            NeovimSession::new(NeovimInstance::Embedded(command), TestUiHandler, None)
                .await
                .unwrap();
        let mut options = UiAttachOptions::new();
        options.set_linegrid_external(true);
        session.neovim.ui_attach(80, 24, &options).await.unwrap();
        session.neovim.command("set modified").await.unwrap();

        let error = session.neovim.command("restart").await.unwrap_err();
        assert!(error.to_string().contains("E37"));
        let reason = session.neovim.exec_lua("return vim.v.exitreason", vec![]).await.unwrap();
        assert!(reason.is_nil() || reason.as_str() == Some(""));

        let supervisor = session.supervise_process().unwrap();
        let _ = session.neovim.command("qa!").await;
        tokio::time::timeout(Duration::from_secs(5), supervisor).await.unwrap().unwrap().unwrap();
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn unavailable_destination_returns_connection_error() {
        let address = std::env::temp_dir()
            .join(format!("neovide-no-server-{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let result = NeovimInstance::Server { address }.connect().await;
        assert!(result.is_err());
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn detached_supervisor_keeps_embedded_process_and_stderr_reader_alive() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 0.1; printf 'server still alive\\n' >&2"])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, receiver) = oneshot::channel();
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let line = lines.next_line().await.unwrap().unwrap();
            let _ = sender.send(line.clone());
            vec![line]
        });

        // Dropping the join handle models run() after the old UI channel
        // closes.
        drop(spawn_process_supervisor(child, Some(stderr_task)));

        let line = tokio::time::timeout(Duration::from_secs(2), receiver).await.unwrap().unwrap();
        assert_eq!(line, "server still alive");
    }

    #[test]
    fn matches_neovim_address_selection() {
        assert!(is_tcp_address("127.0.0.1:6666"));
        assert!(is_tcp_address("[::1]:6666"));
        assert!(!is_tcp_address(":socket"));
        assert!(!is_tcp_address("C:/nvim/socket"));
        assert!(!is_tcp_address("/tmp/nvim.sock"));
    }

    #[test]
    fn preserves_canonical_windows_pipe_address() {
        let address = r"\\.\pipe\nvim.123".to_string();
        assert_eq!(normalize_windows_pipe_address(address.clone()), address);
    }

    #[test]
    fn normalizes_neovim_forward_slash_pipe_address() {
        assert_eq!(
            normalize_windows_pipe_address("//./pipe/nvim.123".to_string()),
            r"\\.\pipe\nvim.123"
        );
    }

    #[test]
    fn prefixes_bare_windows_pipe_name() {
        assert_eq!(normalize_windows_pipe_address("nvim.123".to_string()), r"\\.\pipe\nvim.123");
    }

    #[test]
    fn preserves_windows_drive_path() {
        let address = "C:/nvim/socket".to_string();
        assert_eq!(normalize_windows_pipe_address(address.clone()), address);
    }
}
