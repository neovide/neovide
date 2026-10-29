//! Sends UI commands to the neovim connection belonging to each route. The
//! command tasks persist across session switches. Session setup replaces the
//! neovim handle they read when executing each command. A separate registry
//! selects a route for callbacks that have no `RouteId`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;

use anyhow::Context;
use anyhow::Result;
use indoc::indoc;
use log::trace;
use nvim_rs::Neovim;
use nvim_rs::call_args;
use nvim_rs::error::CallError;
use nvim_rs::rpc::model::IntoVal;
use rmpv::Value;
use strum::AsRefStr;
use tokio::sync::mpsc::unbounded_channel;

use super::NeovimHandler;
use super::RedrawEvent;
use super::Settings;
use super::StartupMessage;
use super::set_background_if_allowed;
use super::show_error_message;
use super::show_error_status;
use super::show_startup_message;
use crate::bridge::NeovimWriter;
use crate::bridge::nvim_dict;
use crate::cmd_line::CmdLineSettings;
#[cfg(feature = "profiling")]
use crate::profiling::scroll_tick_count;
#[cfg(all(feature = "profiling", target_os = "macos"))]
use crate::profiling::scroll_to_exec_lua_latency;
use crate::profiling::tracy_dynamic_zone;
use crate::utils::handle_wslpaths;
use crate::window::RouteId;

// nvim_get_option_info2 reports this when an option
// was last set through an API client, which is how ui_attach applies ext_messages.
const SID_API_CLIENT: i64 = -9;

/// Route handlers and the target for callers without a `RouteId`.
/// `start_ui_command_handler` registers each route, including after a switch.
/// Registration only selects a route when none is active. Window creation and
/// focus select another route. Removing the active route selects the lowest
/// surviving `RouteId`, or leaves `active_route` empty when the map is empty.
/// Keeping the map and selection under one lock stops a background attach or
/// close from redirecting global menu callbacks. The selection is a command
/// target, not proof that its Neovim channel has finished attaching.
struct HandlerRegistry<T> {
    by_route: HashMap<RouteId, T>,
    active_route: Option<RouteId>,
}

impl<T> Default for HandlerRegistry<T> {
    fn default() -> Self { Self { by_route: HashMap::new(), active_route: None } }
}

impl<T> HandlerRegistry<T> {
    fn register(&mut self, route_id: RouteId, handler: T) {
        self.by_route.insert(route_id, handler);
        if self.active_route.is_none() {
            self.active_route = Some(route_id);
        }
    }

    fn activate(&mut self, route_id: RouteId) {
        if self.by_route.contains_key(&route_id) {
            self.active_route = Some(route_id);
        }
    }

    fn unregister(&mut self, route_id: RouteId) {
        self.by_route.remove(&route_id);
        if self.active_route == Some(route_id) {
            self.active_route = self.by_route.keys().min().copied();
        }
    }

    fn active(&self) -> Option<&T> {
        self.active_route.and_then(|route_id| self.by_route.get(&route_id))
    }
}

static HANDLER_REGISTRY: LazyLock<Mutex<HandlerRegistry<NeovimHandler>>> =
    LazyLock::new(|| Mutex::new(HandlerRegistry::default()));

/// A macOS file-open request held until dispatch is permitted. The tuple stores
/// a path and an optional tab choice. `None` uses the current
/// `CmdLineSettings::tabs` value when the command runs.
/// `send_or_queue_file_drop` queues it until the first attach has completed and
/// an active handler exists. The attach flag stays set after that, so it does
/// not verify a later attach.
#[cfg(target_os = "macos")]
type PendingFileDrop = (String, Option<bool>);

#[cfg(target_os = "macos")]
static PENDING_FILE_DROPS: LazyLock<Mutex<Vec<PendingFileDrop>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Whether any session has completed `nvim_ui_attach` in this process. A
/// successful attach sets this flag and nothing resets it. It keeps cold
/// startup file opens from running before the initial UI attach completes, but
/// does not establish that the selected route has finished a later attach.
#[cfg(target_os = "macos")]
static FILE_DROP_HANDLER_READY: AtomicBool = AtomicBool::new(false);

pub fn get_active_handler() -> Option<NeovimHandler> {
    HANDLER_REGISTRY.lock().unwrap().active().cloned()
}

