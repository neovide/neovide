use std::sync::{Arc, Mutex};
use std::time::Instant;

use lru::LruCache;

use skulpin::{CoordinateSystem, RendererBuilder};
use skulpin::skia_safe::{Canvas, colors, Font, FontStyle, Paint, Point, Rect, Shaper, TextBlob, Typeface};
use skulpin::skia_safe::icu;
use skulpin::winit::dpi::LogicalSize;
use skulpin::winit::event::{ElementState, Event, MouseScrollDelta, WindowEvent};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::{Window, WindowBuilder};

use neovim_lib::{Neovim, NeovimApi};

use crate::editor::{Editor, CursorType, Style, Colors};
use crate::keybindings::construct_keybinding_string;

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f32 = 14.0;

struct CachingShaper {
    shaper: Shaper,
    cache: LruCache<String, TextBlob>
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            shaper: Shaper::new(None),
            cache: LruCache::new(1000)
        }
    }

    pub fn shape(&self, text: &str, font: &Font) -> TextBlob {
        let (blob, _) = self.shaper.shape_text_blob(text, font, true, 1000000.0, Point::default()).unwrap();
        blob
    }

    pub fn shape_cached(&mut self, text: String, font: &Font) -> &TextBlob {
        if !self.cache.contains(&text) {
            self.cache.put(text.clone(), self.shape(&text, &font));
        }

        self.cache.get(&text).unwrap()
    }
}

struct FpsTracker {
    last_record_time: Instant,
    frame_count: usize,
    fps: usize
}

impl FpsTracker {
    pub fn new() -> FpsTracker {
        FpsTracker {
            fps: 0,
            last_record_time: Instant::now(),
            frame_count: 0
        }
    }

    pub fn record_frame(&mut self) {
        self.frame_count = self.frame_count + 1;
        let now = Instant::now();
        let time_since = (now - self.last_record_time).as_secs_f32();
        if time_since > 1.0 {
            self.fps = self.frame_count;
            self.last_record_time = now;
            self.frame_count = 0;
        }
    }
}

struct Renderer {
    editor: Arc<Mutex<Editor>>,

    title: String,

    paint: Paint,
    font: Font,
    shaper: CachingShaper,

    font_width: f32,
    font_height: f32,
    cursor_pos: (f32, f32),

    fps_tracker: FpsTracker
}

impl Renderer {
    pub fn new(editor: Arc<Mutex<Editor>>) -> Renderer {
        let title = "".to_string();

        let paint = Paint::new(colors::WHITE, None);
        let typeface = Typeface::new(FONT_NAME, FontStyle::default()).expect("Could not load font file.");
        let font = Font::from_typeface(typeface, FONT_SIZE);
        let shaper = CachingShaper::new();

        let (_, bounds) = font.measure_str("0", Some(&paint));
        let font_width = bounds.width();
        let (_, metrics) = font.metrics();
        let font_height = metrics.descent - metrics.ascent;
        let cursor_pos = (0.0, 0.0);

        let fps_tracker = FpsTracker::new();

        Renderer { editor, title, paint, font, shaper, font_width, font_height, cursor_pos, fps_tracker }
    }

