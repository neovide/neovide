mod error_window;
mod keyboard_manager;
mod mouse_manager;
mod settings;
mod update_loop;
mod window_wrapper;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
use std::env;

use winit::{
    dpi::{PhysicalSize, Size},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Icon, Theme, Window},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;

#[cfg(target_os = "linux")]
use winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;

#[cfg(target_os = "macos")]
use winit::platform::macos::EventLoopBuilderExtMacOS;

use image::{load_from_memory, GenericImageView, Pixel};
use keyboard_manager::KeyboardManager;
use mouse_manager::MouseManager;

use crate::{
    cmd_line::{CmdLineSettings, GeometryArgs},
    frame::Frame,
    renderer::{build_window_config, DrawCommand, WindowConfig},
    settings::{
        clamped_grid_size, load_last_window_settings, save_window_size, HotReloadConfigs,
        PersistentWindowSettings, SettingsChanged, SETTINGS,
    },
    units::GridSize,
};
pub use error_window::show_error_window;
pub use settings::{WindowSettings, WindowSettingsChanged};
pub use update_loop::ShouldRender;
pub use update_loop::UpdateLoop;
pub use window_wrapper::WinitWindowWrapper;

static ICON: &[u8] = include_bytes!("../../assets/neovide.ico");

const DEFAULT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 500,
    height: 500,
};
const MIN_PERSISTENT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 300,
    height: 150,
};
const MAX_PERSISTENT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 8192,
    height: 8192,
};