#[cfg(target_os = "macos")]
pub fn require_active_handler() -> NeovimHandler {
    get_active_handler().expect("NeovimHandler has not been initialized")
}

#[cfg(target_os = "macos")]
pub fn send_or_queue_file_drop(path: String, tabs: Option<bool>) {
    if FILE_DROP_HANDLER_READY.load(Ordering::SeqCst)
        && let Some(handler) = get_active_handler()
    {
        send_ui(ParallelCommand::FileDrop { path, tabs }, &handler);
        return;
    }

    PENDING_FILE_DROPS.lock().unwrap().push((path, tabs));
}

#[cfg(target_os = "macos")]
fn flush_pending_file_drops(handler: &NeovimHandler) {
    let pending = {
        let mut pending = PENDING_FILE_DROPS.lock().unwrap();
        std::mem::take(&mut *pending)
    };

    for (path, tabs) in pending {
        send_ui(ParallelCommand::FileDrop { path, tabs }, handler);
    }
}

#[cfg(target_os = "macos")]
fn flush_pending_file_drops_when_ready() {
    if FILE_DROP_HANDLER_READY.load(Ordering::SeqCst)
        && let Some(handler) = get_active_handler()
    {
        flush_pending_file_drops(&handler);
    }
}

#[cfg(target_os = "macos")]
/// Record a successful UI attachment and send queued file opens to the
/// currently selected route, if one is registered.
pub(crate) fn mark_file_drop_handler_ready() {
    FILE_DROP_HANDLER_READY.store(true, Ordering::SeqCst);
    flush_pending_file_drops_when_ready();
}

async fn ime_call(
    nvim: &Neovim<NeovimWriter>,
    func: &str,
    args: Vec<Value>,
    context: &'static str,
    trace_msg: &'static str,
) -> Result<()> {
    nvim.call("nvim__exec_lua_fast", call_args![func, args])
        .await
        .map(|_| trace!("{trace_msg}"))
        .context(context)
}

/// The window and buffer that produced an absolute scroll request.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScrollTarget {
    window: i64,
    buffer: i64,
}

/// An absolute view position, with one-based buffer line numbers for Neovim.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScrollToRequest {
    target: ScrollTarget,
    topline: i64,
    cursor_lnum: i64,
}

#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
enum ScrollToOutcome {
    /// No retry is needed. The view was restored or a stale target was ignored.
    Finished,
    /// Terminal-mode was asked to exit. Retry this buffer after the mode barrier.
    RetryAfterTerminalExit { buffer: i64 },
}

/// Restores a view, leaving terminal input mode first when the target requires it.
///
/// most scrolls restores the view through `vim.fn.winrestview()` in a single
/// request. the exception is the current window while its terminal buffer is
/// in input mode. `stopinsert` requests a mode exit, but doesn't finish it
/// inside that call. terminal cleanup then restores the live cursor and topline,
/// which would overwrite a scrollback view restored too soon. an inactive terminal
/// or one already outside input mode, needs no handoff.
///
/// See https://neovim.io/doc/user/insert/#%3Astopinsert
///
/// the first call therefore requests the exit and returns the buffer without
/// restoring the view. `get_mode()` is the barrier before the second call:
/// neovim defers this rpc until it is waiting for input if it isn't already.
/// that separates the pending mode exit and cursor cleanup from the scrollback
/// restore. this approach uses two requests plus the intervening mode request.
///
/// the mode response isn't polled or inspected here. the second call checks
/// the mode again before restoring the view. if it requests another terminal
/// exit, this fn returns an error rather than retrying indefinitely.
///
/// the serial executor awaits this whole operation as one [`SerialCommand::ScrollTo`].
/// later keyboard and scroll commands on the same route stay queued until the
/// handoff finishes. putting the retry back on the queue would let those commands
/// run between the exit and the restore. this ordering doesn't lock neovim or
/// prevent autocommands and other clients from changing state, so it doesn't
/// replace the target checks.
#[cfg(target_os = "macos")]
async fn scroll_to_call(nvim: &Neovim<NeovimWriter>, request: ScrollToRequest) -> Result<()> {
    trace!("ScrollTo execute {request:?}");
    let ScrollToOutcome::RetryAfterTerminalExit { buffer } =
        scroll_to_request(nvim, request).await?
    else {
        return Ok(());
    };

    // terminal-mode restores the live cursor when it exits so we wait for that work
    // before restoring the scrollback view without releasing the serial queue.
    nvim.get_mode().await.context("Waiting for terminal to leave input mode failed")?;
    let retry = ScrollToRequest { target: ScrollTarget { buffer, ..request.target }, ..request };
    match scroll_to_request(nvim, retry).await? {
        ScrollToOutcome::Finished => Ok(()),
        ScrollToOutcome::RetryAfterTerminalExit { .. } => {
            anyhow::bail!("Terminal re-entered input mode while scrolling")
        }
    }
}

