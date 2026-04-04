use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, Weak};

use copypasta::{ClipboardContext, ClipboardProvider};
#[cfg(target_os = "linux")]
use copypasta::{
    wayland_clipboard,
    x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext},
};
use log::warn;
use raw_window_handle::HasDisplayHandle;
#[cfg(target_os = "linux")]
use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
use winit::event_loop::EventLoop;

use crate::window::EventPayload;

type ProviderInitResult<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;
pub type ClipboardResult<T> = std::result::Result<T, ClipboardError>;

const CLIPBOARD_PROVIDER: &str = "clipboard";
#[cfg(target_os = "linux")]
const PRIMARY_SELECTION_PROVIDER: &str = "primary selection";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    DisplayHandleUnavailable { source: String },
    ProviderInitializationFailed { provider: &'static str, backend: &'static str, source: String },
    ProviderUnavailable { provider: &'static str, register: String, reason: String },
    GetContentsFailed { provider: &'static str, register: String, source: String },
    SetContentsFailed { provider: &'static str, register: String, source: String },
}

impl ClipboardError {
    fn display_handle_unavailable(error: impl ToString) -> Self {
        Self::DisplayHandleUnavailable { source: error.to_string() }
    }

    fn provider_initialization_failed(
        provider: &'static str,
        backend: &'static str,
        error: impl ToString,
    ) -> Self {
        Self::ProviderInitializationFailed { provider, backend, source: error.to_string() }
    }

    fn provider_unavailable(provider: &'static str, register: &str, reason: impl ToString) -> Self {
        Self::ProviderUnavailable {
            provider,
            register: register.to_string(),
            reason: reason.to_string(),
        }
    }

    fn get_contents_failed(provider: &'static str, register: &str, error: impl ToString) -> Self {
        Self::GetContentsFailed {
            provider,
            register: register.to_string(),
            source: error.to_string(),
        }
    }

    fn set_contents_failed(provider: &'static str, register: &str, error: impl ToString) -> Self {
        Self::SetContentsFailed {
            provider,
            register: register.to_string(),
            source: error.to_string(),
        }
    }

    fn unavailability_reason(&self) -> String {
        match self {
            Self::DisplayHandleUnavailable { source } => {
                format!("display handle unavailable: {source}")
            }
            Self::ProviderInitializationFailed { backend, source, .. } => {
                format!("initialization via {backend} failed: {source}")
            }
            Self::ProviderUnavailable { reason, .. } => reason.clone(),
            Self::GetContentsFailed { source, .. } => source.clone(),
            Self::SetContentsFailed { source, .. } => source.clone(),
        }
    }
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisplayHandleUnavailable { source } => {
                write!(f, "clipboard display handle unavailable: {source}")
            }
            Self::ProviderInitializationFailed { provider, backend, source } => {
                write!(f, "{provider} provider initialization via {backend} failed: {source}")
            }
            Self::ProviderUnavailable { provider, register, reason } => {
                write!(f, "{provider} for register {register} is unavailable: {reason}")
            }
            Self::GetContentsFailed { provider, register, source } => {
                write!(f, "failed to get contents from {provider} register {register}: {source}")
            }
            Self::SetContentsFailed { provider, register, source } => {
                write!(f, "failed to set contents for {provider} register {register}: {source}")
            }
        }
    }
}

pub enum ProviderState {
    Available(Box<dyn ClipboardProvider>),
    Unavailable(ClipboardError),
}

impl ProviderState {
    fn from_init_result(
        provider: &'static str,
        backend: &'static str,
        result: ProviderInitResult<Box<dyn ClipboardProvider>>,
    ) -> Self {
        match result {
            Ok(provider) => Self::Available(provider),
            Err(error) => {
                let error =
                    ClipboardError::provider_initialization_failed(provider, backend, error);
                warn!("{error}");
                Self::Unavailable(error)
            }
        }
    }

    fn get_mut(
        &mut self,
        provider: &'static str,
        register: &str,
    ) -> ClipboardResult<&mut dyn ClipboardProvider> {
        match self {
            Self::Available(provider_state) => Ok(provider_state.as_mut()),
            Self::Unavailable(error) => Err(ClipboardError::provider_unavailable(
                provider,
                register,
                error.unavailability_reason(),
            )),
        }
    }

    fn is_available(&self) -> bool {
        matches!(self, Self::Available(_))
    }

