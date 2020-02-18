use std::time::{Duration, Instant};
use std::thread::sleep;

use log::{info, debug, error};
use skulpin::{LogicalSize, PhysicalSize};
use skulpin::sdl2;
use skulpin::sdl2::event::Event;
use skulpin::sdl2::keyboard::Mod;
use skulpin::{RendererBuilder, PresentMode, CoordinateSystem, dpis};

use crate::bridge::{parse_keycode, append_modifiers, BRIDGE, UiCommand};
use crate::renderer::Renderer;
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::editor::EDITOR;
use crate::settings::SETTINGS;
use crate::INITIAL_DIMENSIONS;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

#[cfg(target_os = "windows")]
fn windows_fix_dpi() {
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn handle_new_grid_size(new_size: LogicalSize, renderer: &Renderer) {
    if new_size.width > 0 && new_size.height > 0 {
        let new_width = ((new_size.width + 1) as f32 / renderer.font_width) as u32;
        let new_height = ((new_size.height + 1) as f32 / renderer.font_height) as u32;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        BRIDGE.queue_command(UiCommand::Resize { width: new_width, height: new_height });
    }
}

pub fn ui_loop() {
    let sdl_context = sdl2::init().expect("Failed to initialize sdl2");
    let video_subsystem = sdl_context.video().expect("Failed to create sdl video subsystem");

    let (width, height) = INITIAL_DIMENSIONS;

    let mut renderer = Renderer::new();
    let logical_size = LogicalSize {
        width: (width as f32 * renderer.font_width) as u32, 
        height: (height as f32 * renderer.font_height + 1.0) as u32
    };

    // let icon = {
    //     let icon_data = Asset::get("nvim.ico").expect("Failed to read icon data");
    //     let icon = load_from_memory(&icon_data).expect("Failed to parse icon data");
    //     let (width, height) = icon.dimensions();
    //     let mut rgba = Vec::with_capacity((width * height) as usize * 4);
    //     for (_, _, pixel) in icon.pixels() {
    //         rgba.extend_from_slice(&pixel.to_rgba().0);
    //     }
    //     Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
    // };
    // info!("icon created");
    #[cfg(target_os = "windows")]
    windows_fix_dpi();
    sdl2::hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");

    let mut window = video_subsystem.window("Neovide", logical_size.width, logical_size.height)
            .position_centered()
            .allow_highdpi()
            .resizable()
            .vulkan()
            .build()
            .expect("Failed to create window");
    info!("window created");

    let mut skulpin_renderer = RendererBuilder::new()
        .prefer_integrated_gpu()
        .use_vulkan_debug_layer(true)
        .present_mode_priority(vec![PresentMode::Immediate])
        .coordinate_system(CoordinateSystem::Logical)
        .build(&window)
        .expect("Failed to create renderer");
    info!("renderer created");

    let mut mouse_down = false;
    let mut mouse_position = LogicalSize {
        width: 0, 
        height: 0
    };

    let mut title = "Neovide".to_string();
    let mut previous_size = LogicalSize::new(&window).unwrap();
    let mut previous_dpis = dpis(&window).unwrap();

    info!("Starting window event loop");
    let mut event_pump = sdl_context.event_pump().expect("Could not create sdl event pump");
    'running: loop {
        let frame_start = Instant::now();

        let editor_title = { EDITOR.lock().title.clone() };
        if title != editor_title {
            title = editor_title;
            window.set_title(&title).expect("Could not set title");
        }

        let mut ignore_text_input = false;
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {..} => break 'running,
                Event::Window {..} => REDRAW_SCHEDULER.queue_next_frame(),
                Event::KeyDown { keycode: Some(keycode), keymod: modifiers, .. } => {
                    if let Some((key_text, special)) = parse_keycode(keycode) {
                        let will_text_input =
                            
                            !modifiers.contains(Mod::LCTRLMOD) &&
                            !modifiers.contains(Mod::RCTRLMOD) &&
                            !modifiers.contains(Mod::LALTMOD) &&
                            !modifiers.contains(Mod::RALTMOD) &&
                            !modifiers.contains(Mod::LGUIMOD) &&
                            !modifiers.contains(Mod::RGUIMOD);
                        if will_text_input && !special {
                            break;
                        }

                        BRIDGE.queue_command(UiCommand::Keyboard(append_modifiers(modifiers, key_text, special)));
                        ignore_text_input = true;
                    }
                },
                Event::TextInput { text, .. } => {
                    if ignore_text_input {
                        ignore_text_input = false;
                    } else {
                        let text = if text == "<" {
                            String::from("<lt>")
                        } else {
                            text
                        };
                        BRIDGE.queue_command(UiCommand::Keyboard(text))
                    }
                },
                Event::MouseMotion { x, y, .. } => {
                    let previous_position = mouse_position;
                    mouse_position = LogicalSize::from_physical_size_tuple((
                            (x as f32 / renderer.font_width) as u32,
                            (y as f32 / renderer.font_height) as u32
                        ), 
                        &window
                    ).expect("Could not calculate logical mouse position");
                    if mouse_down && previous_position != mouse_position {
                        BRIDGE.queue_command(UiCommand::Drag(mouse_position.width, mouse_position.height));
                    }
                },
                Event::MouseButtonDown { .. } => {
                    BRIDGE.queue_command(UiCommand::MouseButton { action: String::from("press"), position: (mouse_position.width, mouse_position.height) });
                    mouse_down = true;
                },
                Event::MouseButtonUp { .. } => {
                    BRIDGE.queue_command(UiCommand::MouseButton { action: String::from("release"), position: (mouse_position.width, mouse_position.height) });
                    mouse_down = false;
                },
                Event::MouseWheel { x, y, .. } => {
                    let vertical_input_type = if y > 0 {
                        Some("up")
                    } else if y < 0 {
                        Some("down")
                    } else {
                        None
                    };

                    if let Some(input_type) = vertical_input_type {
                        BRIDGE.queue_command(UiCommand::Scroll { direction: input_type.to_string(), position: (mouse_position.width, mouse_position.height) });
                    }

                    let horizontal_input_type = if x > 0 {
                        Some("right")
                    } else if x < 0 {
                        Some("left")
                    } else {
                        None
                    };

                    if let Some(input_type) = horizontal_input_type {
                        BRIDGE.queue_command(UiCommand::Scroll { direction: input_type.to_string(), position: (mouse_position.width, mouse_position.height) });
                    }
                },
                _ => {}
            }
        }

        let new_size = LogicalSize::new(&window).unwrap();
        if previous_size != new_size {
            handle_new_grid_size(new_size, &renderer);
            previous_size = new_size;
        }

        let new_dpis = dpis(&window).unwrap();
        if previous_dpis != new_dpis {
            let physical_size = PhysicalSize::new(&window);
            window.set_size(
                (physical_size.width as f32 * new_dpis.0 / previous_dpis.0) as u32,
                (physical_size.height as f32 * new_dpis.1 / previous_dpis.1) as u32).unwrap();
            previous_dpis = new_dpis;
        }

        debug!("Render Triggered");
        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get("no_idle").read_bool() {
            if skulpin_renderer.draw(&window, |canvas, coordinate_system_helper| {
                if renderer.draw(canvas, coordinate_system_helper) {
                    handle_new_grid_size(new_size, &renderer)
                }
            }).is_err() {
                error!("Render failed. Closing");
                break;
            }
        }

        let elapsed = frame_start.elapsed();
        let refresh_rate = SETTINGS.get("refresh_rate").read_u16() as f32;
        let frame_length = Duration::from_secs_f32(1.0 / refresh_rate);
        if elapsed < frame_length {
            sleep(frame_length - elapsed);
        }
    }
}
