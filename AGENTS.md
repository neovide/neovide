# Neovide IPC (minimal window control)

This document summarizes the minimal IPC feature introduced by the commit
"feat: add minimal IPC for window-level control" and describes the three IPC
interfaces as implemented in the current codebase.

## Overview
- Opt-in IPC server: enabled via CLI `--neovide-ipc` or config `neovide-ipc`.
- Transport:
  - macOS/Linux: Unix domain socket.
  - Windows: Named pipe.
- Address formats:
  - Unix: `unix:<path>` or any path containing `/`.
  - Windows: `pipe:<name>` or full `\\.\pipe\<name>`.
- Protocol: newline-delimited JSON-RPC 2.0 requests/responses.
- Threading: requests are forwarded to the UI thread via `EventLoopProxy` and
  responded through `oneshot` channels.

## JSON-RPC methods

### 1) ListWindows
- Method: `ListWindows`
- Params: none
- Request example:
  ```json
  {"jsonrpc":"2.0","id":1,"method":"ListWindows"}
  ```
- Response result:
  - Array of objects with:
    - `window_id` (string)
    - `is_active` (bool)
  ```json
  {"jsonrpc":"2.0","id":1,"result":[{"window_id":"42","is_active":true}]}
  ```
- Implementation:
  - `src/ipc/mod.rs` -> `handle_list_windows`
  - UI side: `WinitWindowWrapper::list_windows` returns all `routes` with
    active window marked via `get_focused_route()`.

### 2) ActivateWindow
- Method: `ActivateWindow`
- Params:
  - `window_id` (string; parsed to `u64` and converted into `WindowId`)
  ```json
  {"jsonrpc":"2.0","id":2,"method":"ActivateWindow","params":{"window_id":"42"}}
  ```
- Success response:
  ```json
  {"jsonrpc":"2.0","id":2,"result":{"ok":true}}
  ```
- Errors:
  - `-32602` if `window_id` is missing/invalid.
  - `-32000` if window not found or IPC dispatch fails.
- Implementation:
  - `src/ipc/mod.rs` -> `handle_activate_window`
  - UI side: `WinitWindowWrapper::activate_window` focuses the target window,
    and on macOS also calls `activate_application()`.

### 3) CreateWindow
- Method: `CreateWindow`
- Params (optional):
  - `nvim_args` (array of strings)
  ```json
  {"jsonrpc":"2.0","id":3,"method":"CreateWindow","params":{"nvim_args":["-S","session.vim"]}}
  ```
- Success response:
  ```json
  {"jsonrpc":"2.0","id":3,"result":{"window_id":"99"}}
  ```
- Errors:
  - `-32602` if `nvim_args` is not an array of strings.
  - `-32000` if creation fails or IPC dispatch fails.
- Implementation:
  - `src/ipc/mod.rs` -> `handle_create_window`
  - UI side: `WinitWindowWrapper::create_window_with_args` calls
    `try_create_window(..., Some(nvim_args))`, reusing existing Neovim args
    handling for per-window overrides.

## Error handling
- Parse error: `-32700` on invalid JSON.
- Method not found: `-32601`.
- Invalid params: `-32602`.
- Internal/dispatch errors: `-32000`.

## Notes
- `window_id` is serialized as a string (`u64` -> string) in responses.
- Requests are processed per-connection; each line is a complete JSON-RPC
  request, and each response is terminated with a newline.