    #[cfg(test)]
    pub fn available_for_test(provider: impl ClipboardProvider + 'static) -> Self {
        Self::Available(Box::new(provider))
    }

    #[cfg(test)]
    pub fn unavailable_for_test(error: ClipboardError) -> Self {
        Self::Unavailable(error)
    }
}

pub struct Clipboard {
    clipboard: ProviderState,
    #[cfg(target_os = "linux")]
    selection: ProviderState,
}

#[derive(Clone)]
pub struct ClipboardHandle {
    inner: Weak<Mutex<Clipboard>>,
}

impl ClipboardHandle {
    pub fn new(clipboard: &Arc<Mutex<Clipboard>>) -> Self {
        Self { inner: Arc::downgrade(clipboard) }
    }

    pub fn upgrade(&self) -> Option<Arc<Mutex<Clipboard>>> {
        self.inner.upgrade()
    }
}

impl Clipboard {
    pub fn new(event_loop: &EventLoop<EventPayload>) -> Arc<Mutex<Self>> {
        let clipboard = match event_loop.display_handle() {
            #[cfg(target_os = "linux")]
            Ok(display_handle) => match display_handle.as_raw() {
                RawDisplayHandle::Wayland(WaylandDisplayHandle { mut display, .. }) => unsafe {
                    let (selection, clipboard) =
                        wayland_clipboard::create_clipboards_from_external(display.as_mut());
                    Self::with_providers(
                        ProviderState::Available(Box::new(clipboard)),
                        ProviderState::Available(Box::new(selection)),
                    )
                },
                _ => Self::new_x11(),
            },
            #[cfg(not(target_os = "linux"))]
            Ok(_) => Self::new_system(),
            Err(error) => Self::disabled(ClipboardError::display_handle_unavailable(error)),
        };

        Arc::new(Mutex::new(clipboard))
    }

    #[cfg(target_os = "linux")]
    fn new_x11() -> Self {
        let clipboard = ProviderState::from_init_result(
            CLIPBOARD_PROVIDER,
            "x11",
            ClipboardContext::new()
                .map(|clipboard| Box::new(clipboard) as Box<dyn ClipboardProvider>),
        );

        let selection = ProviderState::from_init_result(
            PRIMARY_SELECTION_PROVIDER,
            "x11",
            X11ClipboardContext::<X11SelectionClipboard>::new()
                .map(|selection| Box::new(selection) as Box<dyn ClipboardProvider>),
        );

        Self::with_providers(clipboard, selection)
    }

    #[cfg(not(target_os = "linux"))]
    fn new_system() -> Self {
        Self::with_providers(ProviderState::from_init_result(
            CLIPBOARD_PROVIDER,
            "system",
            ClipboardContext::new()
                .map(|clipboard| Box::new(clipboard) as Box<dyn ClipboardProvider>),
        ))
    }

    #[cfg(target_os = "linux")]
    fn with_providers(clipboard: ProviderState, selection: ProviderState) -> Self {
        let clipboard = Self { clipboard, selection };
        clipboard.warn_if_unavailable();
        clipboard
    }

    #[cfg(not(target_os = "linux"))]
    fn with_providers(clipboard: ProviderState) -> Self {
        let clipboard = Self { clipboard };
        clipboard.warn_if_unavailable();
        clipboard
    }

    fn disabled(error: ClipboardError) -> Self {
        warn!("{error}");
        Self::empty(error)
    }

    fn empty(error: ClipboardError) -> Self {
        Self {
            clipboard: ProviderState::Unavailable(error.clone()),
            #[cfg(target_os = "linux")]
            selection: ProviderState::Unavailable(error),
        }
    }

    fn warn_if_unavailable(&self) {
        if !self.has_any_provider() {
            warn!("clipboard support disabled: no clipboard providers were initialized");
        }
    }

    #[cfg(target_os = "linux")]
    fn has_any_provider(&self) -> bool {
        self.clipboard.is_available() || self.selection.is_available()
    }

    #[cfg(not(target_os = "linux"))]
    fn has_any_provider(&self) -> bool {
        self.clipboard.is_available()
    }