/// Restores the view or identifies a terminal buffer that needs a deferred retry.
/// both attempts must still target the buffer that produced the scroll command.
#[cfg(target_os = "macos")]
async fn scroll_to_request(
    nvim: &Neovim<NeovimWriter>,
    request: ScrollToRequest,
) -> Result<ScrollToOutcome> {
    const SCROLL_TO_LUA: &str = indoc! {"
        local win, topline, cursor_lnum, expected_buffer = ...
        if not vim.api.nvim_win_is_valid(win) then
          return
        end
        local buffer = vim.api.nvim_win_get_buf(win)
        if buffer ~= expected_buffer then
          return
        end
        if win == vim.api.nvim_get_current_win()
            and vim.bo[buffer].buftype == 'terminal'
            and vim.api.nvim_get_mode().mode == 't' then
          vim.cmd('stopinsert')
          return buffer
        end
        vim.api.nvim_win_call(win, function()
          local view = vim.fn.winsaveview()
          view.topline = topline
          view.lnum = cursor_lnum
          vim.fn.winrestview(view)
          vim.api.nvim__redraw({
            win = win,
            cursor = true,
            flush = true,
          })
        end)
    "};

    let args = call_args![
        request.target.window,
        request.topline,
        request.cursor_lnum,
        request.target.buffer,
    ];

    #[cfg(feature = "profiling")]
    let rpc_start = std::time::Instant::now();

    let result = nvim.exec_lua(SCROLL_TO_LUA, args).await;

    #[cfg(feature = "profiling")]
    scroll_to_exec_lua_latency(rpc_start.elapsed());

    let result = result.context("ScrollTo failed")?;
    if result.is_nil() {
        Ok(ScrollToOutcome::Finished)
    } else {
        let buffer = result.as_i64().context("ScrollTo returned an invalid terminal buffer")?;
        Ok(ScrollToOutcome::RetryAfterTerminalExit { buffer })
    }
}

async fn scroll_call(
    nvim: &Neovim<NeovimWriter>,
    direction: &str,
    grid_id: u64,
    position: (u32, u32),
    modifier_string: &str,
    count: u32,
) -> Result<()> {
    if count == 0 {
        return Ok(());
    }

    #[cfg(feature = "profiling")]
    scroll_tick_count(count);

    let (grid_x, grid_y) = position;
    for _ in 0..count {
        nvim.input_mouse(
            "wheel",
            direction,
            modifier_string,
            grid_id as i64,
            grid_y as i64,
            grid_x as i64,
        )
        .await
        .context("Mouse Scroll Failed")?;
    }

    Ok(())
}

// Serial commands are any commands which must complete before the next value is sent. This
// includes keyboard and mouse input which would cause problems if sent out of order.
//
// When in doubt, use Parallel Commands.
#[derive(Clone, Debug, AsRefStr)]
pub enum SerialCommand {
    Keyboard(String),
    KeyboardImeCommit {
        formatted: String,
        raw: String,
    },
    KeyboardImePreedit {
        raw: String,
        cursor_offset: Option<(usize, usize)>,
    },
    MouseButton {
        button: String,
        action: String,
        grid_id: u64,
        position: (u32, u32),
        modifier_string: String,
    },
    Scroll {
        direction: String,
        grid_id: u64,
        position: (u32, u32),
        count: u32,
        modifier_string: String,
    },
    Drag {
        button: String,
        grid_id: u64,
        position: (u32, u32),
        modifier_string: String,
    },
    #[cfg(target_os = "macos")]
    ScrollTo {
        window: i64,
        buffer: i64,
        topline: i64,
        cursor_lnum: i64,
    },
    #[cfg(target_os = "macos")]
    ForceClickCommand,
}

/// Fields that must match before adjacent scroll commands can be merged.
///
/// neovim interprets wheel input using the target grid position and modifier
/// state, so these fields are part of the merge key alongside direction.
#[derive(PartialEq, Eq)]
struct ScrollMergeKey<'a> {
    direction: &'a str,
    grid_id: u64,
    position: (u32, u32),
    modifier_string: &'a str,
}

