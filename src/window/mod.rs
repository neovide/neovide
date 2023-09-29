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
    event_loop::EventLoop,
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
use window_wrapper::WinitWindowWrapper;

use crate::{
    cmd_line::CmdLineSettings,
    frame::Frame,
    profiling::tracy_create_gpu_context,
    renderer::build_window,
    settings::{load_last_window_settings, PersistentWindowSettings, SETTINGS},
};
pub use settings::{KeyboardSettings, WindowSettings};

static ICON: &[u8] = include_bytes!("../../assets/neovide.ico");

#[derive(Clone, Debug)]
pub enum WindowCommand {
    TitleChanged(String),
    SetMouseEnabled(bool),
    ListAvailableFonts,
    FocusWindow,
}

pub fn create_window() {
    let icon = {
        let icon = load_from_memory(ICON).expect("Failed to parse icon data");
        let (width, height) = icon.dimensions();
        let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        for (_, _, pixel) in icon.pixels() {
            rgba.extend_from_slice(&pixel.to_rgba().0);
        }
        Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
    };

    let event_loop = EventLoop::new();

    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();

    let mut maximized = cmd_line_settings.maximized;
    let mut previous_position = None;
    if let Ok(last_window_settings) = load_last_window_settings() {
        match last_window_settings {
            PersistentWindowSettings::Maximized => {
                maximized = true;
            }
            PersistentWindowSettings::Windowed { position, .. } => {
                previous_position = Some(position);
            }
        }
    }

    let winit_window_builder = WindowBuilder::new()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_maximized(maximized)
        .with_transparent(true);

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
        if !maximized {
            winit_window_builder = winit_window_builder.with_position(previous_position);
        }
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

    let (window, config) = build_window(winit_window_builder, &event_loop);
    let mut window_wrapper = WinitWindowWrapper::new(
        window,
        config,
        &cmd_line_settings,
        previous_position,
        maximized,
    );

    tracy_create_gpu_context("main_render_context");

    let mut update_loop = UpdateLoop::new();

    event_loop.run(move |e, _window_target, control_flow| {
        *control_flow = update_loop.step(&mut window_wrapper, e);
    });
}
