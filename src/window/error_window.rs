use std::sync::Arc;

use skia_safe::{
    canvas::{Canvas, SaveLayerRec},
    colors::{BLACK, WHITE},
    textlayout::{
        FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextHeightBehavior, TextIndex,
        TextStyle,
    },
    Color4f, FontMgr, Paint, Point, Rect, Size,
};
use strum::IntoEnumIterator;
use strum::{EnumCount, EnumIter};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, Modifiers, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::Window as WinitWindow,
};

use crate::{
    clipboard,
    cmd_line::SRGB_DEFAULT,
    renderer::{build_window_config, create_skia_renderer, SkiaRenderer, WindowConfig},
    settings::Settings,
    window::{load_icon, DrawCommand, ShouldRender, UserEvent, Window},
};

const TEXT_COLOR: Color4f = WHITE;
const BACKGROUND_COLOR: Color4f = BLACK;
const FONT_SIZE: f32 = 12.0 * 96.0 / 72.0;
const PADDING: f32 = 10.0;
const MAX_LINES: i32 = 9999;
const MIN_SIZE: PhysicalSize<u32> = PhysicalSize::new(500, 500);
const DEFAULT_SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 600);

#[derive(Debug)]
enum Scroll {
    None,
    Line(i32),
    Page(f32),
    Start,
    End,
}

#[derive(EnumCount, EnumIter)]
enum PossibleScrollDirection {
    None,
    Up,
    Down,
    Both,
}

struct Paragraphs {
    message: Paragraph,
    help_messages: [Paragraph; PossibleScrollDirection::COUNT],
}

pub struct ErrorWindow {
    message: String,
    skia_renderer: Box<dyn SkiaRenderer>,
    font_collection: FontCollection,
    size: PhysicalSize<u32>,
    scale_factor: f64,
    paragraphs: Paragraphs,
    scroll: Scroll,
    current_position: TextIndex,
    modifiers: Modifiers,
    mouse_scroll_accumulator: f32,
}

impl ErrorWindow {
    pub fn new(
        message: String,
        event_loop: &ActiveEventLoop,
        settings: Arc<Settings>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let message = message.trim_end().to_string();

        let font_manager = FontMgr::new();
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(Some(font_manager), None);

        let srgb = SRGB_DEFAULT == "1";
        let vsync = true;
        let window = create_window(event_loop, &settings);
        let skia_renderer = create_skia_renderer(window, srgb, vsync, settings, proxy);
        skia_renderer.window().set_visible(true);
        let scale_factor = skia_renderer.window().scale_factor();
        let size = skia_renderer.window().inner_size();
        let paragraphs = create_paragraphs(&message, scale_factor as f32, &font_collection);
        let scroll = Scroll::None;
        let current_position = 0;
        let modifiers = Modifiers::default();
        let mouse_scroll_accumulator = 0.0;

        Self {
            message,
            skia_renderer,
            font_collection,
            size,
            scale_factor,
            paragraphs,
            scroll,
            current_position,
            modifiers,
            mouse_scroll_accumulator,
        }
    }

