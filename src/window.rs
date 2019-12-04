use std::sync::{Arc, Mutex};
use std::rc::Rc;
use std::any::Any;
use itertools::Itertools;
use druid_shell::{Application, WinHandler, WindowHandle, WinCtx, KeyEvent, WindowBuilder, RunLoop, HotKey, KeyCode};
use druid_shell::piet::{
    Piet, PietFont, Color, FontBuilder, RenderContext, Text, TextLayoutBuilder
};
use druid_shell::kurbo::Rect;
use neovim_lib::NeovimApi;

use crate::editor::{DrawCommand, Editor};

#[derive(new)]
struct WindowState {
    editor: Arc<Mutex<Editor>>,

    #[new(default)]
    pub size: (f64, f64),
    #[new(default)]
    pub handle: WindowHandle,
    #[new(default)]
    pub font: Option<PietFont>
}

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f64 = 14.0;
const FONT_WIDTH: f64 = 10.0;
const FONT_HEIGHT: f64 = 20.0;

fn process_draw_commands(draw_commands: &Vec<Vec<Arc<Option<DrawCommand>>>>, piet: &mut Piet, font: &PietFont) {
    for row in draw_commands {
        for possible_command in row.iter().dedup() {
            if let Some(ref command) = **possible_command {
                let x = command.col_start as f64 * FONT_WIDTH;
                let y = command.row as f64 * FONT_HEIGHT + FONT_HEIGHT;
                let top = y - FONT_HEIGHT * 0.8;
                let top_left = (x, top);
                let width = command.text.chars().count() as f64 * FONT_WIDTH;
                let height = FONT_HEIGHT;
                let bottom_right = (x + width, top + height);
                let region = Rect::from_points(top_left, bottom_right);
                piet.fill(region, &Color::rgb8(0x11, 0x11, 0x11));
                    
                let layout_ref = command.layout.lock().unwrap();

                let owned;
                let text_layout = if layout_ref.is_some() {
                    layout_ref.as_ref().unwrap()
                } else {
                    let piet_text = piet.text();
                    owned = piet_text.new_text_layout(&font, &command.text).build().unwrap();
                    &owned
                };
                piet.draw_text(&text_layout, (x, y), &Color::rgb8(0xff, 0xff, 0xff));
            }
        }
    }
}

impl WinHandler for WindowState {
    fn connect(&mut self, handle: &WindowHandle) {
        self.handle = handle.clone();
    }

    fn paint(&mut self, piet: &mut Piet, _ctx: &mut dyn WinCtx) -> bool {
        let text = piet.text();
        if self.font.is_none() {
            self.font = Some(text.new_font_by_name(FONT_NAME, FONT_SIZE).build().unwrap());
        }

        let font = self.font.as_ref().unwrap();

        piet.clear(Color::rgb8(0x00, 0x00, 0x00));
        let editor = self.editor.lock().unwrap();
        process_draw_commands(&editor.draw_commands, piet, font);
        true
    }

    fn key_down(&mut self, key_event: KeyEvent, _ctx: &mut dyn WinCtx) -> bool {
        let mut editor = self.editor.lock().unwrap();
        match key_event {
            k_e if k_e.key_code.is_printable() => {
                let incoming_text = k_e.unmod_text().unwrap_or("");
                editor.nvim.input(incoming_text).expect("Input call failed...");
            },
            k_e if (HotKey::new(None, KeyCode::Escape)).matches(k_e) => {
                editor.nvim.input("<Esc>").expect("Input call failed...");
            },
            k_e if (HotKey::new(None, KeyCode::Backspace)).matches(k_e) => {
                editor.nvim.input("<BS>").expect("Input call failed...");
            }
            _ => ()
        };
        true
    }

    fn size(&mut self, width: u32, height: u32, _ctx: &mut dyn WinCtx) {
        let dpi = self.handle.get_dpi();
        let dpi_scale = dpi as f64 / 96.0;
        let width_f = (width as f64) / dpi_scale;
        let height_f = (height as f64) / dpi_scale;
        self.size = (width_f, height_f);

        let mut editor = self.editor.lock().unwrap();
        editor.nvim.ui_try_resize((width_f / FONT_WIDTH) as i64, (height_f / FONT_HEIGHT) as i64).expect("Resize failed");
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

pub fn ui_loop(editor: Arc<Mutex<Editor>>) {
    let window_state = WindowState::new(editor);

    Application::init();
    let mut run_loop = RunLoop::new();
    let mut builder = WindowBuilder::new();
    builder.set_handler(Box::new(window_state));
    builder.set_title("Nvim editor");
    let window = builder.build().unwrap();
    window.show();
    run_loop.run();
}
