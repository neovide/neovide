use std::sync::{Arc, Mutex};
use std::any::Any;

use skulpin::{CoordinateSystem, CoordinateSystemHelper, RendererBuilder};
use skulpin::skia_safe::{Canvas, Color4f, Font, FontStyle, Point, Paint, Rect, Typeface};
use skulpin::skia_safe::paint::Style;
use skulpin::skia_safe::matrix::ScaleToFit;
use skulpin::winit::dpi::LogicalSize;
use skulpin::winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::WindowBuilder;

use neovim_lib::NeovimApi;

use crate::editor::{DrawCommand, Editor, Colors};

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f32 = 14.0;
const FONT_WIDTH: f32 = 8.2;
const FONT_HEIGHT: f32 = 16.4;

// fn process_draw_commands(draw_commands: &Vec<DrawCommand>, default_colors: &Colors, piet: &mut Piet, font: &PietFont) {
// }

fn draw(
    editor: &Arc<Mutex<Editor>>,
    canvas: &mut Canvas,
    _coordinate_system_helper: &CoordinateSystemHelper
) {

    let typeface = Typeface::new(FONT_NAME, FontStyle::default()).expect("Could not load font file.");
    let font = Font::from_typeface(typeface, FONT_SIZE);
    let paint = Paint::new(Color4f::new(0.0, 1.0, 1.0, 1.0), None);

    // let shaper = Shaper::new(None);
    // if let Some((blob, _)) = shaper.shape_text_blob("This is a test ~==", font, false, 10000.0, Point::default()) {
    //     canvas.draw_text_blob(&blob, (50, 50), &paint);
    // }

    let (draw_commands, default_colors, cursor_pos) = {
        let editor = editor.lock().unwrap();
        (editor.build_draw_commands().clone(), editor.default_colors.clone(), editor.cursor_pos.clone())
    };

    canvas.clear(default_colors.background.clone().unwrap().to_color());

    canvas.draw_str("This is a test!", (50, 50), &font, &paint);

    for command in draw_commands {
        let x = command.col_start as f32 * FONT_WIDTH;
        let y = command.row as f32 * FONT_HEIGHT + FONT_HEIGHT;
        let top = y - FONT_HEIGHT * 0.8;
        let width = command.text.chars().count() as f32 * FONT_WIDTH;
        let height = FONT_HEIGHT;
        let region = Rect::new(x, top, x + width, top + height);
        let background_paint = Paint::new(command.style.colors.background.unwrap_or(default_colors.background.clone().unwrap()), None);
        canvas.draw_rect(region, &background_paint);
            
        let foreground_paint = Paint::new(command.style.colors.foreground.unwrap_or(default_colors.foreground.clone().unwrap()), None);
        canvas.draw_str(&command.text, (x, y), &font, &foreground_paint);
    }

    let (cursor_grid_x, cursor_grid_y) = cursor_pos;
    let cursor_x = cursor_grid_x as f32 * FONT_WIDTH;
    let cursor_width = FONT_WIDTH / 8.0;
    let cursor_y = cursor_grid_y as f32 * FONT_HEIGHT + FONT_HEIGHT * 0.2;
    let cursor_height = FONT_HEIGHT;
    let cursor = Rect::new(cursor_x, cursor_y, cursor_x + cursor_width, cursor_y + cursor_height);
    let cursor_paint = Paint::new(default_colors.foreground.unwrap(), None);
    canvas.draw_rect(cursor, &cursor_paint);
}

