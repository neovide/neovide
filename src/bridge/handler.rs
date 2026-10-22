use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use log::trace;
use log::warn;
use nvim_rs::Handler;
use nvim_rs::Neovim;
use nvim_rs::call_args;
use rmpv::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use winit::event_loop::EventLoopProxy;

use super::ui_commands::UiCommand;
use crate::LoggingReceiver;
use crate::LoggingSender;
use crate::bridge::GuiOption;
use crate::bridge::NeovimWriter;
use crate::bridge::ParallelCommand;
use crate::bridge::RedrawEvent;
use crate::bridge::clipboard::get_clipboard_contents;
use crate::bridge::clipboard::set_clipboard_contents;
use crate::bridge::events::parse_redraw_event;
use crate::bridge::parse_progress_bar_event;
use crate::bridge::send_ui;
use crate::clipboard::ClipboardHandle;
use crate::error_handling::ResultPanicExplanation;
use crate::running_tracker::RunningTracker;
use crate::settings::FontConfigState;
use crate::settings::Settings;
use crate::window::EventPayload;
#[cfg(target_os = "macos")]
use crate::window::ForceClickKind;
use crate::window::RouteId;
use crate::window::UserEvent;
use crate::window::WindowCommand;

#[derive(Debug, PartialEq, Eq)]
enum ClipboardRequestError {
    Unavailable,
    LockUnavailable(String),
    CannotGetContents,
    CannotSetContents,
}

impl fmt::Display for ClipboardRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(f, "clipboard unavailable"),
            Self::LockUnavailable(source) => write!(f, "clipboard unavailable: {source}"),
            Self::CannotGetContents => write!(f, "cannot get clipboard contents"),
            Self::CannotSetContents => write!(f, "cannot set clipboard contents"),
        }
    }
}

impl From<ClipboardRequestError> for Value {
    fn from(error: ClipboardRequestError) -> Self { Value::from(error.to_string()) }
}

fn handle_clipboard_request<T, F>(
    clipboard_handle: &ClipboardHandle,
    request: F,
) -> Result<T, ClipboardRequestError>
where
    F: FnOnce(&mut crate::clipboard::Clipboard) -> Result<T, ClipboardRequestError>,
{
    let clipboard = clipboard_handle.upgrade().ok_or(ClipboardRequestError::Unavailable)?;
    let mut clipboard = clipboard
        .lock()
        .map_err(|error| ClipboardRequestError::LockUnavailable(error.to_string()))?;
    request(&mut clipboard)
}

fn handle_quit_request(
    running_tracker: &RunningTracker,
    arguments: &[Value],
) -> Result<Value, Value> {
    match arguments {
        [Value::Nil] => Ok(Value::Nil),
        [value] => {
            let exit_code = value
                .as_i64()
                .ok_or_else(|| Value::from("neovide.quit expects an integer exit code or nil"))?;

            running_tracker.quit_with_code(exit_code as u8, "Quit from neovim");

            Ok(Value::Nil)
        }
        _ => Err(Value::from("neovide.quit expects an argument")),
    }
}

#[derive(Default)]
struct NeovimState {
    nvim: Option<Neovim<NeovimWriter>>,
    can_support_ime_api: bool,
    pre_attach_cmdheight: Option<Value>,
}

#[derive(Clone)]
pub struct NeovimHandler {
    // The EventLoopProxy is not sync on all platforms, so wrap it in a mutex
    proxy: Arc<Mutex<EventLoopProxy<EventPayload>>>,
    redraw_event_sender: LoggingSender<RedrawEvent>,
    ui_command_sender: LoggingSender<UiCommand>,
    ui_command_receiver: LoggingReceiver<UiCommand>,
    current_neovim: Arc<RwLock<NeovimState>>,
    ui_command_started: Arc<AtomicBool>,
    running_tracker: RunningTracker,
    route_id: RouteId,
    #[allow(dead_code)]
    settings: Arc<Settings>,
    clipboard: ClipboardHandle,
}

impl std::fmt::Debug for NeovimHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeovimHandler").finish()
    }
}

