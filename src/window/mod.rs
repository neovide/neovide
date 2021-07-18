mod settings;
mod window_wrapper;

use crate::{
    bridge::UiCommand,
    channel_utils::*,
    editor::{DrawCommand, WindowCommand},
    renderer::Renderer,
};
use glutin::dpi::PhysicalSize;
use std::sync::{atomic::AtomicBool, mpsc::Receiver, Arc};

pub use window_wrapper::start_loop;

pub use settings::*;

#[cfg(target_os = "windows")]
fn windows_fix_dpi() {
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn handle_new_grid_size(
    new_size: PhysicalSize<u32>,
    renderer: &Renderer,
    ui_command_sender: &LoggingTx<UiCommand>,
) {
    if new_size.width > 0 && new_size.height > 0 {
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        let width = ((new_size.width + 1) / renderer.font_width as u32) as u32;
        let height = ((new_size.height + 1) / renderer.font_height as u32) as u32;
        ui_command_sender
            .send(UiCommand::Resize { width, height })
            .ok();
    }
}

pub fn create_window(
    batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
    window_command_receiver: Receiver<WindowCommand>,
    ui_command_sender: LoggingTx<UiCommand>,
    running: Arc<AtomicBool>,
) {
    #[cfg(target_os = "windows")]
    windows_fix_dpi();

    start_loop(
        batched_draw_command_receiver,
        window_command_receiver,
        ui_command_sender,
        running,
    );
}