    fn draw_text(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), style: &Style, default_colors: &Colors, update_cache: bool) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = text.chars().count() as f32 * self.font_width;
        let height = self.font_height;
        let region = Rect::new(x, y, x + width, y + height);
        self.paint.set_color(style.background(default_colors).to_color());
        canvas.draw_rect(region, &self.paint);

        if style.underline || style.undercurl {
            let (_, metrics) = self.font.metrics();
            let line_position = metrics.underline_position().unwrap();

            self.paint.set_color(style.special(&default_colors).to_color());
            canvas.draw_line((x, y - line_position + self.font_height), (x + width, y - line_position + self.font_height), &self.paint);
        }

        self.paint.set_color(style.foreground(&default_colors).to_color());
        let text = text.trim_end();
        if text.len() > 0 {
            let reference;
            let blob = if update_cache {
                self.shaper.shape_cached(text.to_string(), &self.font)
            } else {
                reference = self.shaper.shape(text, &self.font);
                &reference
            };
            canvas.draw_text_blob(blob, (x, y), &self.paint);
        }
    }

    pub fn draw(&mut self, window: &Window, canvas: &mut Canvas) {
        let (draw_commands, title, default_colors, (width, height), cursor_grid_pos, cursor_type, cursor_foreground, cursor_background, cursor_enabled) = {
            let editor = self.editor.lock().unwrap();
            (
                editor.build_draw_commands().clone(), 
                editor.title.clone(),
                editor.default_colors.clone(), 
                editor.size.clone(),
                editor.cursor_pos.clone(), 
                editor.cursor_type.clone(),
                editor.cursor_foreground(),
                editor.cursor_background(),
                editor.cursor_enabled
            )
        };

        canvas.clear(default_colors.background.clone().unwrap().to_color());

        for command in draw_commands {
            self.draw_text(canvas, &command.text, command.grid_position, &command.style, &default_colors, true);
        }

        self.fps_tracker.record_frame();
        self.draw_text(canvas, &self.fps_tracker.fps.to_string(), (width - 2, height - 1), &Style::new(default_colors.clone()), &default_colors, false);

        let (cursor_grid_x, cursor_grid_y) = cursor_grid_pos;
        let target_cursor_x = cursor_grid_x as f32 * self.font_width;
        let target_cursor_y = cursor_grid_y as f32 * self.font_height;
        let (previous_cursor_x, previous_cursor_y) = self.cursor_pos;

        let cursor_x = (target_cursor_x - previous_cursor_x) * 0.5 + previous_cursor_x;
        let cursor_y = (target_cursor_y - previous_cursor_y) * 0.5 + previous_cursor_y;

        self.cursor_pos = (cursor_x, cursor_y);
        if cursor_enabled {
            let cursor_width = match cursor_type {
                CursorType::Vertical => self.font_width / 8.0,
                CursorType::Horizontal | CursorType::Block => self.font_width
            };
            let cursor_height = match cursor_type {
                CursorType::Horizontal => self.font_width / 8.0,
                CursorType::Vertical | CursorType::Block => self.font_height
            };
            let cursor = Rect::new(cursor_x, cursor_y, cursor_x + cursor_width, cursor_y + cursor_height);
            self.paint.set_color(cursor_background.to_color());
            canvas.draw_rect(cursor, &self.paint);

            if let CursorType::Block = cursor_type {
                self.paint.set_color(cursor_foreground.to_color());
                let editor = self.editor.lock().unwrap();
                let character = editor.grid[cursor_grid_y as usize][cursor_grid_x as usize].clone()
                    .map(|(character, _)| character)
                    .unwrap_or(' ');
                canvas.draw_text_blob(
                    self.shaper.shape_cached(character.to_string(), &self.font), 
                    (cursor_x, cursor_y), &self.paint);
            }
        }

        if self.title != title {
            window.set_title(&title);
            self.title = title;
        }
    }
}

pub fn ui_loop(editor: Arc<Mutex<Editor>>, nvim: Neovim, initial_size: (u64, u64)) {
    let mut nvim = nvim;
    let mut renderer = Renderer::new(editor);
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
                    nvim.ui_try_resize(new_width as i64, new_height as i64).expect("Resize failed");
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
                    "up"
                } else {
                    "down"
                };
                let (grid_x, grid_y) = mouse_pos;
                nvim.input_mouse("wheel", input_type, "", 0, grid_y, grid_x).expect("Could not send mouse input");
            }

            Event::EventsCleared => {
                window.request_redraw();
            },

            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if let Err(e) = skulpin_renderer.draw(&window, |canvas, _coordinate_system_helper| {
                    renderer.draw(&window, canvas);
                }) {
                    println!("Error during draw: {:?}", e);
                    *control_flow = ControlFlow::Exit
                }
            },

            _ => {}
        }
    })
}
