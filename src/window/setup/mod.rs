use std::fs::File;
use std::io::Read;

use glamour::Size2;
use image::GenericImageView;
use image::Pixel;
use image::load_from_memory;
use winit::dpi::PhysicalSize;
use winit::dpi::Size;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoop;
use winit::window::Cursor;
use winit::window::Icon;
use winit::window::Theme;
use winit::window::Window;

use super::EventPayload;
use crate::cmd_line::CmdLineSettings;
use crate::cmd_line::GeometryArgs;
use crate::renderer::WindowConfig;
use crate::renderer::build_window_config;
use crate::settings::PersistentWindowSettings;
use crate::settings::Settings;
use crate::settings::clamped_grid_size;
use crate::settings::load_last_window_settings;
use crate::units::Grid;
use crate::units::GridSize;
use crate::utils::expand_tilde;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod platform;

#[cfg(target_os = "windows")]
pub(crate) use platform::configure_process_dpi;

static DEFAULT_ICON: &[u8] = include_bytes!("../../../assets/neovide.ico");

const DEFAULT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize { width: 500, height: 500 };
const MIN_PERSISTENT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize { width: 300, height: 150 };
const MAX_PERSISTENT_WINDOW_SIZE: PhysicalSize<u32> = PhysicalSize { width: 8192, height: 8192 };

pub fn create_event_loop() -> EventLoop<EventPayload> {
    let mut builder = EventLoop::with_user_event();
    #[cfg(target_os = "macos")]
    platform::configure_event_loop(&mut builder);
    let event_loop = builder.build().expect("Failed to create winit event loop");
    #[cfg(target_os = "macos")]
    platform::register_file_handler();
    #[allow(clippy::let_and_return)]
    event_loop
}

pub fn create_window(
    event_loop: &ActiveEventLoop,
    maximized: bool,
    title: &str,
    settings: &Settings,
    theme: Option<Theme>,
) -> WindowConfig {
    let cmd_line_settings = settings.get::<CmdLineSettings>();
    let icon = load_icon(cmd_line_settings.icon.as_ref());

    let persistent_window_settings = load_last_window_settings().ok();

    let previous_position = match persistent_window_settings {
        Some(PersistentWindowSettings::Windowed { position, .. }) => Some(position),
        _ => None,
    };

    let window_attributes = Window::default_attributes()
        .with_title(title)
        .with_cursor(Cursor::Icon(cmd_line_settings.mouse_cursor_icon.parse()))
        .with_maximized(maximized)
        .with_transparent(true)
        .with_visible(false)
        .with_theme(theme);

    let window_attributes = platform::create_window_attributes(
        event_loop,
        window_attributes,
        &cmd_line_settings,
        icon,
        previous_position,
    );

    #[allow(clippy::let_and_return)]
    let window_config = build_window_config(window_attributes, event_loop, settings);

    #[cfg(target_os = "macos")]
    platform::apply_previous_position(&window_config, previous_position);

    window_config
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
        GeometryArgs { grid: Some(Some(dimensions)), .. } => {
            WindowSize::Grid(clamped_grid_size(&GridSize::new(
                dimensions.width.try_into().unwrap(),
                dimensions.height.try_into().unwrap(),
            )))
        }
        GeometryArgs { grid: Some(None), .. } => WindowSize::NeovimGrid,
        GeometryArgs { size: Some(dimensions), .. } => WindowSize::Size(dimensions.into()),
        GeometryArgs { maximized: true, .. } => WindowSize::Maximized,
        _ => match window_settings {
            Some(PersistentWindowSettings::Maximized { .. }) => WindowSize::Maximized,
            Some(PersistentWindowSettings::Windowed { pixel_size: Some(pixel_size), .. }) => {
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

pub fn determine_grid_size(
    window_size: &WindowSize,
    window_settings: Option<PersistentWindowSettings>,
) -> Option<Size2<Grid<u32>>> {
    match window_size {
        WindowSize::Grid(grid_size) => Some(*grid_size),
        // Clippy wrongly suggests to use unwrap or default here
        #[allow(clippy::manual_unwrap_or_default)]
        _ => match window_settings {
            Some(PersistentWindowSettings::Maximized { grid_size, .. }) => grid_size,
            Some(PersistentWindowSettings::Windowed { grid_size, .. }) => grid_size,
            _ => None,
        },
    }
}

pub fn load_icon(path: Option<&String>) -> Icon {
    let icon_result = path
        .and_then(|path| {
            let expanded_path = expand_tilde(path);
            let mut file = File::open(expanded_path).ok()?;
            let mut data = Vec::new();
            file.read_to_end(&mut data).ok()?;
            Some(data)
        })
        .map(|data| load_from_memory(&data));

    let icon = match icon_result {
        Some(Ok(icon)) => icon,
        _ => load_from_memory(DEFAULT_ICON).expect("Failed to parse icon data"),
    };

    let (width, height) = icon.dimensions();
    let mut rgba = Vec::with_capacity((width * height) as usize * 4);
    for (_, _, pixel) in icon.pixels() {
        rgba.extend_from_slice(&pixel.to_rgba().0);
    }
    Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
}
