use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, ErrorKind, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use winit::event_loop::EventLoopProxy;

use crate::{
    settings::neovide_std_datapath,
    version::{BUILD_VERSION, release_channel},
    window::{EventPayload, EventTarget, UserEvent},
};

const CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(2);
const LISTENER_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRequest {
    pub version: String,
    pub files_to_open: Vec<String>,
    /// Directory to start nvim in, accounting for --chdir
    pub cwd: Option<String>,
    pub tabs: bool,
    pub new_window: bool,
    #[serde(default)]
    pub neovim_bin: Option<String>,
    #[serde(default)]
    pub neovim_args: Option<Vec<String>>,
}

impl HandoffRequest {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            version: BUILD_VERSION.to_owned(),
            files_to_open: Vec::new(),
            cwd: None,
            tabs: true,
            new_window: false,
            neovim_bin: None,
            neovim_args: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandoffResult {
    Accepted,
    NoListener,
    Failed(String),
    Rejected(String),
}

pub struct ListenerGuard {
    shutdown: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
    endpoint: std::path::PathBuf,
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);

        let _ = std::os::unix::net::UnixStream::connect(&self.endpoint);

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }

        let _ = std::fs::remove_file(&self.endpoint);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HandoffResponse {
    accepted: bool,
    version: String,
    error: Option<String>,
}

pub fn try_handoff(request: &HandoffRequest) -> HandoffResult {
    let endpoint = endpoint_path();
    let stream = match UnixStream::connect(&endpoint) {
        Ok(stream) => stream,
        Err(error) => match error.kind() {
            ErrorKind::NotFound | ErrorKind::ConnectionRefused => return HandoffResult::NoListener,
            _ => {
                return HandoffResult::Failed(format!(
                    "failed to connect to instance IPC listener at {}: {error}",
                    endpoint.display()
                ));
            }
        },
    };

    let _ = stream.set_read_timeout(Some(CLIENT_IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CLIENT_IO_TIMEOUT));

    let mut writer = match stream.try_clone() {
        Ok(stream) => BufWriter::new(stream),
        Err(error) => {
            return HandoffResult::Failed(format!(
                "failed to clone instance IPC stream for writing: {error}"
            ));
        }
    };

    if let Err(error) = write_message(&mut writer, request) {
        return HandoffResult::Failed(format!("failed to send instance IPC request: {error}"));
    }

    let mut reader = BufReader::new(stream);
    match read_message::<HandoffResponse, _>(&mut reader) {
        Ok(response) if response.accepted => HandoffResult::Accepted,
        Ok(response) => HandoffResult::Rejected(
            response.error.unwrap_or_else(|| "instance IPC request was rejected".to_owned()),
        ),
        Err(error) => {
            HandoffResult::Failed(format!("failed to read instance IPC response: {error}"))
        }
    }
}

pub fn start_listener(proxy: EventLoopProxy<EventPayload>) -> Result<ListenerGuard> {
    let endpoint = endpoint_path();
    fs::create_dir_all(neovide_std_datapath()).with_context(|| {
        format!("failed to create instance IPC data directory {}", neovide_std_datapath().display())
    })?;

    if endpoint.exists() {
        match UnixStream::connect(&endpoint) {
            Ok(_) => {
                bail!("instance IPC listener is already running at {}", endpoint.display());
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                fs::remove_file(&endpoint).with_context(|| {
                    format!("failed to remove stale instance IPC socket {}", endpoint.display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to validate existing instance IPC socket {}",
                        endpoint.display()
                    )
                });
            }
        }
    }

    let listener = UnixListener::bind(&endpoint).with_context(|| {
        format!("failed to bind instance IPC listener at {}", endpoint.display())
    })?;

    listener
        .set_nonblocking(true)
        .context("failed to configure instance IPC listener as nonblocking")?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let listener_proxy = proxy.clone();
    let listener_shutdown = shutdown.clone();
    let join_handle = thread::Builder::new()
        .name("instance-ipc-listener".to_owned())
        .spawn(move || {
            while !listener_shutdown.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Err(error) = handle_connection(stream, &listener_proxy) {
                            log::warn!("instance IPC connection failed: {error:#}");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(LISTENER_POLL_INTERVAL);
                    }
                    Err(error) => {
                        log::error!("instance IPC listener failed: {error}");
                        break;
                    }
                }
            }
        })
        .context("failed to spawn instance IPC listener thread")?;

    Ok(ListenerGuard { shutdown, join_handle: Some(join_handle), endpoint })
}

