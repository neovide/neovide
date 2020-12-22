mod settings;
#[cfg_attr(feature = "sdl2", path = "sdl2/mod.rs")]
mod window_wrapper;

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crossfire::mpsc::TxUnbounded;
use skulpin::LogicalSize;

use crate::bridge::UiCommand;
use crate::editor::{DrawCommand, WindowCommand};
use crate::renderer::Renderer;
use crate::INITIAL_DIMENSIONS;

#[cfg(feature = "sdl2")]
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
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn handle_new_grid_size(
    new_size: LogicalSize,
    renderer: &Renderer,
    ui_command_sender: &TxUnbounded<UiCommand>,
) {
    if new_size.width > 0 && new_size.height > 0 {
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        let new_width = ((new_size.width + 1) as f32 / renderer.font_width) as u32;
        let new_height = ((new_size.height + 1) as f32 / renderer.font_height) as u32;
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
    let logical_size = LogicalSize {
        width: (width as f32 * renderer.font_width) as u32,
        height: (height as f32 * renderer.font_height + 1.0) as u32,
    };

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
