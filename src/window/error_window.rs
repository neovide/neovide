use skia_safe::{
    canvas::SaveLayerRec,
    colors::{BLACK, WHITE},
    textlayout::{
        FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextHeightBehavior, TextIndex,
        TextStyle,
    },
    Color4f, FontMgr, Paint, Point, Rect, Size,
};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyEvent, Modifiers, WindowEvent},
    event_loop::{EventLoop, EventLoopWindowTarget},
    keyboard::Key,
    window::WindowBuilder,
};

use crate::{
    cmd_line::SRGB_DEFAULT,
    renderer::{build_context, build_window, GlWindow, WindowedContext},
    window::{load_icon, SkiaRenderer, UserEvent},
    clipboard,
};

const TEXT_COLOR: Color4f = WHITE;
const BACKGROUND_COLOR: Color4f = BLACK;
const FONT_SIZE: f32 = 12.0 * 96.0 / 72.0;
const PADDING: f32 = 10.0;
const MAX_LINES: i32 = 9999;

pub fn show_error_window(message: &str, event_loop: EventLoop<UserEvent>) {
    let mut error_window = ErrorWindow::new(message, &event_loop);
    error_window.run_event_loop(event_loop);
}

#[derive(Debug)]
enum Scroll {
    None,
    Line(i32),
    Page(f32),
    Start,
    End,
}

struct ErrorWindow<'a> {
    skia_renderer: SkiaRenderer,
    context: WindowedContext,
    font_collection: FontCollection,
    size: PhysicalSize<u32>,
    scale_factor: f64,
    paragraph: Paragraph,
    message: &'a str,
    scroll: Scroll,
    current_position: TextIndex,
    modifiers: Modifiers,
}

impl<'a> ErrorWindow<'a> {
    fn new(message: &'a str, event_loop: &EventLoop<UserEvent>) -> Self {
        let font_manager = FontMgr::new();
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(Some(font_manager), None);

        let srgb = SRGB_DEFAULT == "1";
        let vsync = true;
        let window = create_window(event_loop);
        let context = build_context(window, srgb, vsync);
        let scale_factor = context.window().scale_factor();
        let size = context.window().inner_size();
        let skia_renderer = SkiaRenderer::new(&context);
        let paragraph = create_paragraph(message, scale_factor as f32, &font_collection);
        let scroll = Scroll::None;
        let current_position = 0;
        let modifiers = Modifiers::default();

        Self {
            skia_renderer,
            context,
            font_collection,
            size,
            scale_factor,
            paragraph,
            message,
            scroll,
            current_position,
            modifiers,
        }
    }

    fn run_event_loop(&mut self, event_loop: EventLoop<UserEvent>) {
        let _ = event_loop.run(move |e, window_target| {
            if let Event::WindowEvent { event, .. } = e {
                self.handle_window_event(event, window_target);
            }
        });
    }

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
        window_target: &EventLoopWindowTarget<UserEvent>,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                window_target.exit();
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::Resized(size) => {
                self.size = size;
                self.skia_renderer.resize(&self.context);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                self.paragraph =
                    create_paragraph(self.message, scale_factor as f32, &self.font_collection);
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if self.handle_keyboard_input(event, window_target) {
                    self.context.window().request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
            _ => {}
        }
    }

    fn render(&mut self) {
        let size = Size::new(self.size.width as f32, self.size.height as f32);
        let padding_top_left = Point::new(PADDING, PADDING);
        let rect = Rect::from_point_and_size(
            padding_top_left,
            Size::new(size.width - 2.0 * PADDING, size.height - 2.0 * PADDING),
        );

        self.paragraph.layout(size.width - 2.0 * PADDING);
        let offset = self.handle_scrolling(rect.height() as f64);

        let canvas = self.skia_renderer.canvas();
        canvas.save();
        canvas.clear(BACKGROUND_COLOR);

        let save_layer_rec = SaveLayerRec::default().bounds(&rect);
        canvas.save_layer(&save_layer_rec);
        self.paragraph
            .paint(canvas, Point::new(PADDING, PADDING - offset as f32));
        canvas.restore();

        canvas.restore();

        self.skia_renderer.gr_context.flush_and_submit();
        self.context.window().pre_present_notify();
        self.context.swap_buffers().unwrap();
    }