fn handle_connection(
    stream: std::os::unix::net::UnixStream,
    proxy: &EventLoopProxy<EventPayload>,
) -> Result<()> {
    let _ = stream.set_read_timeout(Some(CLIENT_IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CLIENT_IO_TIMEOUT));

    let mut reader = BufReader::new(stream.try_clone().context("failed to clone IPC stream")?);
    let request: HandoffRequest =
        read_message(&mut reader).context("failed to decode instance IPC request")?;

    let response = handle_request(request, proxy);
    let mut writer = BufWriter::new(stream);
    write_message(&mut writer, &response).context("failed to encode instance IPC response")
}

fn handle_request(
    request: HandoffRequest,
    proxy: &EventLoopProxy<EventPayload>,
) -> HandoffResponse {
    if request.new_window || !request.files_to_open.is_empty() {
        let payload = EventPayload {
            payload: UserEvent::OpenFiles {
                files: request.files_to_open,
                cwd: request.cwd,
                tabs: request.tabs,
                new_window: request.new_window,
                neovim_bin: request.neovim_bin,
                neovim_args: request.neovim_args,
            },
            target: EventTarget::Focused,
        };

        if let Err(error) = proxy.send_event(payload) {
            return HandoffResponse {
                accepted: false,
                version: BUILD_VERSION.to_owned(),
                error: Some(format!(
                    "failed to forward handoff request to app event loop: {error}"
                )),
            };
        }
    }

    HandoffResponse { accepted: true, version: BUILD_VERSION.to_owned(), error: None }
}

fn endpoint_path() -> std::path::PathBuf {
    neovide_std_datapath().join(format!("neovide-{}.sock", release_channel()))
}

fn write_message<T, W>(writer: &mut W, value: &T) -> Result<()>
where
    T: Serialize,
    W: Write,
{
    serde_json::to_writer(&mut *writer, value).context("failed to serialize JSON message")?;
    writer.write_all(b"\n").context("failed to terminate JSON message")?;
    writer.flush().context("failed to flush JSON message")
}

fn read_message<T, R>(reader: &mut R) -> Result<T>
where
    T: DeserializeOwned,
    R: BufRead,
{
    let mut buffer = Vec::new();
    let bytes_read =
        reader.read_until(b'\n', &mut buffer).context("failed to read JSON message")?;

    if bytes_read == 0 {
        bail!("connection closed before a JSON message was received");
    }

    if matches!(buffer.last(), Some(b'\n')) {
        buffer.pop();
    }

    serde_json::from_slice(&buffer).context("failed to deserialize JSON message")
}

#[cfg(test)]
mod tests {
    use super::{HandoffRequest, HandoffResponse, read_message, write_message};
    use crate::version::BUILD_VERSION;
    use std::io::Cursor;

    #[test]
    fn handoff_request_new_sets_build_version() {
        let request = HandoffRequest::new();
        assert_eq!(request.version, BUILD_VERSION);
        assert!(request.files_to_open.is_empty());
        assert!(request.cwd.is_none());
        assert!(request.tabs);
        assert!(!request.new_window);
    }

    #[test]
    fn json_line_roundtrip_preserves_request() {
        let request = HandoffRequest {
            files_to_open: vec!["~/project".into()],
            cwd: Some("/path/to/user".into()),
            tabs: false,
            new_window: true,
            ..HandoffRequest::new()
        };

        let mut encoded = Vec::new();
        write_message(&mut encoded, &request).unwrap();

        let mut cursor = Cursor::new(encoded);
        let decoded: HandoffRequest = read_message(&mut cursor).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn json_line_roundtrip_preserves_response() {
        let response =
            HandoffResponse { accepted: true, version: BUILD_VERSION.to_owned(), error: None };

        let mut encoded = Vec::new();
        write_message(&mut encoded, &response).unwrap();

        let mut cursor = Cursor::new(encoded);
        let decoded: HandoffResponse = read_message(&mut cursor).unwrap();

        assert_eq!(decoded, response);
    }
}
