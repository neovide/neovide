use std::sync::Arc;
use std::time::{Duration, Instant};

use image::{load_from_memory, GenericImageView, Pixel};
use skulpin::{CoordinateSystem, RendererBuilder, PresentMode};
use skulpin::skia_safe::icu;
use skulpin::winit::dpi::LogicalSize;
use skulpin::winit::event::{ElementState, Event, MouseScrollDelta, StartCause, WindowEvent};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::{Icon, WindowBuilder};

use crate::bridge::{construct_keybinding_string, BRIDGE, UiCommand};
use crate::renderer::Renderer;
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::INITIAL_DIMENSIONS;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

fn handle_new_grid_size(new_size: LogicalSize, renderer: &Renderer) {
    if new_size.width > 0.0 && new_size.height > 0.0 {
        let new_width = ((new_size.width + 1.0) as f32 / renderer.font_width) as u64;
        let new_height = ((new_size.height + 1.0) as f32 / renderer.font_height) as u64;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        BRIDGE.queue_command(UiCommand::Resize { width: new_width as i64, height: new_height as i64 });
    }
}

pub fn ui_loop() {
    let event_loop = EventLoop::<()>::with_user_event();

    let mut renderer = Renderer::new();
    let (width, height) = INITIAL_DIMENSIONS;
    let logical_size = LogicalSize::new(
        (width as f32 * renderer.font_width) as f64, 
        (height as f32 * renderer.font_height + 1.0) as f64
    );

    let icon = {
        let icon_data = Asset::get("nvim.ico").expect("Failed to read icon data");
        let icon = load_from_memory(&icon_data).expect("Failed to parse icon data");
        let (width, height) = icon.dimensions();
        let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        for (_, _, pixel) in icon.pixels() {
            rgba.extend_from_slice(&pixel.to_rgba().0);
        }
        Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
    };

    let window = Arc::new(WindowBuilder::new()
        .with_title("Neovide")
        .with_inner_size(logical_size)
        .with_window_icon(Some(icon))
        .build(&event_loop)
        .expect("Failed to create window"));

    let mut skulpin_renderer = RendererBuilder::new()
        .prefer_integrated_gpu()
        .use_vulkan_debug_layer(true)
        .present_mode_priority(vec![PresentMode::Mailbox, PresentMode::Immediate])
        .coordinate_system(CoordinateSystem::Logical)
        .build(&window)
        .expect("Failed to create renderer");

    icu::init();

    let mut mouse_down = false;
    let mut mouse_pos = (0, 0);

    event_loop.run(move |event, _window_target, control_flow| {
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
                handle_new_grid_size(new_size, &renderer)
            },

            Event::WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input,
                    ..
                },
                ..
            } => {
                construct_keybinding_string(input)
                    .map(UiCommand::Keyboard)
                    .map(|keybinding_string| BRIDGE.queue_command(keybinding_string));
            },

            Event::WindowEvent {
                event: WindowEvent::CursorMoved {
                    position,
                    ..
                },
                ..
            } => {
                let grid_y = (position.x as f32 / renderer.font_width) as i64;
                let grid_x = (position.y as f32 / renderer.font_height) as i64;
                mouse_pos = (grid_x, grid_y);
                if mouse_down {
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
                let input_type = match state {
                    ElementState::Pressed => {
                        mouse_down = true;
                        "press"
                    },
                    ElementState::Released => {
                        mouse_down = false;
                        "release"
                    }
                };
                let (grid_x, grid_y) = mouse_pos;
                BRIDGE.queue_command(UiCommand::MouseButton { action: input_type.to_string(), position: (grid_x, grid_y) });
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
                let frame_start = Instant::now();

                if REDRAW_SCHEDULER.should_draw() {
                    skulpin_renderer.draw(&window, |canvas, coordinate_system_helper| {
                        if renderer.draw(canvas, coordinate_system_helper) {
                            handle_new_grid_size(window.inner_size(), &renderer)
                        }
                    }).ok();
                }

                *control_flow = ControlFlow::WaitUntil(frame_start + Duration::from_secs_f32(1.0 / 60.0));
            },

            _ => {}
        }
    })
}
