use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    runtime::Runtime,
    sync::oneshot,
};
use winit::event_loop::EventLoopProxy;

#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

#[cfg(windows)]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

use crate::window::{EventPayload, IpcRequest, IpcResponse, IpcWindowInfo, UserEvent};

#[derive(Debug)]
enum IpcEndpoint {
    #[cfg(unix)]
    Unix(PathBuf),
    #[cfg(windows)]
    NamedPipe(String),
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

pub fn start_ipc_server(address: String, proxy: EventLoopProxy<EventPayload>, runtime: &Runtime) {
    let endpoint = match parse_endpoint(&address) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            log::error!("Invalid IPC address {address:?}: {error}");
            return;
        }
    };

    runtime.spawn(async move {
        if let Err(error) = run_ipc_server(endpoint, proxy).await {
            log::error!("IPC server failed: {error}");
        }
    });
}

fn parse_endpoint(address: &str) -> Result<IpcEndpoint, String> {
    #[cfg(unix)]
    {
        if let Some(rest) = address.strip_prefix("unix:") {
            return Ok(IpcEndpoint::Unix(PathBuf::from(rest)));
        }

        if address.contains('/') {
            return Ok(IpcEndpoint::Unix(PathBuf::from(address)));
        }
    }

    #[cfg(windows)]
    {
        if let Some(rest) = address.strip_prefix("pipe:") {
            return Ok(IpcEndpoint::NamedPipe(normalize_pipe_name(rest)));
        }

        if address.starts_with(r"\\.\pipe\") {
            return Ok(IpcEndpoint::NamedPipe(address.to_string()));
        }
    }

    Err("unsupported IPC address format (expected unix:<path> or pipe:<name>)".to_string())
}

#[cfg(windows)]
fn normalize_pipe_name(name: &str) -> String {
    if name.starts_with(r"\\.\pipe\") {
        name.to_string()
    } else {
        format!(r"\\.\pipe\{name}")
    }
}

async fn run_ipc_server(
    endpoint: IpcEndpoint,
    proxy: EventLoopProxy<EventPayload>,
) -> anyhow::Result<()> {
    match endpoint {
        #[cfg(unix)]
        IpcEndpoint::Unix(path) => run_unix_server(path, proxy).await,
        #[cfg(windows)]
        IpcEndpoint::NamedPipe(name) => run_named_pipe_server(name, proxy).await,
    }
}

#[cfg(unix)]
async fn run_unix_server(path: PathBuf, proxy: EventLoopProxy<EventPayload>) -> anyhow::Result<()> {
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    let listener = UnixListener::bind(&path)?;
    loop {
        let (stream, _) = listener.accept().await?;
        let proxy = proxy.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_unix_connection(stream, proxy).await {
                log::warn!("IPC connection error: {error}");
            }
        });
    }
}

#[cfg(unix)]
async fn handle_unix_connection(
    stream: UnixStream,
    proxy: EventLoopProxy<EventPayload>,
) -> anyhow::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(());
        }

        if line.trim().is_empty() {
            continue;
        }

        let response = handle_json_rpc_line(&line, &proxy).await;
        let payload = serde_json::to_string(&response)?;
        write_half.write_all(payload.as_bytes()).await?;
        write_half.write_all(b"\n").await?;
    }
}

#[cfg(windows)]
async fn run_named_pipe_server(
    name: String,
    proxy: EventLoopProxy<EventPayload>,
) -> anyhow::Result<()> {
    loop {
        let server = ServerOptions::new().create(&name)?;
        let proxy = proxy.clone();
        tokio::spawn(async move {
            if let Err(error) = server.connect().await {
                log::warn!("IPC pipe connect error: {error}");
                return;
            }
            if let Err(error) = handle_named_pipe_connection(server, proxy).await {
                log::warn!("IPC connection error: {error}");
            }
        });
    }
}

#[cfg(windows)]
async fn handle_named_pipe_connection(
    mut server: NamedPipeServer,
    proxy: EventLoopProxy<EventPayload>,
) -> anyhow::Result<()> {
    let (read_half, mut write_half) = server.split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(());
        }

        if line.trim().is_empty() {
            continue;
        }

        let response = handle_json_rpc_line(&line, &proxy).await;
        let payload = serde_json::to_string(&response)?;
        write_half.write_all(payload.as_bytes()).await?;
        write_half.write_all(b"\n").await?;
    }
}

async fn handle_json_rpc_line(line: &str, proxy: &EventLoopProxy<EventPayload>) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(error) => {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {error}"),
                }),
            };
        }
    };

    let id = request.id.unwrap_or(Value::Null);

    match request.method.as_str() {
        "ListWindows" => handle_list_windows(id, proxy).await,
        "ActivateWindow" => handle_activate_window(id, request.params, proxy).await,
        "CreateWindow" => handle_create_window(id, request.params, proxy).await,
        _ => rpc_error(id, -32601, "Method not found".to_string()),
    }
}