#[derive(Clone, Debug, PartialEq)]
pub enum WindowCommand {
    TitleChanged(String),
    SetMouseEnabled(bool),
    ListAvailableFonts,
    FocusWindow,
    Minimize,
    #[allow(dead_code)] // Theme change is only used on macOS right now
    ThemeChanged(Option<Theme>),
    #[cfg(windows)]
    RegisterRightClick,
    #[cfg(windows)]
    UnregisterRightClick,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserEvent {
    DrawCommandBatch(Vec<DrawCommand>),
    WindowCommand(WindowCommand),
    SettingsChanged(SettingsChanged),
    ConfigsChanged(Box<HotReloadConfigs>),
    #[allow(dead_code)]
    RedrawRequested,
    NeovimExited,
}

impl From<Vec<DrawCommand>> for UserEvent {
    fn from(value: Vec<DrawCommand>) -> Self {
        UserEvent::DrawCommandBatch(value)
    }
}

impl From<WindowCommand> for UserEvent {
    fn from(value: WindowCommand) -> Self {
        UserEvent::WindowCommand(value)
    }
}

impl From<SettingsChanged> for UserEvent {
    fn from(value: SettingsChanged) -> Self {
        UserEvent::SettingsChanged(value)
    }
}

impl From<HotReloadConfigs> for UserEvent {
    fn from(value: HotReloadConfigs) -> Self {
        UserEvent::ConfigsChanged(Box::new(value))
    }
}

pub fn create_event_loop() -> EventLoop<UserEvent> {
    let mut builder = EventLoop::with_user_event();
    #[cfg(target_os = "macos")]
    builder.with_default_menu(false);
    let event_loop = builder.build().expect("Failed to create winit event loop");
    #[allow(clippy::let_and_return)]
    event_loop
}

pub fn create_window(event_loop: &ActiveEventLoop, maximized: bool, title: &str) -> WindowConfig {
    let icon = load_icon();

    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();

    let window_settings = load_last_window_settings().ok();

    let previous_position = match window_settings {
        Some(PersistentWindowSettings::Windowed { position, .. }) => Some(position),
        _ => None,
    };

    let window_attributes = Window::default_attributes()
        .with_title(title)
        .with_window_icon(Some(icon))
        .with_maximized(maximized)
        .with_transparent(true)
        .with_visible(false);

    #[cfg(target_os = "windows")]
    let window_attributes = if !cmd_line_settings.opengl {
        WindowAttributesExtWindows::with_no_redirection_bitmap(window_attributes, true)
    } else {
        window_attributes
    };

    let frame_decoration = cmd_line_settings.frame;

    #[cfg(target_os = "macos")]
    let title_hidden = cmd_line_settings.title_hidden;

    // There is only two options for windows & linux, no need to match more options.
    #[cfg(not(target_os = "macos"))]
    let mut window_attributes = window_attributes.with_decorations(frame_decoration == Frame::Full);

    #[cfg(target_os = "macos")]
    let mut window_attributes = match frame_decoration {
        Frame::Full => window_attributes,
        Frame::None => window_attributes.with_decorations(false),
        Frame::Buttonless => window_attributes
            .with_title_hidden(title_hidden)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
        Frame::Transparent => window_attributes
            .with_title_hidden(title_hidden)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
    };

    if let Some(previous_position) = previous_position {
        window_attributes = window_attributes.with_position(previous_position);
    }

    #[cfg(target_os = "linux")]
    let window_attributes = {
        if env::var("WAYLAND_DISPLAY").is_ok() {
            let app_id = &cmd_line_settings.wayland_app_id;
            WindowAttributesExtWayland::with_name(window_attributes, "neovide", app_id.clone())
        } else {
            let class = &cmd_line_settings.x11_wm_class;
            let instance = &cmd_line_settings.x11_wm_class_instance;
            WindowAttributesExtX11::with_name(window_attributes, class, instance)
        }
    };

    #[cfg(target_os = "macos")]
    let window_attributes = window_attributes.with_accepts_first_mouse(false);

    #[allow(clippy::let_and_return)]
    let window_config = build_window_config(window_attributes, event_loop);

    #[cfg(target_os = "macos")]
    if let Some(previous_position) = previous_position {
        window_config.window.set_outer_position(previous_position);
    }

    window_config
}

#[derive(Clone, Debug)]
pub enum WindowSize {
    Size(PhysicalSize<u32>),
    Maximized,
    Grid(GridSize<u32>),
    NeovimGrid, // The geometry is read from init.vim/lua
}

pub fn determine_window_size(window_settings: Option<&PersistentWindowSettings>) -> WindowSize {
    let cmd_line = SETTINGS.get::<CmdLineSettings>();

    match cmd_line.geometry {
        GeometryArgs {
            grid: Some(Some(dimensions)),
            ..
        } => WindowSize::Grid(clamped_grid_size(&GridSize::new(
            dimensions.width.try_into().unwrap(),
            dimensions.height.try_into().unwrap(),
        ))),
        GeometryArgs {
            grid: Some(None), ..
        } => WindowSize::NeovimGrid,
        GeometryArgs {
            size: Some(dimensions),
            ..
        } => WindowSize::Size(dimensions.into()),
        GeometryArgs {
            maximized: true, ..
        } => WindowSize::Maximized,
        _ => match window_settings {
            Some(PersistentWindowSettings::Maximized { .. }) => WindowSize::Maximized,
            Some(PersistentWindowSettings::Windowed {
                pixel_size: Some(pixel_size),
                ..
            }) => {
                let size = Size::new(*pixel_size);
                let scale = 1.0;
                WindowSize::Size(
                    Size::clamp(
                        size,
                        MIN_PERSISTENT_WINDOW_SIZE.into(),
                        MAX_PERSISTENT_WINDOW_SIZE.into(),
                        scale,
                    )
                    .to_physical(scale),
                )
            }
            _ => WindowSize::Size(DEFAULT_WINDOW_SIZE),
        },
    }
}

pub fn load_icon() -> Icon {
    let icon = load_from_memory(ICON).expect("Failed to parse icon data");
    let (width, height) = icon.dimensions();
    let mut rgba = Vec::with_capacity((width * height) as usize * 4);
    for (_, _, pixel) in icon.pixels() {
        rgba.extend_from_slice(&pixel.to_rgba().0);
    }
    Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
}
