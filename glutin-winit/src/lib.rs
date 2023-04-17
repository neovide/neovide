//! This library provides helpers for cross-platform [`glutin`] bootstrapping
//! with [`winit`].

#![deny(rust_2018_idioms)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(clippy::all)]
#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![cfg_attr(feature = "cargo-clippy", deny(warnings))]

mod window;

pub use window::GlWindow;

use std::error::Error;

use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::display::{Display, DisplayApiPreference};
#[cfg(x11_platform)]
use glutin::platform::x11::X11GlConfigExt;
use glutin::prelude::*;

use raw_window_handle::{HasRawDisplayHandle, RawWindowHandle};

#[cfg(wgl_backend)]
use raw_window_handle::HasRawWindowHandle;

use winit::error::OsError;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};

#[cfg(glx_backend)]
use winit::platform::x11::register_xlib_error_hook;
#[cfg(x11_platform)]
use winit::platform::x11::WindowBuilderExtX11;

#[cfg(all(not(egl_backend), not(glx_backend), not(wgl_backend), not(cgl_backend)))]
compile_error!("Please select at least one api backend");

/// The helper to perform [`Display`] creation and OpenGL platform
/// bootstrapping with the help of [`winit`] with little to no platform specific
/// code.
///
/// This is only required for the initial setup. If you want to create
/// additional windows just use the [`finalize_window`] function and the
/// configuration you've used either for the original window or picked with the
/// existing [`Display`].
///
/// [`winit`]: winit
/// [`Display`]: glutin::display::Display
#[derive(Default, Debug, Clone)]
pub struct DisplayBuilder {
    preference: ApiPreference,
    window_builder: Option<WindowBuilder>,
}

impl DisplayBuilder {
    /// Create new display builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// The preference in picking the configuration.
    pub fn with_preference(mut self, preference: ApiPreference) -> Self {
        self.preference = preference;
        self
    }

    /// The window builder to use when building a window.
    ///
    /// By default no window is created.
    pub fn with_window_builder(mut self, window_builder: Option<WindowBuilder>) -> Self {
        self.window_builder = window_builder;
        self
    }

    /// Initialize the OpenGL platform and create a compatible window to use
    /// with it when the [`WindowBuilder`] was passed with
    /// [`Self::with_window_builder`]. It's optional, since on some
    /// platforms like `Android` it is not available early on, so you want to
    /// find configuration and later use it with the [`finalize_window`].
    /// But if you don't care about such platform you can always pass
    /// [`WindowBuilder`].
    ///
    /// # Api-specific
    ///
    /// **WGL:** - [`WindowBuilder`] **must** be passed in
    /// [`Self::with_window_builder`] if modern OpenGL(ES) is desired,
    /// otherwise only builtin functions like `glClear` will be available.
    pub fn build<T, Picker>(
        mut self,
        window_target: &EventLoopWindowTarget<T>,
        template_builder: ConfigTemplateBuilder,
        config_picker: Picker,
    ) -> Result<(Option<Window>, Config), Box<dyn Error>>
    where
        Picker: FnOnce(Box<dyn Iterator<Item = Config> + '_>) -> Config,
    {
        // XXX with WGL backend window should be created first.
        #[cfg(wgl_backend)]
        let window = if let Some(wb) = self.window_builder.take() {
            Some(wb.build(window_target)?)
        } else {
            None
        };

        #[cfg(wgl_backend)]
        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());
        #[cfg(not(wgl_backend))]
        let raw_window_handle = None;

        let gl_display = create_display(window_target, self.preference, raw_window_handle)?;

        // XXX the native window must be passed to config picker when WGL is used
        // otherwise very limited OpenGL features will be supported.
        #[cfg(wgl_backend)]
        let template_builder = if let Some(raw_window_handle) = raw_window_handle {
            template_builder.compatible_with_native_window(raw_window_handle)
        } else {
            template_builder
        };

        let template = template_builder.build();

        let gl_config = unsafe {
            let configs = gl_display.find_configs(template)?;
            config_picker(configs)
        };

        #[cfg(not(wgl_backend))]
        let window = if let Some(wb) = self.window_builder.take() {
            Some(finalize_window(window_target, wb, &gl_config)?)
        } else {
            None
        };

        Ok((window, gl_config))
    }
}

fn create_display<T>(
    window_target: &EventLoopWindowTarget<T>,
    _api_preference: ApiPreference,
    _raw_window_handle: Option<RawWindowHandle>,
) -> Result<Display, Box<dyn Error>> {
    #[cfg(egl_backend)]
    let _preference = DisplayApiPreference::Egl;

    #[cfg(glx_backend)]
    let _preference = DisplayApiPreference::Glx(Box::new(register_xlib_error_hook));

    #[cfg(cgl_backend)]
    let _preference = DisplayApiPreference::Cgl;

    #[cfg(wgl_backend)]
    let _preference = DisplayApiPreference::Wgl(_raw_window_handle);

    #[cfg(all(egl_backend, glx_backend))]
    let _preference = match _api_preference {
        ApiPreference::PreferEgl => {
            DisplayApiPreference::EglThenGlx(Box::new(register_xlib_error_hook))
        }
        ApiPreference::FallbackEgl => {
            DisplayApiPreference::GlxThenEgl(Box::new(register_xlib_error_hook))
        }
    };

    #[cfg(all(wgl_backend, egl_backend))]
    let _preference = match _api_preference {
        ApiPreference::PreferEgl => DisplayApiPreference::EglThenWgl(_raw_window_handle),
        ApiPreference::FallbackEgl => DisplayApiPreference::WglThenEgl(_raw_window_handle),
    };

    unsafe {
        Ok(Display::new(
            window_target.raw_display_handle(),
            _preference,
        )?)
    }
}

/// Finalize [`Window`] creation by applying the options from the [`Config`], be
/// aware that it could remove incompatible options from the window builder like
/// `transparency`, when the provided config doesn't support it.
///
/// [`Window`]: winit::window::Window
/// [`Config`]: glutin::config::Config
pub fn finalize_window<T>(
    window_target: &EventLoopWindowTarget<T>,
    mut builder: WindowBuilder,
    gl_config: &Config,
) -> Result<Window, OsError> {
    // Disable transparency if the end config doesn't support it.
    if gl_config.supports_transparency() == Some(false) {
        builder = builder.with_transparent(false);
    }

    #[cfg(x11_platform)]
    let builder = if let Some(x11_visual) = gl_config.x11_visual() {
        builder.with_x11_visual(x11_visual.into_raw())
    } else {
        builder
    };

    builder.build(window_target)
}

/// Simplified version of the [`DisplayApiPreference`] which is used to simplify
/// cross platform window creation.
///
/// To learn about platform differences the [`DisplayApiPreference`] variants.
///
/// [`DisplayApiPreference`]: glutin::display::DisplayApiPreference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiPreference {
    /// Prefer `EGL` over system provider like `GLX` and `WGL`.
    PreferEgl,

    /// Fallback to `EGL` when failed to create the system profile.
    ///
    /// This behavior is used by default. However consider using
    /// [`Self::PreferEgl`] if you don't care about missing EGL features.
    #[default]
    FallbackEgl,
}