    fn handle_keyboard_input(
        &mut self,
        event: KeyEvent,

        window_target: &EventLoopWindowTarget<UserEvent>,
    ) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let handled = if self.modifiers.state().control_key() {
            // Ctrl is pressed
            // Require e and y to be combined with ctrl, since y is copy
            match &event.logical_key {
                Key::Character(c) => match c.as_str() {
                    "e" => self.scroll_line(1),
                    "y" => self.scroll_line(-1),
                    "n" => self.scroll_line(1),
                    _ => false,
                },
                _ => false,
            }
        } else {
            // Ctrl is not pressed
            match &event.logical_key {
                Key::Character(c) => match c.as_str() {
                    "j" => self.scroll_line(1),
                    "g" => {
                        self.scroll = Scroll::Start;
                        true
                    }
                    "G" => {
                        self.scroll = Scroll::End;
                        true
                    }
                    "q" => {
                        window_target.exit();
                        true
                    }
                    "y" => {
                        let _ = clipboard::set_contents(self.message.to_string());
                        true
                    }
                    _ => false,
                },
                Key::ArrowDown => self.scroll_line(1),
                Key::ArrowUp => self.scroll_line(-1),
                Key::Space => self.scroll_page(1.0),
                Key::Enter => self.scroll_line(1),
                Key::Home => {
                    self.scroll = Scroll::Start;
                    true
                }
                Key::End => {
                    self.scroll = Scroll::End;
                    true
                }
                Key::Escape => {
                    window_target.exit();
                    true
                }
                _ => false,
            }
        };
        if handled {
            return true;
        }

        match event.logical_key {
            // NOTE: These work regardless of the ctrl state, mimicking "less"
            Key::Character(c) => match c.as_str() {
                "k" => self.scroll_line(-1),
                "d" => self.scroll_page(0.5),
                "u" => self.scroll_page(-0.5),
                "f" => self.scroll_page(1.0),
                "b" => self.scroll_page(-1.0),
                _ => false,
            },
            _ => false,
        }
    }

    fn scroll_line(&mut self, count: i32) -> bool {
        self.scroll = match self.scroll {
            Scroll::Line(prev_count) => Scroll::Line(prev_count + count),
            _ => Scroll::Line(count),
        };
        true
    }

    fn scroll_page(&mut self, amount: f32) -> bool {
        self.scroll = match self.scroll {
            Scroll::Page(prev_amount) => Scroll::Page(prev_amount + amount),
            _ => Scroll::Page(amount),
        };
        true
    }

    fn handle_scrolling(&mut self, allowed_height: f64) -> f64 {
        let metrics = self.paragraph.get_line_metrics();
        let mut current_line =
            metrics.partition_point(|v| v.start_index <= self.current_position) - 1;

        let lines_to_scroll = match self.scroll {
            Scroll::Line(lines) => lines,
            Scroll::Page(amount) => {
                let mut height = 0.0;
                let count = metrics[current_line..]
                    .iter()
                    .take_while(|line| {
                        height += line.height;
                        height <= allowed_height
                    })
                    .count() as f32;
                (count * amount).round() as i32
            }
            Scroll::Start => -MAX_LINES,
            Scroll::End => MAX_LINES,
            Scroll::None => 0,
        };
        current_line =
            (current_line as i32 + lines_to_scroll).clamp(0, metrics.len() as i32 - 1) as usize;
        let mut current_line_metrics = &metrics[current_line];

        self.scroll = Scroll::None;

        let mut offset = current_line_metrics.baseline - current_line_metrics.ascent;

        let last_line_metrix = metrics.last().unwrap();
        let last_line_pos = last_line_metrix.baseline + last_line_metrix.descent;
        while current_line > 0 && allowed_height > last_line_pos - offset {
            current_line -= 1;
            current_line_metrics = &metrics[current_line];
            offset = current_line_metrics.baseline - current_line_metrics.ascent;
        }

        self.current_position = current_line_metrics.start_index;
        current_line_metrics.baseline - current_line_metrics.ascent
    }
}

fn create_paragraph(
    message: &str,
    scale_factor: f32,
    font_collection: &FontCollection,
) -> Paragraph {
    let text_paint = Paint::new(TEXT_COLOR, None);

    let mut text_style = TextStyle::new();
    text_style.set_font_families(&["monospace"]);
    text_style.set_foreground_color(&text_paint);
    text_style.set_font_size(FONT_SIZE * scale_factor);

    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_style(&text_style);
    paragraph_style.set_max_lines(MAX_LINES as usize);
    paragraph_style.set_text_height_behavior(TextHeightBehavior::DisableAll);

    let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection);
    paragraph_builder.push_style(&text_style);
    paragraph_builder.add_text(message);
    paragraph_builder.pop();

    paragraph_builder.build()
}

fn create_window(event_loop: &EventLoop<UserEvent>) -> GlWindow {
    let icon = load_icon();

    let winit_window_builder = WindowBuilder::new()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_transparent(false)
        .with_visible(true)
        .with_decorations(true);

    #[cfg(target_os = "macos")]
    let winit_window_builder = winit_window_builder.with_accepts_first_mouse(false);

    build_window(winit_window_builder, event_loop)
}