impl SerialCommand {
    pub fn scroll(
        direction: impl Into<String>,
        grid_id: u64,
        position: (u32, u32),
        count: u32,
        modifier_string: String,
    ) -> Self {
        Self::Scroll { direction: direction.into(), grid_id, position, count, modifier_string }
    }

    /// Returns the comparable part of a scroll command.
    ///
    /// non-scroll commands don't have a merge key and must remain as ordering
    /// barriers in the serial command stream.
    fn scroll_merge_key(&self) -> Option<ScrollMergeKey<'_>> {
        match self {
            Self::Scroll { direction, grid_id, position, modifier_string, .. } => {
                Some(ScrollMergeKey {
                    direction,
                    grid_id: *grid_id,
                    position: *position,
                    modifier_string,
                })
            }
            _ => None,
        }
    }

    /// Attempts to absorb the next_command into this command.
    fn try_merge_scroll(&mut self, next_command: Self) -> Option<Self> {
        let can_merge = match (self.scroll_merge_key(), next_command.scroll_merge_key()) {
            (Some(current), Some(next)) => current == next,
            _ => false,
        };

        if !can_merge {
            return Some(next_command);
        }

        match (self, next_command) {
            (Self::Scroll { count, .. }, Self::Scroll { count: next_count, .. }) => {
                *count = count.saturating_add(next_count);
                None
            }
            (_, other) => Some(other),
        }
    }

    /// Replaces a queued position only for the same buffer in the same window.
    /// Other targets, wheel ticks and keyboard input remain ordering barriers.
    #[cfg(target_os = "macos")]
    fn try_merge_scroll_to(&mut self, next: Self) -> Option<Self> {
        if matches!((&*self, &next),
            (
                Self::ScrollTo { window, buffer, .. },
                Self::ScrollTo { window: next_window, buffer: next_buffer, .. },
            ) if window == next_window && buffer == next_buffer)
        {
            *self = next;
            None
        } else {
            Some(next)
        }
    }

    async fn execute(self, nvim: &Neovim<NeovimWriter>, can_support_ime_api: bool) {
        // Don't panic here unless there's absolutely no chance of continuing the program, Instead
        // just log the error and hope that it's something temporary or recoverable A normal reason
        // for failure is when neovim has already quit, and a command, for example mouse move is
        // being sent
        log::trace!("In Serial Command");
        let result = match self {
            SerialCommand::Keyboard(input_command) => {
                trace!("Keyboard Input Sent: {input_command}");
                nvim.input(&input_command).await.map(|_| ()).context("Input failed")
            }
            SerialCommand::KeyboardImeCommit { formatted, raw } => {
                // Notified ime commit event, the text is guaranteed not to be None.
                trace!("IME Input Sent: {formatted}");
                if can_support_ime_api {
                    ime_call(
                        nvim,
                        "neovide.commit_handler(...)",
                        vec![Value::from(raw), Value::from(formatted)],
                        "IME Commit failed",
                        "IME Commit Called",
                    )
                    .await
                } else {
                    trace!("Keyboard Input Sent: {formatted}");
                    nvim.input(&formatted).await.map(|_| ()).context("Input failed")
                }
            }
            SerialCommand::KeyboardImePreedit { raw, cursor_offset } => {
                trace!("IME Input Preedit");
                if can_support_ime_api {
                    let (start_col, end_col) = cursor_offset
                        .map_or((Value::Nil, Value::Nil), |(start, end)| {
                            (Value::from(start), Value::from(end))
                        });

                    ime_call(
                        nvim,
                        "neovide.preedit_handler(...)",
                        vec![Value::from(raw), start_col, end_col],
                        "IME Preedit failed",
                        "IME Preedit Called",
                    )
                    .await
                } else {
                    Ok(())
                }
            }
            SerialCommand::MouseButton {
                button,
                action,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => nvim
                .input_mouse(
                    &button,
                    &action,
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .context("Mouse input failed"),
            SerialCommand::Scroll {
                direction,
                grid_id,
                position: (grid_x, grid_y),
                count,
                modifier_string,
            } => {
                scroll_call(nvim, &direction, grid_id, (grid_x, grid_y), &modifier_string, count)
                    .await
            }
            SerialCommand::Drag {
                button,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => nvim
                .input_mouse(
                    &button,
                    "drag",
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .context("Mouse Drag Failed"),
            #[cfg(target_os = "macos")]
            SerialCommand::ScrollTo { window, buffer, topline, cursor_lnum } => {
                let request = ScrollToRequest {
                    target: ScrollTarget { window, buffer },
                    topline,
                    cursor_lnum,
                };
                scroll_to_call(nvim, request).await
            }
            #[cfg(target_os = "macos")]
            SerialCommand::ForceClickCommand => {
                nvim.command("NeovideForceClick").await.context("Force click command failed")
            }
        };

        if let Err(error) = result {
            log::error!("{error:?}");
        }
    }
}

fn normalize_file_drop_path(path: String, windows: bool) -> String {
    // Ex parsing can consume a Windows separator before escapable filename characters such as `(`.
    if windows { path.replace('\\', "/") } else { path }
}

#[derive(Debug, Clone, AsRefStr)]
pub enum ParallelCommand {
    Quit,
    Resize { width: u64, height: u64 },
    FileDrop { path: String, tabs: Option<bool> },
    FocusLost,
    FocusGained,
    DisplayAvailableFonts(Vec<String>),
    ShowError { lines: Vec<String> },
    ShowErrorStatus { message: String },
    SetBackground { background: String },
    FlushStartupMessages { messages: Vec<StartupMessage> },
    ReplayStartupMessages { messages: Vec<StartupMessage> },
}

async fn display_available_fonts(
    nvim: &Neovim<NeovimWriter>,
    fonts: Vec<String>,
) -> Result<(), Box<CallError>> {
    let mut content: Vec<String> = vec![
        "What follows are the font names available for guifont. You can try any of them with <CR> in normal mode.",
        "",
        "To switch to one of them, use one of them, type:",
        "",
        "    :set guifont=<font name>:h<font size>",
        "",
        "where <font name> is one of the following with spaces escaped",
        "and <font size> is the desired font size. As an example:",
        "",
        "    :set guifont=Cascadia\\ Code\\ PL:h12",
        "",
        "You may specify multiple fonts for fallback purposes separated by commas like so:",
        "",
        "    :set guifont=Cascadia\\ Code\\ PL,Delugia\\ Nerd\\ Font:h12",
        "",
        "Make sure to add the above command when you're happy with it to your .vimrc file or similar config to make it permanent.",
        "------------------------------",
        "Available Fonts on this System",
        "------------------------------",
    ].into_iter().map(|text| text.to_owned()).collect();
    content.extend(fonts);

    nvim.exec2(
        indoc! {"
            split
            noswapfile hide enew
            setlocal buftype=nofile
            setlocal bufhidden=hide
            file scratch
            nnoremap <buffer> <CR> <cmd>lua vim.opt.guifont=vim.fn.getline('.')<CR>,
        "},
        nvim_dict! {},
    )
    .await?;
    let _ = nvim.call("nvim_buf_set_lines", call_args![0i64, 0i64, -1i64, false, content]).await?;
    Ok(())
}

async fn disable_ui_option(nvim: &Neovim<NeovimWriter>, option: &'static str) -> Result<()> {
    nvim.ui_set_option(option, Value::from(false))
        .await
        .with_context(|| format!("Disable {option} failed"))
}

fn cmdheight_to_restore(
    pre_attach_cmdheight: Option<&Value>,
    externalized_cmdheight: &Value,
    last_set_sid: i64,
) -> Option<Value> {
    if externalized_cmdheight.as_u64() == Some(0) && !cmdheight_zero_was_user_set(last_set_sid) {
        pre_attach_cmdheight.cloned()
    } else {
        Some(externalized_cmdheight.clone())
    }
}

fn cmdheight_zero_was_user_set(last_set_sid: i64) -> bool {
    // ext_messages sets cmdheight through the ui api during attach.
    // older nvim versions may leave that source unset, so both sources are treated as a
    // neovide temporary value.
    !matches!(last_set_sid, 0 | SID_API_CLIENT)
}

async fn cmdheight_last_set_sid(nvim: &Neovim<NeovimWriter>) -> Result<i64> {
    nvim.exec_lua(
        "return vim.api.nvim_get_option_info2('cmdheight', {}).last_set_sid",
        call_args![],
    )
    .await?
    .as_i64()
    .context("Read cmdheight option source failed")
}

async fn restore_builtin_message_ui(
    nvim: &Neovim<NeovimWriter>,
    pre_attach_cmdheight: Option<Value>,
) -> Result<()> {
    let cmdheight = nvim.get_option("cmdheight").await.context("Read cmdheight failed")?;
    let last_set_sid = cmdheight_last_set_sid(nvim).await?;
    let restored_cmdheight =
        cmdheight_to_restore(pre_attach_cmdheight.as_ref(), &cmdheight, last_set_sid);
    disable_ui_option(nvim, "ext_messages").await?;
    disable_ui_option(nvim, "ext_cmdline").await?;
    if let Some(cmdheight) = restored_cmdheight {
        nvim.set_option("cmdheight", cmdheight).await.context("Restore cmdheight failed")?;
    }

    Ok(())
}

async fn replay_startup_messages(
    nvim: &Neovim<NeovimWriter>,
    messages: Vec<StartupMessage>,
) -> Result<()> {
    for message in messages {
        show_startup_message(nvim, &message).await.context("Show startup message failed")?;
    }

    Ok(())
}

async fn flush_startup_messages(
    nvim: &Neovim<NeovimWriter>,
    handler: &NeovimHandler,
    messages: Vec<StartupMessage>,
) -> Result<()> {
    restore_builtin_message_ui(nvim, handler.pre_attach_cmdheight()).await?;
    let result = replay_startup_messages(nvim, messages).await;
    handler.send_redraw_event(RedrawEvent::StartupMessageUiRestored);
    result
}

impl ParallelCommand {
    async fn execute(
        self,
        nvim: &Neovim<NeovimWriter>,
        settings: &Settings,
        handler: &NeovimHandler,
    ) {
        // Don't panic here unless there's absolutely no chance of continuing the program, Instead
        // just log the error and hope that it's something temporary or recoverable A normal reason
        // for failure is when neovim has already quit, and a command, for example mouse move is
        // being sent
        let result = match self {
            ParallelCommand::Quit => {
                // Ignore all errors, since neovim exits immediately before the response is sent.
                // We could an RPC notify instead of request, but nvim-rs does currently not support it.
                let _ = nvim
                    .exec_lua(
                        include_str!("../../lua/exit_handler.lua"),
                        call_args![settings.get::<CmdLineSettings>().server.is_some()],
                    )
                    .await;
                Ok(())
            }
            ParallelCommand::Resize { width, height } => nvim
                .ui_try_resize(width.max(10) as i64, height.max(3) as i64)
                .await
                .context("Resize failed"),
            ParallelCommand::FocusLost => {
                nvim.ui_set_focus(false).await.context("FocusLost failed")
            }
            ParallelCommand::FocusGained => {
                nvim.ui_set_focus(true).await.context("FocusGained failed")
            }
            ParallelCommand::FileDrop { path, tabs } => nvim
                .exec_lua(
                    "neovide.private.dropfile(...)",
                    call_args![
                        normalize_file_drop_path(
                            handle_wslpaths(vec![path], settings.get::<CmdLineSettings>().wsl)
                                .first()
                                .unwrap()
                                .to_string(),
                            cfg!(target_os = "windows"),
                        ),
                        tabs.unwrap_or(settings.get::<CmdLineSettings>().tabs)
                    ],
                )
                .await
                .map(|_| ()) // We don't care about the result
                .context("FileDrop failed"),
            ParallelCommand::DisplayAvailableFonts(fonts) => {
                display_available_fonts(nvim, fonts).await.context("DisplayAvailableFonts failed")
            }
            ParallelCommand::ShowError { lines } => {
                // nvim.err_write(&message).await.ok();
                // NOTE: https://github.com/neovim/neovim/issues/5067
                // nvim_err_write[ln] is broken for multiline messages
                // We should go back to it whenever that bug gets fixed.
                show_error_message(nvim, &lines).await.context("ShowError failed")
            }
            ParallelCommand::ShowErrorStatus { message } => {
                show_error_status(nvim, &message).await.context("ShowErrorStatus failed")
            }
            ParallelCommand::SetBackground { background } => {
                set_background_if_allowed(&background, nvim).await;
                Ok(())
            }
            ParallelCommand::FlushStartupMessages { messages } => {
                flush_startup_messages(nvim, handler, messages).await
            }
            ParallelCommand::ReplayStartupMessages { messages } => {
                replay_startup_messages(nvim, messages).await
            }
        };

        if let Err(error) = result {
            log::error!("{error:?}");
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    Serial(SerialCommand),
    Parallel(ParallelCommand),
}

impl From<SerialCommand> for UiCommand {
    fn from(serial: SerialCommand) -> Self { UiCommand::Serial(serial) }
}

impl From<ParallelCommand> for UiCommand {
    fn from(parallel: ParallelCommand) -> Self { UiCommand::Parallel(parallel) }
}

impl AsRef<str> for UiCommand {
    fn as_ref(&self) -> &str {
        match self {
            UiCommand::Serial(cmd) => cmd.as_ref(),
            UiCommand::Parallel(cmd) => cmd.as_ref(),
        }
    }
}

/// Install the current Neovim handle for a route and start command tasks once.
/// Reattaching the route updates the handle used by those tasks without
/// creating a second receiver. Registration happens before `nvim_ui_attach`, so
/// the active registry entry alone does not mean setup has completed.
pub fn start_ui_command_handler(
    route_id: RouteId,
    handler: NeovimHandler,
    nvim: Neovim<NeovimWriter>,
    settings: Arc<Settings>,
    can_support_ime_api: bool,
) {
    handler.update_current_neovim(nvim, can_support_ime_api);
    register_route_handler(route_id, handler.clone());
    #[cfg(target_os = "macos")]
    flush_pending_file_drops_when_ready();
    if handler.mark_ui_command_started() {
        return;
    }

    let (serial_tx, mut serial_rx) = unbounded_channel::<SerialCommand>();
    let (_ui_command_sender, mut ui_command_receiver) = handler.get_ui_command_channel();

    let handler_for_parallel = handler.clone();
    let settings_for_parallel = settings.clone();
    tokio::spawn(async move {
        loop {
            match ui_command_receiver.recv().await {
                Some(UiCommand::Serial(serial_command)) => {
                    tracy_dynamic_zone!(serial_command.as_ref());
                    // This can fail if the serial_rx loop exits before this one, so ignore the errors
                    let _ = serial_tx.send(serial_command);
                }
                Some(UiCommand::Parallel(parallel_command)) => {
                    tracy_dynamic_zone!(parallel_command.as_ref());
                    let handler_for_command = handler_for_parallel.clone();
                    let settings = settings_for_parallel.clone();
                    tokio::spawn(async move {
                        if let Some(ui_command_nvim) = handler_for_command.clone_current_neovim() {
                            parallel_command
                                .execute(&ui_command_nvim, settings.as_ref(), &handler_for_command)
                                .await;
                        } else {
                            log::warn!("Parallel command received without an active Neovim handle");
                        }
                    });
                }
                None => break,
            }
        }
        log::info!("ui command receiver finished");
    });

    let handler_for_serial = handler.clone();
    tokio::spawn(async move {
        let mut pending_command: Option<SerialCommand> = None;

        loop {
            let mut serial_command = match pending_command.take() {
                Some(command) => command,
                None => match serial_rx.recv().await {
                    Some(command) => command,
                    None => break,
                },
            };

            #[cfg(target_os = "macos")]
            if matches!(serial_command, SerialCommand::ScrollTo { .. }) {
                while let Ok(next_command) = serial_rx.try_recv() {
                    if let Some(other) = serial_command.try_merge_scroll_to(next_command) {
                        pending_command = Some(other);
                        break;
                    }
                }
            }

            if matches!(serial_command, SerialCommand::Scroll { .. }) {
                while let Ok(next_command) = serial_rx.try_recv() {
                    if let Some(other) = serial_command.try_merge_scroll(next_command) {
                        pending_command = Some(other);
                        break;
                    }
                }
            }

            {
                tracy_dynamic_zone!(serial_command.as_ref());
            }

            match handler_for_serial.clone_current_neovim_with_ime() {
                Some((serial_nvim, ime_api)) => {
                    serial_command.execute(&serial_nvim, ime_api).await;
                }
                None => {
                    log::warn!("Serial command received without an active Neovim handle");
                    break;
                }
            }
        }
        log::info!("serial command receiver finished");
    });
}

pub fn send_ui<T>(command: T, handler: &NeovimHandler)
where T: Into<UiCommand> {
    let command: UiCommand = command.into();
    let sender = handler.get_ui_command_channel().0;
    sender.send(command).expect("The UI command channel has not been initialized");
}

pub fn register_route_handler(route_id: RouteId, handler: NeovimHandler) {
    HANDLER_REGISTRY.lock().unwrap().register(route_id, handler);
}

pub fn set_active_route_handler(route_id: RouteId) {
    HANDLER_REGISTRY.lock().unwrap().activate(route_id);
}

pub fn unregister_route_handler(route_id: RouteId) {
    HANDLER_REGISTRY.lock().unwrap().unregister(route_id);
}

#[cfg(test)]
mod tests {
    use rmpv::Value;

    use super::HandlerRegistry;
    use super::cmdheight_to_restore;
    use super::normalize_file_drop_path;
    use crate::window::RouteId;

    #[test]
    fn background_route_changes_do_not_steal_the_active_handler() {
        let first = RouteId::next();
        let second = RouteId::next();
        let mut registry = HandlerRegistry::default();

        registry.register(first, "first");
        registry.register(second, "second");
        assert_eq!(registry.active(), Some(&"first"));

        registry.activate(second);
        registry.register(first, "reconnected first");
        assert_eq!(registry.active(), Some(&"second"));

        registry.unregister(first);
        assert_eq!(registry.active(), Some(&"second"));

        registry.register(first, "first again");
        registry.unregister(second);
        assert_eq!(registry.active(), Some(&"first again"));

        registry.unregister(first);
        assert_eq!(registry.active(), None);
    }

    #[test]
    fn normalizes_windows_file_drop_separators() {
        assert_eq!(
            normalize_file_drop_path(
                r"E:\Documents\atoms\duration\(Modified) Duration.md".to_owned(),
                true,
            ),
            "E:/Documents/atoms/duration/(Modified) Duration.md"
        );
    }

    #[test]
    fn preserves_non_windows_file_drop_separators() {
        let path = r"/tmp/a\b/file name.md";
        assert_eq!(normalize_file_drop_path(path.to_owned(), false), path);
    }

    #[test]
    fn cmdheight_restore_uses_pre_attach_value_for_ext_messages_zero() {
        assert_eq!(
            cmdheight_to_restore(Some(&Value::from(1)), &Value::from(0), super::SID_API_CLIENT),
            Some(Value::from(1))
        );
    }

    #[test]
    fn cmdheight_restore_uses_pre_attach_value_for_unset_zero_source() {
        assert_eq!(
            cmdheight_to_restore(Some(&Value::from(1)), &Value::from(0), 0),
            Some(Value::from(1))
        );
    }

    #[test]
    fn cmdheight_restore_preserves_user_zero_value() {
        assert_eq!(
            cmdheight_to_restore(Some(&Value::from(1)), &Value::from(0), 42),
            Some(Value::from(0))
        );
    }

    #[test]
    fn cmdheight_restore_preserves_startup_nonzero_value() {
        assert_eq!(
            cmdheight_to_restore(Some(&Value::from(1)), &Value::from(2), super::SID_API_CLIENT),
            Some(Value::from(2))
        );
    }

    #[test]
    fn cmdheight_restore_preserves_pre_attach_zero() {
        assert_eq!(
            cmdheight_to_restore(Some(&Value::from(0)), &Value::from(0), super::SID_API_CLIENT),
            Some(Value::from(0))
        );
    }

    #[test]
    fn cmdheight_restore_skips_zero_without_pre_attach_value() {
        assert_eq!(cmdheight_to_restore(None, &Value::from(0), super::SID_API_CLIENT), None);
    }
}
