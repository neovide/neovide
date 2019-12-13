use std::sync::{Arc, Mutex};
use std::any::Any;

use skulpin::{CoordinateSystem, CoordinateSystemHelper, RendererBuilder};
use skulpin::skia_safe::{Canvas, colors, Color4f, Font, FontStyle, Point, Paint, Rect, Shaper, Typeface};
use skulpin::skia_safe::paint::Style;
use skulpin::skia_safe::matrix::ScaleToFit;
use skulpin::skia_safe::icu;
use skulpin::winit::dpi::{LogicalSize, LogicalPosition};
use skulpin::winit::event::{ElementState, Event, MouseScrollDelta, KeyboardInput, VirtualKeyCode, WindowEvent};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::WindowBuilder;

use neovim_lib::NeovimApi;

use crate::editor::{DrawCommand, Editor, Colors, CursorType};
use crate::keybindings::construct_keybinding_string;

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f32 = 14.0;

fn draw(
    editor: &Arc<Mutex<Editor>>,
    canvas: &mut Canvas,
    cursor_pos: &mut (f32, f32),
    shaper: &Shaper,
    paint: &mut Paint,
    font: &Font,
    font_width: f32,
    font_height: f32
) {
    let (draw_commands, default_colors, cursor_grid_pos, cursor_type, cursor_foreground, cursor_background, cursor_enabled) = {
        let editor = editor.lock().unwrap();
        (
            editor.build_draw_commands().clone(), 
            editor.default_colors.clone(), 
            editor.cursor_pos.clone(), 
            editor.cursor_type.clone(),
            editor.cursor_foreground(),
            editor.cursor_background(),
            editor.cursor_enabled
        )
    };

    canvas.clear(default_colors.background.clone().unwrap().to_color());

    for command in draw_commands {
        let x = command.col_start as f32 * font_width;
        let y = command.row as f32 * font_height + font_height - font_height * 0.2;
        let top = y - font_height * 0.8;
        let width = command.text.chars().count() as f32 * font_width;
        let height = font_height;
        let region = Rect::new(x, top, x + width, top + height);
        paint.set_color(command.style.background(&default_colors).to_color());
        canvas.draw_rect(region, &paint);

        if command.style.underline || command.style.undercurl {
            let (_, metrics) = font.metrics();
            let width = command.text.chars().count() as f32 * font_width;
            let underline_position = metrics.underline_position().unwrap();

            paint.set_color(command.style.special(&default_colors).to_color());
            canvas.draw_line((x, y + underline_position), (x + width, y + underline_position), &paint);
        }

        paint.set_color(command.style.foreground(&default_colors).to_color());
        let text = command.text.trim_end();
        canvas.draw_str(text, (x, y), &font, &paint);
        // if text.len() > 0 {
        //     if let Some((blob, _)) = shaper.shape_text_blob(&text, font, false, 10000.0, Point::default()) {
        //         canvas.draw_text_blob(&blob, (x, top), &paint);
        //     }
        // }
        
    }

    let (cursor_grid_x, cursor_grid_y) = cursor_grid_pos;
    let target_cursor_x = cursor_grid_x as f32 * font_width;
    let target_cursor_y = cursor_grid_y as f32 * font_height;
    let (previous_cursor_x, previous_cursor_y) = cursor_pos;

    let cursor_x = (target_cursor_x - *previous_cursor_x) * 0.5 + *previous_cursor_x;
    let cursor_y = (target_cursor_y - *previous_cursor_y) * 0.5 + *previous_cursor_y;

    *cursor_pos = (cursor_x, cursor_y);
    if cursor_enabled {
        let cursor_width = match cursor_type {
            CursorType::Vertical => font_width / 8.0,
            CursorType::Horizontal | CursorType::Block => font_width
        };
        let cursor_height = match cursor_type {
            CursorType::Horizontal => font_width / 8.0,
            CursorType::Vertical | CursorType::Block => font_height
        };
        let cursor = Rect::new(cursor_x, cursor_y, cursor_x + cursor_width, cursor_y + cursor_height);
        paint.set_color(cursor_background.to_color());
        canvas.draw_rect(cursor, &paint);

        if let CursorType::Block = cursor_type {
            paint.set_color(cursor_foreground.to_color());
            let editor = editor.lock().unwrap();
            let character = editor.grid[cursor_grid_y as usize][cursor_grid_x as usize].clone()
                .map(|(character, _)| character)
                .unwrap_or(' ');
            let text_y = cursor_y + font_height - font_height * 0.2;
            canvas.draw_str(character.to_string(), (cursor_x, text_y), &font, &paint);
        }
    }
}

pub fn ui_loop(editor: Arc<Mutex<Editor>>) {
    let shaper = Shaper::new(None);
    let typeface = Typeface::new(FONT_NAME, FontStyle::default()).expect("Could not load font file.");
    let font = Font::from_typeface(typeface, FONT_SIZE);
    let mut paint = Paint::new(colors::WHITE, None);

    let (width, bounds) = font.measure_str("0", Some(&paint));
    let font_width = width;
    let font_height = bounds.height() * 1.68;

    let event_loop = EventLoop::<()>::with_user_event();
    let logical_size = LogicalSize::new((100.0 * font_width) as f64, (50.0 * font_height) as f64);

    let window = WindowBuilder::new()
        .with_title("Neovide")
        .with_inner_size(logical_size)
        .build(&event_loop)
        .expect("Failed to create window");

    let mut renderer = RendererBuilder::new()
        .coordinate_system(CoordinateSystem::Logical)
        .prefer_mailbox_present_mode()
        .build(&window)
        .expect("Failed to create renderer");

    let mut cursor_pos = (0.0, 0.0);
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
                    editor.lock().unwrap().resize(
                        (new_size.width as f32 / font_width) as u64, 
                        (new_size.height as f32 / font_height) as u64
                    )
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
                    editor.lock().unwrap().nvim.input(&string).expect("Input call failed...");
                }
            },

            Event::WindowEvent {
                event: WindowEvent::CursorMoved {
                    position,
                    ..
                },
                ..
            } => {
                let grid_x = (position.x as f32 / font_width) as i64;
                let grid_y = (position.y as f32 / font_height) as i64;
                mouse_pos = (grid_x, grid_y);
                if mouse_down {
                    editor.lock().unwrap().nvim.input_mouse("left", "drag", "", 0, grid_y, grid_x);
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
                editor.lock().unwrap().nvim.input_mouse("left", input_type, "", 0, grid_y, grid_x);
            }

            Event::WindowEvent {
                event: WindowEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(delta, _),
                    ..
                },
                ..
            } => {
                let input_type = if delta > 0.0 {
                    "up"
                } else {
                    "down"
                };
                let (grid_x, grid_y) = mouse_pos;
                editor.lock().unwrap().nvim.input_mouse("wheel", input_type, "", 0, grid_y, grid_x);
            }

            Event::EventsCleared => {
                window.request_redraw();
            },

            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if let Err(e) = renderer.draw(&window, |canvas, _coordinate_system_helper| {
                    draw(&editor, canvas, &mut cursor_pos, &shaper, &mut paint, &font, font_width, font_height);
                }) {
                    println!("Error during draw: {:?}", e);
                    *control_flow = ControlFlow::Exit
                }
            },

            _ => {}
        }
    })
}