    #[cfg(target_os = "linux")]
    fn provider_state(&mut self, register: &str) -> (&'static str, &mut ProviderState) {
        match register {
            "*" => (PRIMARY_SELECTION_PROVIDER, &mut self.selection),
            _ => (CLIPBOARD_PROVIDER, &mut self.clipboard),
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn provider_state(&mut self, _register: &str) -> (&'static str, &mut ProviderState) {
        (CLIPBOARD_PROVIDER, &mut self.clipboard)
    }

    pub fn get_contents(&mut self, register: &str) -> ClipboardResult<String> {
        let (provider, state) = self.provider_state(register);
        state
            .get_mut(provider, register)?
            .get_contents()
            .map_err(|error| ClipboardError::get_contents_failed(provider, register, error))
    }

    pub fn set_contents(&mut self, lines: String, register: &str) -> ClipboardResult<()> {
        let (provider, state) = self.provider_state(register);
        state
            .get_mut(provider, register)?
            .set_contents(lines)
            .map_err(|error| ClipboardError::set_contents_failed(provider, register, error))
    }

    #[cfg(test)]
    pub fn from_provider_states_for_test(
        clipboard: ProviderState,
        #[cfg(target_os = "linux")] selection: ProviderState,
    ) -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::with_providers(clipboard, selection)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Self::with_providers(clipboard)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    const TEST_CLIPBOARD_BACKEND: &str = "x11";
    #[cfg(not(target_os = "linux"))]
    const TEST_CLIPBOARD_BACKEND: &str = "system";

    struct TestClipboardProvider {
        contents: String,
        get_error: Option<&'static str>,
        set_error: Option<&'static str>,
    }

    impl TestClipboardProvider {
        #[cfg(target_os = "linux")]
        fn with_contents(contents: &str) -> Self {
            Self { contents: contents.to_string(), get_error: None, set_error: None }
        }

        fn with_get_error(error: &'static str) -> Self {
            Self { contents: String::new(), get_error: Some(error), set_error: None }
        }
    }

    impl ClipboardProvider for TestClipboardProvider {
        fn get_contents(&mut self) -> ProviderInitResult<String> {
            match self.get_error {
                Some(error) => Err(Box::new(std::io::Error::other(error))),
                None => Ok(self.contents.clone()),
            }
        }

        fn set_contents(&mut self, contents: String) -> ProviderInitResult<()> {
            match self.set_error {
                Some(error) => Err(Box::new(std::io::Error::other(error))),
                None => {
                    self.contents = contents;
                    Ok(())
                }
            }
        }
    }

    fn init_error(
        provider: &'static str,
        backend: &'static str,
        error: &'static str,
    ) -> ClipboardError {
        ClipboardError::provider_initialization_failed(provider, backend, error)
    }

    #[test]
    fn total_failure_preserves_initialization_reason() {
        let mut clipboard = Clipboard::from_provider_states_for_test(
            ProviderState::unavailable_for_test(init_error(
                CLIPBOARD_PROVIDER,
                TEST_CLIPBOARD_BACKEND,
                "setup failed",
            )),
            #[cfg(target_os = "linux")]
            ProviderState::unavailable_for_test(init_error(
                PRIMARY_SELECTION_PROVIDER,
                "x11",
                "selection setup failed",
            )),
        );

        let error = clipboard.get_contents("+").unwrap_err().to_string();
        assert!(error.contains("clipboard for register + is unavailable"));
        assert!(error.contains(&format!(
            "initialization via {TEST_CLIPBOARD_BACKEND} failed: setup failed"
        )));
    }

    #[test]
    fn provider_operation_failures_include_context() {
        let mut clipboard = Clipboard::from_provider_states_for_test(
            ProviderState::available_for_test(TestClipboardProvider::with_get_error("read failed")),
            #[cfg(target_os = "linux")]
            ProviderState::available_for_test(TestClipboardProvider::with_contents("selection")),
        );

        let error = clipboard.get_contents("+").unwrap_err().to_string();
        assert!(error.contains("failed to get contents from clipboard register +"));
        assert!(error.contains("read failed"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn partial_failure_keeps_available_provider_working() {
        let mut clipboard = Clipboard::from_provider_states_for_test(
            ProviderState::available_for_test(TestClipboardProvider::with_contents("copied")),
            ProviderState::unavailable_for_test(init_error(
                PRIMARY_SELECTION_PROVIDER,
                "x11",
                "selection setup failed",
            )),
        );

        assert_eq!(clipboard.get_contents("+").unwrap(), "copied");

        let error = clipboard.get_contents("*").unwrap_err().to_string();
        assert!(error.contains("primary selection for register * is unavailable"));
        assert!(error.contains("selection setup failed"));
    }
}