    fn handle_window_event_impl(
        &mut self,
        event: WindowEvent,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        let mut should_render = true;
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.size = size;
                self.skia_renderer.resize();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                self.paragraphs =
                    create_paragraphs(&self.message, scale_factor as f32, &self.font_collection);
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if self.handle_keyboard_input(event, event_loop) {
                    self.skia_renderer.window().request_redraw();
                }
            }
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(_, y),
                ..
            } => {
                self.mouse_scroll_accumulator += y * 3.0;
                self.handle_mouse_scroll();
            }
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(delta),
                ..
            } => {
                if let Some(line_metrics) = self.paragraphs.message.get_line_metrics_at(0) {
                    let line_height = line_metrics.height;
                    self.mouse_scroll_accumulator += (delta.y / line_height) as f32;
                    self.handle_mouse_scroll();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
            _ => should_render = false,
        };
        should_render
    }

    fn render(&mut self) {
        let (message_rect, help_message_rect) = self.layout();

        let (offset, possible_scroll_direction) =
            self.handle_scrolling(message_rect.height() as f64);

        let canvas = self.skia_renderer.canvas();
        canvas.save();

        render_main_message(&self.paragraphs.message, canvas, &message_rect, offset);
        render_help_message(
            &self.paragraphs.help_messages[possible_scroll_direction as usize],
            canvas,
            &help_message_rect,
        );

        canvas.restore();

        self.skia_renderer.flush();
        self.skia_renderer.swap_buffers();
    }

    fn handle_keyboard_input(&mut self, event: KeyEvent, event_loop: &ActiveEventLoop) -> bool {
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
                        event_loop.exit();
                        true
                    }
                    "y" => {
                        let _ = clipboard::set_contents(self.message.clone(), "+");
                        true
                    }
                    _ => false,
                },
                Key::Named(named_key) => match named_key {
                    NamedKey::ArrowDown => self.scroll_line(1),
                    NamedKey::ArrowUp => self.scroll_line(-1),
                    NamedKey::Space => self.scroll_page(1.0),
                    NamedKey::Enter => self.scroll_line(1),
                    NamedKey::Home => {
                        self.scroll = Scroll::Start;
                        true
                    }
                    NamedKey::End => {
                        self.scroll = Scroll::End;
                        true
                    }
                    NamedKey::Escape => {
                        event_loop.exit();
                        true
                    }
                    _ => false,
                },
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

    fn handle_mouse_scroll(&mut self) {
        let tolerance: f32 = 1.0 / 1000000.0;
        let lines = (self.mouse_scroll_accumulator
            + self.mouse_scroll_accumulator.signum() * tolerance)
            .trunc() as i32;
        if lines != 0 {
            self.scroll_line(-lines);
            self.mouse_scroll_accumulator -= lines as f32;
            if self.mouse_scroll_accumulator.abs() < tolerance {
                self.mouse_scroll_accumulator = 0.0
            }
            self.skia_renderer.window().request_redraw();
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

    fn handle_scrolling(&mut self, allowed_height: f64) -> (f64, PossibleScrollDirection) {
        let metrics = self.paragraphs.message.get_line_metrics();
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
        while current_line > 0
            && (allowed_height - current_line_metrics.height) > last_line_pos - offset
        {
            current_line -= 1;
            current_line_metrics = &metrics[current_line];
            offset = current_line_metrics.baseline - current_line_metrics.ascent;
        }

        self.current_position = current_line_metrics.start_index;

        let can_scroll_up = current_line > 0;
        let can_scroll_down = last_line_pos - offset > allowed_height;

        let possible_scroll_direction = match (can_scroll_up, can_scroll_down) {
            (true, true) => PossibleScrollDirection::Both,
            (true, false) => PossibleScrollDirection::Up,
            (false, true) => PossibleScrollDirection::Down,
            (false, false) => PossibleScrollDirection::None,
        };

        (
            current_line_metrics.baseline - current_line_metrics.ascent,
            possible_scroll_direction,
        )
    }

    fn layout(&mut self) -> (Rect, Rect) {
        let window_size = Size::new(self.size.width as f32, self.size.height as f32);

        let message_width = window_size.width - 2.0 * PADDING;
        self.paragraphs.message.layout(message_width);
        for p in &mut self.paragraphs.help_messages {
            p.layout(message_width);
        }

        let help_message_height = self
            .paragraphs
            .help_messages
            .iter()
            .map(|p| p.height())
            .reduce(f32::max)
            .unwrap()
            + PADDING;

        let message_rect = Rect::from_point_and_size(
            Point::new(PADDING, PADDING),
            Size::new(
                window_size.width - 2.0 * PADDING,
                window_size.height - 2.0 * PADDING - help_message_height,
            ),
        );

        let help_message_rect = Rect::from_point_and_size(
            Point::new(0.0, message_rect.bottom + PADDING),
            Size::new(window_size.width, help_message_height),
        );

        (message_rect, help_message_rect)
    }
}

fn render_main_message(message: &Paragraph, canvas: &Canvas, rect: &Rect, offset: f64) {
    canvas.clear(BACKGROUND_COLOR);

    let save_layer_rec = SaveLayerRec::default().bounds(rect);
    canvas.save_layer(&save_layer_rec);
    message.paint(canvas, Point::new(PADDING, PADDING - offset as f32));
    canvas.restore();
}

fn render_help_message(message: &Paragraph, canvas: &Canvas, help_message_rect: &Rect) {
    let help_message_text_point =
        Point::new(help_message_rect.left + PADDING, help_message_rect.top);
    canvas.draw_rect(help_message_rect, &Paint::new(TEXT_COLOR, None));
    message.paint(canvas, help_message_text_point);
}

fn create_paragraphs(
    message: &str,
    scale_factor: f32,
    font_collection: &FontCollection,
) -> Paragraphs {
    let mut normal_text = TextStyle::new();
    normal_text.set_font_families(&["monospace"]);
    normal_text.set_foreground_paint(&Paint::new(TEXT_COLOR, None));
    normal_text.set_font_size(FONT_SIZE * scale_factor);

    let mut inverted_text = normal_text.clone();
    inverted_text.set_foreground_paint(&Paint::new(BACKGROUND_COLOR, None));

    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_style(&normal_text);
    paragraph_style.set_max_lines(MAX_LINES as usize);
    paragraph_style.set_text_height_behavior(TextHeightBehavior::DisableAll);

    let create_message = |message: &str, style| {
        let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection);
        paragraph_builder.push_style(style);
        paragraph_builder.add_text(message);
        paragraph_builder.pop();

        paragraph_builder.build()
    };

    let message_line = "quit (q), copy (y)";

    let help_messages = PossibleScrollDirection::iter()
        .map(|dir| match dir {
            PossibleScrollDirection::None => message_line.to_owned(),
            PossibleScrollDirection::Down => message_line.to_owned() + " ↓",
            PossibleScrollDirection::Up => message_line.to_owned() + " ↑",
            PossibleScrollDirection::Both => message_line.to_owned() + " ↑↓",
        })
        .map(|msg| create_message(&msg, &inverted_text))
        .collect::<Vec<Paragraph>>()
        .try_into()
        .unwrap();

    Paragraphs {
        message: create_message(message, &normal_text),
        help_messages,
    }
}

