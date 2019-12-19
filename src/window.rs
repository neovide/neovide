use std::sync::{Arc, Mutex};
use skulpin::{CoordinateSystem, RendererBuilder, PresentMode};
use skulpin::skia_safe::icu;
use skulpin::winit::dpi::LogicalSize;
use skulpin::winit::event::{ElementState, Event, MouseScrollDelta, WindowEvent};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::WindowBuilder;
use neovim_lib::{Neovim, NeovimApi};
use crate::editor::Editor;
use crate::keybindings::construct_keybinding_string;
use crate::renderer::Renderer;

pub fn ui_loop(editor: Arc<Mutex<Editor>>, nvim: Neovim, initial_size: (u64, u64)) {
    let mut nvim = nvim;
    let mut renderer = Renderer::new(editor.clone());
    let event_loop = EventLoop::<()>::with_user_event();

    let (width, height) = initial_size;
    let logical_size = LogicalSize::new(
        (width as f32 * renderer.font_width) as f64, 
        (height as f32 * renderer.font_height + 1.0) as f64
    );

    let window = WindowBuilder::new()
        .with_title("Neovide")
        .with_inner_size(logical_size)
        .build(&event_loop)
        .expect("Failed to create window");

    let mut skulpin_renderer = RendererBuilder::new()
        .coordinate_system(CoordinateSystem::Logical)
        .present_mode_priority(vec![PresentMode::Immediate])
        .build(&window)
        .expect("Failed to create renderer");

    let mut mouse_down = false;
    let mut mouse_pos = (0, 0);

    icu::init();

    event_loop.run(move |event, _window_target, control_flow| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,

            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                if new_size.width > 0.0 && new_size.height > 0.0 {
                    let new_width = ((new_size.width + 1.0) as f32 / renderer.font_width) as u64;
                    let new_height = ((new_size.height + 1.0) as f32 / renderer.font_height) as u64;
                    // Add 1 here to make sure resizing doesn't change the grid size on startup
                    nvim.ui_try_resize((new_width as i64).max(10), (new_height as i64).max(3)).expect("Resize failed");
                }
            },

            Event::WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input,
                    ..
                },
                ..
            } => {
                if let Some(string) = construct_keybinding_string(input) {
                    nvim.input(&string).expect("Input call failed...");
                }
            },

            Event::WindowEvent {
                event: WindowEvent::CursorMoved {
                    position,
                    ..
                },
                ..
            } => {
                let grid_x = (position.x as f32 / renderer.font_width) as i64;
                let grid_y = (position.y as f32 / renderer.font_height) as i64;
                mouse_pos = (grid_x, grid_y);
                if mouse_down {
                    nvim.input_mouse("left", "drag", "", 0, grid_y, grid_x).expect("Could not send mouse input");
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
                nvim.input_mouse("left", input_type, "", 0, grid_y, grid_x).expect("Could not send mouse input");
            }

            Event::WindowEvent {
                event: WindowEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(_, delta),
                    ..
                },
                ..
            } => {
                let input_type = if delta > 0.0 {
                    Some("up")
                } else if delta < 0.0 {
                    Some("down")
                } else {
                    None
                };

                if let Some(input_type) = input_type {
                    let (grid_x, grid_y) = mouse_pos;
                    nvim.input_mouse("wheel", input_type, "", 0, grid_y, grid_x).expect("Could not send mouse input");
                }
            }

            Event::EventsCleared => {
                window.request_redraw();
            },

            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if let Err(e) = skulpin_renderer.draw(&window, |canvas, coordinate_system_helper| {
                    renderer.draw(canvas, coordinate_system_helper);
                }) {
                    println!("Error during draw: {:?}", e);
                    *control_flow = ControlFlow::Exit
                }
            },

            _ => {}
        }
    })
}
