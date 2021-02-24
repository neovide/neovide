mod keyboard;
mod settings;

#[cfg_attr(feature = "sdl2", path = "sdl2/mod.rs")]
#[cfg_attr(feature = "winit", path = "winit/mod.rs")]
#[cfg_attr(feature = "opengl", path = "glutin/mod.rs")]
mod window_wrapper;

use crate::{
    bridge::UiCommand,
    editor::{DrawCommand, WindowCommand},
    renderer::Renderer,
    INITIAL_DIMENSIONS,
};
use crossfire::mpsc::TxUnbounded;
use std::sync::{atomic::AtomicBool, mpsc::Receiver, Arc};

#[cfg(feature = "sdl2")]
pub use window_wrapper::start_loop;
#[cfg(feature = "winit")]
pub use window_wrapper::start_loop;
#[cfg(feature = "opengl")]
pub use window_wrapper::start_loop;

pub use settings::*;

pub fn window_geometry() -> Result<(u64, u64), String> {
    let prefix = "--geometry=";

    std::env::args()
        .find(|arg| arg.starts_with(prefix))
        .map_or(Ok(INITIAL_DIMENSIONS), |arg| {
            let input = &arg[prefix.len()..];
            let invalid_parse_err = format!(
                "Invalid geometry: {}\nValid format: <width>x<height>",
                input
            );

            input
                .split('x')
                .map(|dimension| {
                    dimension
                        .parse::<u64>()
                        .map_err(|_| invalid_parse_err.as_str())
                        .and_then(|dimension| {
                            if dimension > 0 {
                                Ok(dimension)
                            } else {
                                Err("Invalid geometry: Window dimensions should be greater than 0.")
                            }
                        })
                })
                .collect::<Result<Vec<_>, &str>>()
                .and_then(|dimensions| {
                    if let [width, height] = dimensions[..] {
                        Ok((width, height))
                    } else {
                        Err(invalid_parse_err.as_str())
                    }
                })
                .map_err(|msg| msg.to_owned())
        })
}

pub fn window_geometry_or_default() -> (u64, u64) {
    window_geometry().unwrap_or(INITIAL_DIMENSIONS)
}

#[cfg(target_os = "windows")]
fn windows_fix_dpi() {
    println!("dpi fix applied");
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn handle_new_grid_size(
    new_size: (u32, u32),
    renderer: &Renderer,
    ui_command_sender: &TxUnbounded<UiCommand>,
) {
    let (new_width, new_height) = new_size;
    if new_width > 0 && new_height > 0 {
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        let new_width = ((new_width + 1) as f32 / renderer.font_width) as u32;
        let new_height = ((new_height + 1) as f32 / renderer.font_height) as u32;
        ui_command_sender
            .send(UiCommand::Resize {
                width: new_width,
                height: new_height,
            })
            .ok();
    }
}

pub fn create_window(
    batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
    window_command_receiver: Receiver<WindowCommand>,
    ui_command_sender: TxUnbounded<UiCommand>,
    running: Arc<AtomicBool>,
) {
    let (width, height) = window_geometry_or_default();

    let renderer = Renderer::new(batched_draw_command_receiver);
    let logical_size = (
        (width as f32 * renderer.font_width) as u32,
        (height as f32 * renderer.font_height + 1.0) as u32,
    );

    #[cfg(target_os = "windows")]
    windows_fix_dpi();

    start_loop(
        window_command_receiver,
        ui_command_sender,
        running,
        logical_size,
        renderer,
    );
}
