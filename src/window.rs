use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use image::{load_from_memory, GenericImageView, Pixel};
use skulpin::{CoordinateSystem, RendererBuilder, PresentMode};
use skulpin::skia_safe::icu;
use skulpin::winit::dpi::LogicalSize;
use skulpin::winit::event::{ElementState, Event, MouseScrollDelta, StartCause, WindowEvent};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::{Icon, WindowBuilder};
use tokio::sync::mpsc::UnboundedSender;

use crate::editor::Editor;
use crate::keybindings::construct_keybinding_string;
use crate::renderer::Renderer;
use crate::ui_commands::UiCommand;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

const EXTRA_LIVE_FRAMES: usize = 10;

fn handle_new_grid_size(new_size: LogicalSize, renderer: &Renderer, command_channel: &mut UnboundedSender<UiCommand>) {
    if new_size.width > 0.0 && new_size.height > 0.0 {
        let new_width = ((new_size.width + 1.0) as f32 / renderer.font_width) as u64;
        let new_height = ((new_size.height + 1.0) as f32 / renderer.font_height) as u64;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        command_channel.send(UiCommand::Resize { width: new_width as i64, height: new_height as i64 });
    }
}

pub fn ui_loop(editor: Arc<Mutex<Editor>>, mut command_channel: UnboundedSender<UiCommand>, initial_size: (u64, u64)) {
    let mut renderer = Renderer::new(editor.clone());
    let event_loop = EventLoop::<()>::with_user_event();

    let (width, height) = initial_size;
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

    let mut mouse_down = false;
    let mut mouse_pos = (0, 0);

    icu::init();

    {
        let mut editor = editor.lock().unwrap();
        editor.window = Some(window.clone());
    }

    let mut live_frames = 0;
    let mut frame_start = Instant::now();
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
                handle_new_grid_size(new_size, &renderer, &mut command_channel)
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
                    .map(|keybinding_string| command_channel.send(keybinding_string));
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
                    command_channel.send(UiCommand::Drag(grid_x, grid_y));
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
                command_channel.send(UiCommand::MouseButton { action: input_type.to_string(), position: (grid_x, grid_y) });
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
                    let (grid_y, grid_x) = mouse_pos;
                    command_channel.send(UiCommand::Scroll { direction: input_type.to_string(), position: (grid_x, grid_y) });
                }

                let horizontal_input_type = if horizontal > 0.0 {
                    Some("right")
                } else if horizontal < 0.0 {
                    Some("left")
                } else {
                    None
                };

                if let Some(input_type) = horizontal_input_type {
                    let (grid_y, grid_x) = mouse_pos;
                    command_channel.send(UiCommand::Scroll { direction: input_type.to_string(), position: (grid_x, grid_y) });
                }
            }

            Event::RedrawRequested { .. }  => {
                frame_start = Instant::now();
                if let Err(e) = skulpin_renderer.draw(&window.clone(), |canvas, coordinate_system_helper| {
                    let draw_result = renderer.draw(canvas, coordinate_system_helper);
                    if draw_result.is_animating {
                        live_frames = EXTRA_LIVE_FRAMES;
                    } else {
                        if live_frames > 0 {
                            live_frames = live_frames - 1;
                        }
                    }

                    if draw_result.font_changed {
                        handle_new_grid_size(window.inner_size(), &renderer, &mut command_channel)
                    }

                    if live_frames > 0 {
                        *control_flow = ControlFlow::WaitUntil(frame_start + Duration::from_secs_f32(1.0 / 60.0));
                    } else {
                        *control_flow = ControlFlow::Wait;
                    }
                }) {
                    println!("Error during draw: {:?}", e);
                    *control_flow = ControlFlow::Exit
                }
            },

            _ => {}
        }
    })
}