pub fn ui_loop(editor: Arc<Mutex<Editor>>) {
    let event_loop = EventLoop::<()>::with_user_event();
    let logical_size = LogicalSize::new((100.0 * FONT_WIDTH) as f64, (50.0 * FONT_HEIGHT) as f64);
    let visible_range = Rect {
        left: 0.0,
        right: logical_size.width as f32,
        top: 0.0,
        bottom: logical_size.height as f32
    };
    let scale_to_fit = ScaleToFit::Center;

    let window = WindowBuilder::new()
        .with_title("Neovide")
        .with_inner_size(logical_size)
        .build(&event_loop)
        .expect("Failed to create window");

    let mut renderer = RendererBuilder::new()
        .use_vulkan_debug_layer(true)
        .coordinate_system(CoordinateSystem::Logical)
        .build(&window)
        .expect("Failed to create renderer");

    // icu::init();

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
                editor.lock().unwrap().resize(
                    (new_size.width as f32 / FONT_WIDTH) as u16, 
                    (new_size.height as f32 / FONT_HEIGHT) as u16
                )
            },

            Event:: WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input: KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(keycode),
                        ..
                    },
                    ..
                },
                ..
            } => {
                let possible_string = match keycode {
                    VirtualKeyCode::A => Some("a"),
                    VirtualKeyCode::B => Some("b"),
                    VirtualKeyCode::C => Some("c"),
                    VirtualKeyCode::D => Some("d"),
                    VirtualKeyCode::E => Some("e"),
                    VirtualKeyCode::F => Some("f"),
                    VirtualKeyCode::G => Some("g"),
                    VirtualKeyCode::H => Some("h"),
                    VirtualKeyCode::I => Some("i"),
                    VirtualKeyCode::J => Some("j"),
                    VirtualKeyCode::K => Some("k"),
                    VirtualKeyCode::L => Some("l"),
                    VirtualKeyCode::M => Some("m"),
                    VirtualKeyCode::N => Some("n"),
                    VirtualKeyCode::O => Some("o"),
                    VirtualKeyCode::P => Some("p"),
                    VirtualKeyCode::Q => Some("q"),
                    VirtualKeyCode::R => Some("r"),
                    VirtualKeyCode::S => Some("s"),
                    VirtualKeyCode::T => Some("t"),
                    VirtualKeyCode::U => Some("u"),
                    VirtualKeyCode::V => Some("v"),
                    VirtualKeyCode::W => Some("w"),
                    VirtualKeyCode::X => Some("x"),
                    VirtualKeyCode::Y => Some("y"),
                    VirtualKeyCode::Z => Some("z"),
                    VirtualKeyCode::Escape => Some("<ESC>"),
                    VirtualKeyCode::Back => Some("<BS>"),
                    VirtualKeyCode::Space => Some("<Space>"),
                    VirtualKeyCode::Return => Some("<Enter>"),
                    VirtualKeyCode::Up => Some("<Up>"),
                    VirtualKeyCode::Down => Some("<Down>"),
                    VirtualKeyCode::Left => Some("<Left>"),
                    VirtualKeyCode::Right => Some("<Right>"),
                    VirtualKeyCode::LShift => Some("<S>"),
                    VirtualKeyCode::RShift => Some("<S>"),
                    VirtualKeyCode::Key1 => Some("1"),
                    VirtualKeyCode::Key2 => Some("2"),
                    VirtualKeyCode::Key3 => Some("3"),
                    VirtualKeyCode::Key4 => Some("4"),
                    VirtualKeyCode::Key5 => Some("5"),
                    VirtualKeyCode::Key6 => Some("6"),
                    VirtualKeyCode::Key7 => Some("7"),
                    VirtualKeyCode::Key8 => Some("8"),
                    VirtualKeyCode::Key9 => Some("9"),
                    VirtualKeyCode::Key0 => Some("0"),
                    VirtualKeyCode::Insert => Some("<Insert>"),
                    VirtualKeyCode::Home => Some("<Home>"),
                    VirtualKeyCode::Delete => Some("<Delete>"),
                    VirtualKeyCode::End => Some("<End>"),
                    _ => None
                };

                if let Some(string) = possible_string {
                    editor.lock().unwrap().nvim.input(string).expect("Input call failed...");
                }
            },

            Event::EventsCleared => {
                // Queue a RedrawRequested event.
                window.request_redraw();
            },

            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if let Err(e) = renderer.draw(&window, |canvas, coordinate_system_helper| {
                    draw(&editor, canvas, coordinate_system_helper);
                }) {
                    println!("Error during draw: {:?}", e);
                    *control_flow = ControlFlow::Exit
                }
            },

            _ => {}
        }
    })
}
