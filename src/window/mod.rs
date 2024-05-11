mod error_window;
mod keyboard_manager;
mod mouse_manager;
mod settings;
mod update_loop;
mod window_wrapper;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use std::env;

#[cfg(target_os = "macos")]
use icrate::Foundation::MainThreadMarker;

use winit::{
    dpi::{PhysicalSize, Size},
    error::EventLoopError,
    event::Event,
    event_loop::{EventLoop, EventLoopBuilder},
    window::{Icon, Theme, WindowBuilder},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;

#[cfg(target_os = "linux")]
use winit::platform::{wayland::WindowBuilderExtWayland, x11::WindowBuilderExtX11};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowBuilderExtWindows;

#[cfg(target_os = "macos")]
use winit::platform::macos::EventLoopBuilderExtMacOS;

use image::{load_from_memory, GenericImageView, Pixel};
use keyboard_manager::KeyboardManager;
use mouse_manager::MouseManager;
use update_loop::UpdateLoop;

use crate::{
    cmd_line::{CmdLineSettings, GeometryArgs},
    frame::Frame,
    renderer::{build_window_config, DrawCommand, WindowConfig},
    settings::{
        clamped_grid_size, load_last_window_settings, save_window_size, FontSettings,
        HotReloadConfigs, PersistentWindowSettings, SettingsChanged, SETTINGS,
    },
    units::GridSize,
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
    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    builder.with_default_menu(false);
    let event_loop = builder.build().expect("Failed to create winit event loop");
    #[cfg(target_os = "macos")]
    crate::window::macos::register_file_handler();
    #[allow(clippy::let_and_return)]
    event_loop
}

pub fn create_window(
    event_loop: &EventLoop<UserEvent>,
    initial_window_size: &WindowSize,
) -> WindowConfig {
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

    #[cfg(target_os = "windows")]
    let winit_window_builder = if !cmd_line_settings.opengl {
        WindowBuilderExtWindows::with_no_redirection_bitmap(winit_window_builder, true)
    } else {
        winit_window_builder
    };

    let frame_decoration = cmd_line_settings.frame;

    #[cfg(target_os = "macos")]
    let title_hidden = cmd_line_settings.title_hidden;

    // There is only two options for windows & linux, no need to match more options.
    #[cfg(not(target_os = "macos"))]
    let mut winit_window_builder =
        winit_window_builder.with_decorations(frame_decoration == Frame::Full);

    #[cfg(target_os = "macos")]
    let mut winit_window_builder = match frame_decoration {
        Frame::Full => winit_window_builder,
        Frame::None => winit_window_builder.with_decorations(false),
        Frame::Buttonless => winit_window_builder
            .with_title_hidden(title_hidden)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
        Frame::Transparent => winit_window_builder
            .with_title_hidden(title_hidden)
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

    let window_config = build_window_config(winit_window_builder, event_loop);
    let window = &window_config.window;

    #[cfg(target_os = "macos")]
    if let Some(previous_position) = previous_position {
        window.set_outer_position(previous_position);
    }

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

pub fn main_loop(
    window: WindowConfig,
    initial_window_size: WindowSize,
    initial_font_settings: Option<FontSettings>,
    event_loop: EventLoop<UserEvent>,
) -> Result<(), EventLoopError> {
    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
    let window_wrapper = WinitWindowWrapper::new(
        window,
        initial_window_size,
        initial_font_settings,
        event_loop.create_proxy(),
    );

    let mut update_loop = UpdateLoop::new(cmd_line_settings.idle);
    let mut window_wrapper = Some(window_wrapper);

    #[cfg(target_os = "macos")]
    let mut menu = {
        let mtm = MainThreadMarker::new().expect("must be on the main thread");
        macos::Menu::new(mtm)
    };
    let res = event_loop.run(move |e, window_target| {
        #[cfg(target_os = "macos")]
        menu.ensure_menu_added(&e);

        match e {
            Event::LoopExiting => window_wrapper = None,
            Event::UserEvent(UserEvent::NeovimExited) => {
                save_window_size(window_wrapper.as_ref().unwrap());
                window_target.exit();
            }
            _ => window_target
                .set_control_flow(update_loop.step(window_wrapper.as_mut().unwrap(), e)),
        }
    });
    res
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
