use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use std::thread::sleep;

use image::{load_from_memory, GenericImageView, Pixel};
use log::{info, debug, trace, error};

use crate::bridge::{construct_keybinding_string, BRIDGE, UiCommand};
use crate::renderer::Renderer;
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::editor::EDITOR;
use crate::settings::SETTINGS;
use crate::INITIAL_DIMENSIONS;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

fn handle_new_grid_size(new_size: LogicalSize<f32>, renderer: &Renderer) {
    if new_size.width > 0.0 && new_size.height > 0.0 {
        let new_width = ((new_size.width + 1.0) / renderer.font_width) as i64;
        let new_height = ((new_size.height + 1.0) / renderer.font_height) as i64;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        BRIDGE.queue_command(UiCommand::Resize { width: new_width as i64, height: new_height as i64 });
    }
}

pub fn ui_loop() {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let (width, height) = INITIAL_DIMENSIONS;


    let event_loop = EventLoop::<()>::with_user_event();

    let mut renderer = Renderer::new();
    let logical_size = LogicalSize::new(
        (width as f32 * renderer.font_width) as f64, 
        (height as f32 * renderer.font_height + 1.0) as f64
    );

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

    let mut window = video_subsystem.window("Neovide", width, height)
            .position_centered()
            .vulkan()
            .build()
            .expect("Failed to create window");
    info!("window created");

    let mut skulpin_renderer = RendererBuilder::new()
        .prefer_integrated_gpu()
        .use_vulkan_debug_layer(true)
        .present_mode_priority(vec![PresentMode::Mailbox, PresentMode::Immediate])
        .coordinate_system(CoordinateSystem::Logical)
        .build(&window)
        .expect("Failed to create renderer");
    info!("renderer created");

    let mut mouse_down = false;
    let mut mouse_pos = (0, 0);

    let mut allow_next_char = false;
    let mut next_char_modifiers = ModifiersState::empty();

    info!("Starting window event loop");
    let mut event_pump = sdl_context.event_pump()?;
    loop {
        let frame_start = Instant::now();

        let editor_title = { EDITOR.lock().title.clone() };
        if title != editor_title {
            title = editor_title;
            window.set_title(&title);
        }

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get("no_idle").read_bool() {
            debug!("Render Triggered");
            if skulpin_renderer.draw(&window, |canvas, coordinate_system_helper| {
                if renderer.draw(canvas, coordinate_system_helper) {
                    handle_new_grid_size(window.inner_size().to_logical(window.scale_factor()), &renderer)
                }
            }).is_err() {
                error!("Render failed. Closing");
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        let elapsed = frame_start.since();
        let frame_length = Duration::from_secs_f32(1.0 / 60.0);
        if elapsed < frame_length {
            sleep(frame_length - elapsed);
        }
    }

    event_loop.run(move |event, _window_target, control_flow| {
        trace!("Window Event: {:?}", event);
        match event {
            Event::NewEvents(StartCause::Init) |
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                window.request_redraw()
            },

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,

            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                handle_new_grid_size(new_size.to_logical(window.scale_factor()), &renderer)
            },

            Event::WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input,
                    ..
                },
                ..
            } => {
                // Only interpret 'char' events when we get a previous event without a virtual
                // keycode (which we ignore for KeyboardInput events).
                // This is a hack so we don't lose a bunch of input events on Linux
                if input.virtual_keycode == None {
                    allow_next_char = true;
                }else {
                    allow_next_char = false;
                }
                next_char_modifiers = input.modifiers;

                if let Some(keybinding_string) = construct_keybinding_string(input)
                    .map(UiCommand::Keyboard) {
                        BRIDGE.queue_command(keybinding_string);
                }
            },

            Event::WindowEvent {
                event: WindowEvent::ReceivedCharacter(c),
                ..
            } => {
                if allow_next_char {
                    next_char_modifiers.remove(ModifiersState::SHIFT);
                    let keybinding = super::bridge::append_modifiers(next_char_modifiers, &c.to_string(), false);
                    BRIDGE.queue_command(UiCommand::Keyboard(keybinding));
                }
            },

            Event::WindowEvent {
                event: WindowEvent::CursorMoved {
                    position,
                    ..
                },
                ..
            } => {
                let position: LogicalPosition<f64> = position.to_logical(window.scale_factor());
                let grid_y = (position.x / renderer.font_width as f64) as i64;
                let grid_x = (position.y / renderer.font_height as f64) as i64;
                let (old_x, old_y) = mouse_pos;
                mouse_pos = (grid_x, grid_y);
                if mouse_down && (old_x != grid_x || old_y != grid_y) {
                    BRIDGE.queue_command(UiCommand::Drag(grid_x, grid_y));
                }
            }

            Event::WindowEvent {
                event: WindowEvent::MouseInput {
                    state,
                    ..
                },
                ..
            } => {
                let input_type = match (state, mouse_down) {
                    (ElementState::Pressed, false) => {
                        mouse_down = true;
                        Some("press")
                    },
                    (ElementState::Released, true) => {
                        mouse_down = false;
                        Some("release")
                    },
                    _ => None
                };

                if let Some(input_type) = input_type {
                    let (grid_x, grid_y) = mouse_pos;
                    BRIDGE.queue_command(UiCommand::MouseButton { action: input_type.to_string(), position: (grid_x, grid_y) });
                }
            }

            Event::WindowEvent {
                event: WindowEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(horizontal, vertical),
                    ..
                },
                ..
            } => {
                let vertical_input_type = if vertical > 0.0 {
                    Some("up")
                } else if vertical < 0.0 {
                    Some("down")
                } else {
                    None
                };

                if let Some(input_type) = vertical_input_type {
                    BRIDGE.queue_command(UiCommand::Scroll { direction: input_type.to_string(), position: mouse_pos });
                }

                let horizontal_input_type = if horizontal > 0.0 {
                    Some("right")
                } else if horizontal < 0.0 {
                    Some("left")
                } else {
                    None
                };

                if let Some(input_type) = horizontal_input_type {
                    BRIDGE.queue_command(UiCommand::Scroll { direction: input_type.to_string(), position: mouse_pos });
                }
            }

            Event::RedrawRequested { .. } => {

                *control_flow = ControlFlow::WaitUntil(frame_start + Duration::from_secs_f32(1.0 / 60.0));
            },

            _ => {}
        }
    })
}
