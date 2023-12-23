mod error_window;
mod keyboard_manager;
mod mouse_manager;
mod renderer;
mod settings;
mod update_loop;
mod window_wrapper;

#[cfg(target_os = "macos")]
mod draw_background;

#[cfg(target_os = "linux")]
use std::env;

use winit::{
    dpi::{PhysicalSize, Size},
    error::EventLoopError,
    event::Event,
    event_loop::{EventLoop, EventLoopBuilder},
    window::{Icon, WindowBuilder},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;

#[cfg(target_os = "macos")]
use draw_background::draw_background;

#[cfg(target_os = "linux")]
use winit::platform::wayland::WindowBuilderExtWayland;
#[cfg(target_os = "linux")]
use winit::platform::x11::WindowBuilderExtX11;

use image::{load_from_memory, GenericImageView, Pixel};
use keyboard_manager::KeyboardManager;
use mouse_manager::MouseManager;
use renderer::SkiaRenderer;
use update_loop::UpdateLoop;

use crate::{
    cmd_line::{CmdLineSettings, GeometryArgs},
    dimensions::Dimensions,
    frame::Frame,
    renderer::{build_window, DrawCommand, GlWindow},
    running_tracker::*,
    settings::{
        load_last_window_settings, save_window_size, PersistentWindowSettings, SettingsChanged,
        SETTINGS,
    },
};
pub use error_window::show_error_window;
pub use settings::{WindowSettings, WindowSettingsChanged};
pub use update_loop::ShouldRender;
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
    ShowIntro(Vec<String>),
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
    #[allow(dead_code)]
    RedrawRequested,
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

pub fn create_event_loop() -> EventLoop<UserEvent> {
    EventLoopBuilder::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create winit event loop")
}

pub fn create_window(
    event_loop: &EventLoop<UserEvent>,
    initial_window_size: &WindowSize,
) -> GlWindow {
    let icon = load_icon();

    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();

    let window_settings = load_last_window_settings().ok();

    let previous_position = match window_settings {
        Some(PersistentWindowSettings::Windowed { position, .. }) => Some(position),
        _ => None,
    };

    log::trace!("Settings initial_window_size {:?}", initial_window_size);

    // NOTE: For Geometry, the window is resized when it's shown based on the font and other
    // settings.
    let inner_size = match *initial_window_size {
        WindowSize::Size(size) => size,
        _ => DEFAULT_WINDOW_SIZE,
    };

    let winit_window_builder = WindowBuilder::new()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_inner_size(inner_size)
        // Unfortunately we can't maximize here, because winit shows the window momentarily causing
        // flickering
        .with_maximized(false)
        .with_transparent(true)
        .with_visible(false);

    let frame_decoration = cmd_line_settings.frame;

    // There is only two options for windows & linux, no need to match more options.
    #[cfg(not(target_os = "macos"))]
    let mut winit_window_builder =
        winit_window_builder.with_decorations(frame_decoration == Frame::Full);

    #[cfg(target_os = "macos")]
    let mut winit_window_builder = match frame_decoration {
        Frame::Full => winit_window_builder,
        Frame::None => winit_window_builder.with_decorations(false),
        Frame::Buttonless => winit_window_builder
            .with_transparent(true)
            .with_title_hidden(true)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
        Frame::Transparent => winit_window_builder
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
    };

    if let Some(previous_position) = previous_position {
        winit_window_builder = winit_window_builder.with_position(previous_position);
    }

    #[cfg(target_os = "linux")]
    let winit_window_builder = {
        if env::var("WAYLAND_DISPLAY").is_ok() {
            let app_id = &cmd_line_settings.wayland_app_id;
            WindowBuilderExtWayland::with_name(winit_window_builder, "neovide", app_id.clone())
        } else {
            let class = &cmd_line_settings.x11_wm_class;
            let instance = &cmd_line_settings.x11_wm_class_instance;
            WindowBuilderExtX11::with_name(winit_window_builder, class, instance)
        }
    };

    #[cfg(target_os = "macos")]
    let winit_window_builder = winit_window_builder.with_accepts_first_mouse(false);

    let gl_window = build_window(winit_window_builder, event_loop);
    let window = &gl_window.window;

    // Check that window is visible in some monitor, and reposition it if not.
    window.current_monitor().and_then(|current_monitor| {
        let monitor_position = current_monitor.position();
        let monitor_size = current_monitor.size();
        let monitor_width = monitor_size.width as i32;
        let monitor_height = monitor_size.height as i32;

        let window_position = previous_position.or_else(|| window.outer_position().ok())?;

        let window_size = window.outer_size();
        let window_width = window_size.width as i32;
        let window_height = window_size.height as i32;

        if window_position.x + window_width < monitor_position.x
            || window_position.y + window_height < monitor_position.y
            || window_position.x > monitor_position.x + monitor_width
            || window_position.y > monitor_position.y + monitor_height
        {
            window.set_outer_position(monitor_position);
        }

        Some(())
    });

    gl_window
}

#[derive(Clone, Debug)]
pub enum WindowSize {
    Size(PhysicalSize<u32>),
    Maximized,
    Grid(Dimensions),
    NeovimGrid, // The geometry is read from init.vim/lua
}

pub fn determine_window_size(window_settings: Option<&PersistentWindowSettings>) -> WindowSize {
    let cmd_line = SETTINGS.get::<CmdLineSettings>();

    match cmd_line.geometry {
        GeometryArgs {
            grid: Some(Some(dimensions)),
            ..
        } => WindowSize::Grid(dimensions.clamped_grid_size()),
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
            Some(PersistentWindowSettings::Maximized) => WindowSize::Maximized,
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

pub fn main_loop(
    window: GlWindow,
    initial_window_size: WindowSize,
    event_loop: EventLoop<UserEvent>,
) -> Result<(), EventLoopError> {
    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
    let mut window_wrapper =
        WinitWindowWrapper::new(window, initial_window_size, event_loop.create_proxy());

    let mut update_loop = UpdateLoop::new(cmd_line_settings.idle);

    event_loop.run(move |e, window_target| {
        if e == Event::LoopExiting {
            return;
        }

        if !RUNNING_TRACKER.is_running() {
            save_window_size(&window_wrapper);
            window_target.exit();
        } else {
            window_target.set_control_flow(update_loop.step(&mut window_wrapper, Ok(e)).unwrap());
        }
    })
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
