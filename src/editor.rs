use std::sync::{Arc, Mutex};

use druid_shell::piet::PietTextLayout;

use neovim_lib::Neovim;

#[derive(Derivative, new)]
#[derivative(PartialEq)]
pub struct DrawCommand {
    pub text: String,
    pub row: u64,
    pub col_start: u64,
    #[new(default)]
    #[derivative(PartialEq="ignore")]
    pub layout: Mutex<Option<PietTextLayout>>
}

pub struct Editor {
    pub nvim: Neovim,
    pub draw_commands: Vec<Vec<Arc<Option<DrawCommand>>>>,
}

impl Editor {
    pub fn new(nvim: Neovim, width: usize, height: usize) -> Editor {
        let mut draw_commands = Vec::with_capacity(height);
        for _ in 0..width {
            draw_commands.push(vec![Arc::new(None); width]);
        }

        Editor {
            nvim,
            draw_commands
        }
    }

    pub fn draw(&mut self, command: DrawCommand) {
        let length = command.text.chars().count();
        let row_index = command.row as usize;
        let col_start = command.col_start as usize;
        let pointer = Arc::new(Some(command));

        let row = self.draw_commands.get_mut(row_index).expect("Draw command out of bounds");

        for x in 0..length {
            let pointer_index = x + col_start;
            if pointer_index < row.len() {
                row[pointer_index] = pointer.clone();
            }
        }
    }
}
