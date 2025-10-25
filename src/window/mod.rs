pub mod error_window;
mod keyboard_manager;
mod mouse_manager;
pub mod settings;
mod update_loop;
mod window_wrapper;

#[cfg(target_os = "macos")]
use crate::platform::macos;

#[cfg(target_os = "linux")]
use crate::platform::linux;

use winit::{
    dpi::{PhysicalSize, Size},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Icon, Theme},
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;

use image::{load_from_memory, GenericImageView, Pixel};
use keyboard_manager::KeyboardManager;
use mouse_manager::MouseManager;

use crate::{
    cmd_line::{CmdLineSettings, GeometryArgs},
    renderer::{DrawCommand, WindowConfig},
    settings::{
        clamped_grid_size, save_window_size, HotReloadConfigs, PersistentWindowSettings, Settings,
        SettingsChanged,
    },
    units::GridSize,
};

#[cfg(target_os = "macos")]
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
    #[cfg(target_os = "macos")]
    return macos::window::create_event_loop();

    #[cfg(not(target_os = "macos"))]
    {
        let mut builder = EventLoop::with_user_event();
        builder.build().expect("Failed to create winit event loop")
    }
}

pub fn create_window(
    event_loop: &ActiveEventLoop,
    maximized: bool,
    title: &str,
    settings: &Settings,
) -> WindowConfig {
    #[cfg(target_os = "macos")]
    return macos::window::create_window(event_loop, maximized, title, settings);

    #[cfg(target_os = "windows")]
    return crate::platform::windows::window::create_window(event_loop, maximized, title, settings);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return linux::window::create_window(event_loop, maximized, title, settings);
}

#[derive(Clone, Debug)]
pub enum WindowSize {
    Size(PhysicalSize<u32>),
    Maximized,
    Grid(GridSize<u32>),
    NeovimGrid, // The geometry is read from init.vim/lua
}

pub fn determine_window_size(
    window_settings: Option<&PersistentWindowSettings>,
    settings: &Settings,
) -> WindowSize {
    let cmd_line = settings.get::<CmdLineSettings>();

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