impl NeovimHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        redraw_event_sender: UnboundedSender<RedrawEvent>,
        ui_command_sender: UnboundedSender<UiCommand>,
        ui_command_receiver: UnboundedReceiver<UiCommand>,
        proxy: EventLoopProxy<EventPayload>,
        running_tracker: RunningTracker,
        route_id: RouteId,
        settings: Arc<Settings>,
        clipboard: ClipboardHandle,
    ) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
            redraw_event_sender: LoggingSender::attach(redraw_event_sender, "neovim_handler"),
            ui_command_sender: LoggingSender::attach(ui_command_sender, "UICommand"),
            ui_command_receiver: LoggingReceiver::attach(ui_command_receiver, "UICommand"),
            current_neovim: Arc::new(RwLock::new(NeovimState::default())),
            ui_command_started: Arc::new(AtomicBool::new(false)),
            running_tracker,
            route_id,
            settings,
            clipboard,
        }
    }

    fn send_window_command(&self, command: WindowCommand) {
        let payload = EventPayload::for_route(UserEvent::WindowCommand(command), self.route_id);
        let _ = self.proxy.lock().unwrap().send_event(payload);
    }

    pub fn get_ui_command_channel(&self) -> (LoggingSender<UiCommand>, LoggingReceiver<UiCommand>) {
        (self.ui_command_sender.clone(), self.ui_command_receiver.clone())
    }

    pub fn update_current_neovim(&self, neovim: Neovim<NeovimWriter>, can_support_ime_api: bool) {
        if let Ok(mut guard) = self.current_neovim.write() {
            guard.nvim = Some(neovim);
            guard.can_support_ime_api = can_support_ime_api;
        }
    }

    pub fn clone_current_neovim(&self) -> Option<Neovim<NeovimWriter>> {
        self.current_neovim.read().ok().and_then(|guard| guard.nvim.as_ref().cloned())
    }

    pub fn clone_current_neovim_with_ime(&self) -> Option<(Neovim<NeovimWriter>, bool)> {
        self.current_neovim.read().ok().and_then(|guard| {
            guard.nvim.as_ref().cloned().map(|nvim| (nvim, guard.can_support_ime_api))
        })
    }

    pub fn mark_ui_command_started(&self) -> bool {
        self.ui_command_started.swap(true, Ordering::SeqCst)
    }

    pub fn set_pre_attach_cmdheight(&self, cmdheight: Value) {
        if let Ok(mut guard) = self.current_neovim.write() {
            guard.pre_attach_cmdheight = Some(cmdheight);
        }
    }

    pub fn pre_attach_cmdheight(&self) -> Option<Value> {
        self.current_neovim.read().ok().and_then(|guard| guard.pre_attach_cmdheight.clone())
    }

    pub fn send_redraw_event(&self, event: RedrawEvent) {
        let _ = self.redraw_event_sender.send(event);
    }
}

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = NeovimWriter;

    async fn handle_request(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) -> Result<Value, Value> {
        trace!("Neovim request: {:?}", event_name);

        match event_name.as_ref() {
            "neovide.get_clipboard" => handle_clipboard_request(&self.clipboard, |clipboard| {
                get_clipboard_contents(clipboard, &arguments[0])
                    .map_err(|_| ClipboardRequestError::CannotGetContents)
            })
            .map_err(Value::from),
            "neovide.set_clipboard" => handle_clipboard_request(&self.clipboard, |clipboard| {
                set_clipboard_contents(clipboard, &arguments[0], &arguments[1])
                    .map_err(|_| ClipboardRequestError::CannotSetContents)
            })
            .map_err(Value::from),
            "neovide.quit" => handle_quit_request(&self.running_tracker, &arguments),
            _ => Ok(Value::from("rpcrequest not handled")),
        }
    }

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        neovim: Neovim<Self::Writer>,
    ) {
        trace!("Neovim notification: {:?}", event_name);

        match event_name.as_ref() {
            "redraw" => {
                for events in arguments {
                    let parsed_events = parse_redraw_event(events)
                        .unwrap_or_explained_panic("Could not parse event from neovim");

                    for parsed_event in parsed_events {
                        if skip_default_guifont(&parsed_event, &self.settings, &neovim).await {
                            continue;
                        }

                        match parsed_event {
                            RedrawEvent::Restart { details } => {
                                let payload = EventPayload::for_route(
                                    UserEvent::NeovimRestart(details),
                                    self.route_id,
                                );
                                let _ = self.proxy.lock().unwrap().send_event(payload);
                            }
                            _ => {
                                let _ = self.redraw_event_sender.send(parsed_event);
                            }
                        }
                    }
                }
            }
            "setting_changed" => {
                self.settings.handle_setting_changed_notification(
                    arguments,
                    &self.proxy.lock().unwrap(),
                    self.route_id,
                );
            }
            "option_changed" => {
                self.settings.handle_option_changed_notification(
                    arguments,
                    &self.proxy.lock().unwrap(),
                    self.route_id,
                );
            }
            #[cfg(windows)]
            "neovide.register_right_click" => {
                self.send_window_command(WindowCommand::RegisterRightClick);
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                self.send_window_command(WindowCommand::UnregisterRightClick);
            }
            "neovide.focus_window" => {
                self.send_window_command(WindowCommand::FocusWindow);
            }
            #[cfg(target_os = "macos")]
            "neovide.document_state" => match parse_document_state_args(&arguments) {
                Some((path, modified)) => {
                    self.send_window_command(WindowCommand::DocumentStateChanged {
                        path,
                        modified,
                    });
                }
                None => {
                    warn!("neovide.document_state called with invalid arguments: {arguments:?}")
                }
            },
            #[cfg(target_os = "macos")]
            "neovide.scrollbar_window" => match parse_scrollbar_window_args(&arguments) {
                Some((window, metadata)) => {
                    self.send_redraw_event(RedrawEvent::ScrollbarWindow { window, metadata });
                }
                None => {
                    warn!("neovide.scrollbar_window called with invalid arguments: {arguments:?}")
                }
            },
            #[cfg(target_os = "macos")]
            "neovide.force_click" => match parse_force_click_args(&arguments) {
                Some((col, row, entity, guifont, kind)) => {
                    self.send_window_command(WindowCommand::TouchpadPressure {
                        col,
                        row,
                        entity,
                        guifont,
                        kind,
                    });
                }
                None => warn!("neovide.force_click called with invalid arguments: {arguments:?}"),
            },
            "neovide.exec_detach_handler" => {
                send_ui(ParallelCommand::Quit, self);
            }
            "neovide.set_redraw" => {
                if let Some(value) = arguments.first() {
                    let value = value.as_bool().unwrap_or(true);
                    let _ = self.redraw_event_sender.send(RedrawEvent::NeovideSetRedraw(value));
                }
            }
            "neovide.intro_banner_allowed" => {
                if let Some(value) = arguments.first()
                    && let Some(allowed) = value.as_bool()
                {
                    let _ = self
                        .redraw_event_sender
                        .send(RedrawEvent::NeovideIntroBannerAllowed(allowed));
                }
            }
            "neovide.progress_bar" => {
                parse_progress_bar_event(arguments.first())
                    .map(|event| {
                        let _ = self
                            .proxy
                            .lock()
                            .unwrap()
                            .send_event(EventPayload::for_route(event, self.route_id));
                    })
                    .unwrap_or_else(|| {
                        log::info!(
                            "Failed to parse neovide.progress_bar event data: {:?}",
                            arguments
                        );
                    });
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn parse_force_click_args(
    arguments: &[Value],
) -> Option<(i64, i64, String, String, ForceClickKind)> {
    let (col, row, entity, guifont, kind_value) = match arguments {
        [col, row, entity, guifont, kind, ..] => (col, row, entity, guifont, Some(kind)),
        [col, row, entity, guifont] => (col, row, entity, guifont, None),
        _ => return None,
    };

    let col = col.as_i64()?;
    let row = row.as_i64()?;
    let entity = entity.as_str().unwrap_or("").to_string();
    let guifont = guifont.as_str().unwrap_or("").to_string();
    let kind_str = kind_value.and_then(Value::as_str).unwrap_or("text");
    let kind = ForceClickKind::from(kind_str);

    Some((col, row, entity, guifont, kind))
}

#[cfg(target_os = "macos")]
fn parse_document_state_args(arguments: &[Value]) -> Option<(String, bool)> {
    let [path, modified, ..] = arguments else {
        return None;
    };

    Some((path.as_str().unwrap_or("").to_string(), modified.as_bool().unwrap_or(false)))
}

#[cfg(target_os = "macos")]
fn parse_scrollbar_window_args(
    arguments: &[Value],
) -> Option<(u64, Option<crate::bridge::ScrollbarWindowMetadata>)> {
    use crate::bridge::ScrollbarWindowMetadata;

    let window = arguments.first()?.as_u64().filter(|id| *id > 0)?;
    let metadata = match arguments {
        [_] => None,
        [_, buffer, eligible] => Some(ScrollbarWindowMetadata {
            buffer: buffer.as_u64().filter(|id| *id > 0)?,
            eligible: eligible.as_bool()?,
        }),
        _ => return None,
    };
    Some((window, metadata))
}

async fn skip_default_guifont(
    event: &RedrawEvent,
    settings: &Settings,
    nvim: &Neovim<NeovimWriter>,
) -> bool {
    if !settings.get::<FontConfigState>().has_font || !is_guifont_option_set(event) {
        return false;
    }

    guifont_was_set(nvim).await.map(|was_set| !was_set).unwrap_or_else(|error| {
        warn!("Failed to determine if guifont was set: {error}");
        false
    })
}

// https://neovim.io/doc/user/api.html#nvim_get_option_info2()
async fn guifont_was_set(nvim: &Neovim<NeovimWriter>) -> Result<bool, String> {
    let value = nvim
        .exec_lua("return vim.api.nvim_get_option_info2('guifont', {}).was_set", call_args![])
        .await
        .map_err(|error| error.to_string())?;

    Ok(value.as_bool().unwrap_or(false))
}

fn is_guifont_option_set(event: &RedrawEvent) -> bool {
    matches!(event, RedrawEvent::OptionSet { gui_option: GuiOption::GuiFont(_) })
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;
    use std::panic::catch_unwind;
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::ClipboardRequestError;
    use super::handle_clipboard_request;
    use crate::clipboard::Clipboard;
    use crate::clipboard::ClipboardError;
    use crate::clipboard::ClipboardHandle;
    use crate::clipboard::ProviderState;

    fn unavailable_clipboard(error: ClipboardError) -> Arc<Mutex<Clipboard>> {
        Arc::new(Mutex::new(Clipboard::from_provider_states_for_test(
            ProviderState::unavailable_for_test(error.clone()),
            #[cfg(target_os = "linux")]
            ProviderState::unavailable_for_test(error),
        )))
    }

    #[test]
    fn clipboard_request_returns_generic_clipboard_error_message() {
        let clipboard = unavailable_clipboard(ClipboardError::ProviderInitializationFailed {
            provider: "clipboard",
            backend: "x11",
            source: "setup failed".to_string(),
        });

        let handle = ClipboardHandle::new(&clipboard);
        let error = handle_clipboard_request(&handle, |clipboard| {
            clipboard.get_contents("+").map_err(|_| ClipboardRequestError::CannotGetContents)
        })
        .unwrap_err();

        assert_eq!(error, ClipboardRequestError::CannotGetContents);
    }

    #[test]
    fn clipboard_request_returns_lock_error_instead_of_panicking() {
        let clipboard = unavailable_clipboard(ClipboardError::ProviderInitializationFailed {
            provider: "clipboard",
            backend: "x11",
            source: "setup failed".to_string(),
        });

        let handle = ClipboardHandle::new(&clipboard);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = clipboard.lock().unwrap();
            panic!("poison clipboard mutex");
        }));

        let error = handle_clipboard_request(&handle, |clipboard| {
            clipboard.get_contents("+").map_err(|_| ClipboardRequestError::CannotGetContents)
        })
        .unwrap_err();

        assert!(matches!(error, ClipboardRequestError::LockUnavailable(_)));
    }
}