async fn handle_list_windows(id: Value, proxy: &EventLoopProxy<EventPayload>) -> JsonRpcResponse {
    let (sender, receiver) = oneshot::channel();
    let event = EventPayload::new(
        UserEvent::IpcRequest(IpcRequest::ListWindows(sender)),
        winit::window::WindowId::from(0),
    );
    if proxy.send_event(event).is_err() {
        return rpc_error(id, -32000, "Failed to send IPC request".to_string());
    }

    match receiver.await {
        Ok(IpcResponse::ListWindows(windows)) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(Value::Array(windows.into_iter().map(window_info_to_value).collect())),
            error: None,
        },
        Ok(IpcResponse::Error(message)) => rpc_error(id, -32000, message),
        Ok(_) => rpc_error(id, -32000, "Unexpected IPC response".to_string()),
        Err(_) => rpc_error(id, -32000, "IPC response dropped".to_string()),
    }
}

async fn handle_activate_window(
    id: Value,
    params: Option<Value>,
    proxy: &EventLoopProxy<EventPayload>,
) -> JsonRpcResponse {
    let window_id = match parse_window_id(params) {
        Ok(value) => value,
        Err(message) => return rpc_error(id, -32602, message),
    };

    let (sender, receiver) = oneshot::channel();
    let event = EventPayload::new(
        UserEvent::IpcRequest(IpcRequest::ActivateWindow { window_id, reply: sender }),
        winit::window::WindowId::from(0),
    );

    if proxy.send_event(event).is_err() {
        return rpc_error(id, -32000, "Failed to send IPC request".to_string());
    }

    match receiver.await {
        Ok(IpcResponse::Ok) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(Value::Object(serde_json::Map::from_iter([(
                "ok".to_string(),
                Value::Bool(true),
            )]))),
            error: None,
        },
        Ok(IpcResponse::Error(message)) => rpc_error(id, -32000, message),
        Ok(_) => rpc_error(id, -32000, "Unexpected IPC response".to_string()),
        Err(_) => rpc_error(id, -32000, "IPC response dropped".to_string()),
    }
}

async fn handle_create_window(
    id: Value,
    params: Option<Value>,
    proxy: &EventLoopProxy<EventPayload>,
) -> JsonRpcResponse {
    let nvim_args = match parse_nvim_args(params) {
        Ok(args) => args,
        Err(message) => return rpc_error(id, -32602, message),
    };

    let (sender, receiver) = oneshot::channel();
    let event = EventPayload::new(
        UserEvent::IpcRequest(IpcRequest::CreateWindow { nvim_args, reply: sender }),
        winit::window::WindowId::from(0),
    );

    if proxy.send_event(event).is_err() {
        return rpc_error(id, -32000, "Failed to send IPC request".to_string());
    }

    match receiver.await {
        Ok(IpcResponse::Created(window_id)) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(Value::Object(serde_json::Map::from_iter([(
                "window_id".to_string(),
                        Value::String(window_id_to_string(window_id)),
            )]))),
            error: None,
        },
        Ok(IpcResponse::Error(message)) => rpc_error(id, -32000, message),
        Ok(_) => rpc_error(id, -32000, "Unexpected IPC response".to_string()),
        Err(_) => rpc_error(id, -32000, "IPC response dropped".to_string()),
    }
}

fn parse_window_id(params: Option<Value>) -> Result<winit::window::WindowId, String> {
    let params = params.ok_or_else(|| "Missing params".to_string())?;
    let window_id_value = params
        .get("window_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Missing window_id".to_string())?;
    let raw_id = window_id_value
        .parse::<u64>()
        .map_err(|_| "Invalid window_id".to_string())?;
    Ok(winit::window::WindowId::from(raw_id))
}

fn parse_nvim_args(params: Option<Value>) -> Result<Vec<String>, String> {
    let Some(params) = params else {
        return Ok(Vec::new());
    };

    let Some(args) = params.get("nvim_args").and_then(|value| value.as_array()) else {
        return Ok(Vec::new());
    };

    let mut result = Vec::with_capacity(args.len());
    for value in args {
        let arg = value
            .as_str()
            .ok_or_else(|| "nvim_args must be strings".to_string())?;
        result.push(arg.to_string());
    }

    Ok(result)
}

fn window_info_to_value(info: IpcWindowInfo) -> Value {
    Value::Object(serde_json::Map::from_iter([
        (
            "window_id".to_string(),
            Value::String(window_id_to_string(info.window_id)),
        ),
        ("is_active".to_string(), Value::Bool(info.is_active)),
    ]))
}

fn window_id_to_string(window_id: winit::window::WindowId) -> String {
    let raw: u64 = window_id.into();
    raw.to_string()
}

fn rpc_error(id: Value, code: i64, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError { code, message }),
    }
}
