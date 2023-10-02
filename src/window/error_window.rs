use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use crate::{
    cmd_line::SRGB_DEFAULT,
    renderer::{build_context, build_window, GlWindow},
    window::{load_icon, UserEvent},
};

pub fn show_error_window(_message: &str, event_loop: EventLoop<UserEvent>) {
    let srgb = SRGB_DEFAULT == "1";
    let vsync = true;
    let window = create_window(&event_loop);
    let _context = build_context(window, srgb, vsync);

    let _ = event_loop.run(move |e, window_target| match e {
        Event::LoopExiting => {}
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            window_target.exit();
        }
        _ => {}
    });
}

fn create_window(event_loop: &EventLoop<UserEvent>) -> GlWindow {
    let icon = load_icon();

    let winit_window_builder = WindowBuilder::new()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_transparent(false)
        .with_visible(true)
        .with_decorations(true);

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

    build_window(winit_window_builder, event_loop)
}
