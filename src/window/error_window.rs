use skia_safe::{
    textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle},
    Color, Color4f, FontMgr, Paint, Point,
};
#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use crate::{
    cmd_line::SRGB_DEFAULT,
    renderer::{build_context, build_window, GlWindow, WindowedContext},
    window::{load_icon, SkiaRenderer, UserEvent},
};

pub fn show_error_window(message: &str, event_loop: EventLoop<UserEvent>) {
    let font_manager = FontMgr::new();
    let mut font_collection = FontCollection::new();
    font_collection.set_default_font_manager(Some(font_manager), None);

    let srgb = SRGB_DEFAULT == "1";
    let vsync = true;
    let window = create_window(&event_loop);
    let context = build_context(window, srgb, vsync);
    let mut scale_factor = context.window().scale_factor();
    let mut skia_renderer = SkiaRenderer::new(&context);

    let _ = event_loop.run(move |e, window_target| match e {
        Event::LoopExiting => {}
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            window_target.exit();
        }
        Event::AboutToWait => {}
        Event::WindowEvent {
            event: WindowEvent::RedrawRequested,
            ..
        } => {
            render(message, &mut skia_renderer, &context, &font_collection, scale_factor);
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(..),
            ..
        } => {
            skia_renderer.resize(&context);
        }
        Event::WindowEvent {
            event: WindowEvent::ScaleFactorChanged { scale_factor: new_scale_factor, .. },
            ..
        } => {
            scale_factor = new_scale_factor;
        }
        _ => {}
    });
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

fn render(
    msg: &str,
    renderer: &mut SkiaRenderer,
    context: &WindowedContext,
    font_collection: &FontCollection,
    scale: f64,
) {
    let canvas = renderer.canvas();
    let text_color: Color4f = Color::from_rgb(255, 255, 255).into();
    let background_color: Color4f = Color::from_rgb(0, 0, 0).into();
    let font_size = scale * 12.0 * 96.0 / 72.0;

    canvas.clear(background_color);

    let text_paint = Paint::new(text_color, None);

    let mut text_style = TextStyle::new();
    text_style.set_font_families(&["monospace"]);
    text_style.set_foreground_color(&text_paint);
    text_style.set_font_size(font_size as f32);

    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_style(&text_style);
    paragraph_style.set_max_lines(100);

    let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection);
    paragraph_builder.push_style(&text_style);
    paragraph_builder.add_text(msg);
    paragraph_builder.pop();

    let mut paragraph = paragraph_builder.build();
    paragraph.layout(500.0);

    paragraph.paint(canvas, Point::new(0.0, 0.0));
    renderer.gr_context.flush_and_submit();
    context.window().pre_present_notify();
    context.swap_buffers().unwrap();
}
