mod keyboard;
mod settings;
mod window_wrapper;

use crate::{
    bridge::UiCommand,
    channel_utils::*,
    cmd_line::CmdLineSettings,
    editor::{DrawCommand, WindowCommand},
    renderer::Renderer,
    settings::SETTINGS,
    INITIAL_DIMENSIONS,
};
use std::sync::{atomic::AtomicBool, mpsc::Receiver, Arc};

pub use window_wrapper::start_loop;

pub use settings::*;

pub fn window_geometry() -> Result<(u64, u64), String> {
    //TODO: Maybe move this parsing into cmd_line...
    SETTINGS
        .get::<CmdLineSettings>()
        .geometry
        .map_or(Ok(INITIAL_DIMENSIONS), |input| {
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
    new_size: (u64, u64),
    renderer: &Renderer,
    ui_command_sender: &LoggingTx<UiCommand>,
) {
    let (new_width, new_height) = new_size;
    if new_width > 0 && new_height > 0 {
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        let new_width = ((new_width + 1) / renderer.font_width) as u32;
        let new_height = ((new_height + 1) / renderer.font_height) as u32;
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
    ui_command_sender: LoggingTx<UiCommand>,
    running: Arc<AtomicBool>,
) {
    let (width, height) = window_geometry_or_default();

    let renderer = Renderer::new(batched_draw_command_receiver);
    let logical_size = (
        (width * renderer.font_width) as u64,
        (height * renderer.font_height + 1) as u64,
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