fn create_window(event_loop: &ActiveEventLoop, settings: &Settings) -> WindowConfig {
    let icon = load_icon();

    let window_attributes = WinitWindow::default_attributes()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_transparent(false)
        .with_visible(true)
        .with_decorations(true)
        .with_inner_size(DEFAULT_SIZE)
        .with_min_inner_size(MIN_SIZE);

    build_window_config(window_attributes, event_loop, settings)
}

impl Window for ErrorWindow {
    fn try_create_window(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _proxy: &EventLoopProxy<UserEvent>,
    ) {
    }
    fn prepare_no_update(&mut self) {}
    fn prepare_frame(&mut self) -> ShouldRender {
        ShouldRender::Wait
    }
    fn animate_frame(&mut self, _dt: f32) -> bool {
        false
    }
    fn draw_frame(&mut self, _dt: f32) {
        self.render();
    }

    fn handle_draw_commands(&mut self, _batch: Vec<DrawCommand>) {}

    fn handle_window_event(&mut self, event: WindowEvent, event_loop: &ActiveEventLoop) -> bool {
        self.handle_window_event_impl(event, event_loop)
    }

    fn handle_user_event(&mut self, _event: UserEvent, _settings: &Settings) {}

    fn valid(&self) -> bool {
        true
    }

    fn refresh_interval(&self) -> f32 {
        self.skia_renderer.refresh_interval()
    }

    fn request_redraw(&mut self) -> bool {
        self.skia_renderer.request_redraw()
    }
}
