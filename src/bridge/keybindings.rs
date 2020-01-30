use skulpin::winit::event::{KeyboardInput, ElementState, ModifiersState, VirtualKeyCode};

fn parse_keycode(keycode: VirtualKeyCode) -> Option<(&'static str, bool)> {
    match keycode {
        VirtualKeyCode::Key1 => Some(("1", false)),
        VirtualKeyCode::Key2 => Some(("2", false)),
        VirtualKeyCode::Key3 => Some(("3", false)),
        VirtualKeyCode::Key4 => Some(("4", false)),
        VirtualKeyCode::Key5 => Some(("5", false)),
        VirtualKeyCode::Key6 => Some(("6", false)),
        VirtualKeyCode::Key7 => Some(("7", false)),
        VirtualKeyCode::Key8 => Some(("8", false)),
        VirtualKeyCode::Key9 => Some(("9", false)),
        VirtualKeyCode::Key0 => Some(("0", false)),
        VirtualKeyCode::A => Some(("a", false)),
        VirtualKeyCode::B => Some(("b", false)),
        VirtualKeyCode::C => Some(("c", false)),
        VirtualKeyCode::D => Some(("d", false)),
        VirtualKeyCode::E => Some(("e", false)),
        VirtualKeyCode::F => Some(("f", false)),
        VirtualKeyCode::G => Some(("g", false)),
        VirtualKeyCode::H => Some(("h", false)),
        VirtualKeyCode::I => Some(("i", false)),
        VirtualKeyCode::J => Some(("j", false)),
        VirtualKeyCode::K => Some(("k", false)),
        VirtualKeyCode::L => Some(("l", false)),
        VirtualKeyCode::M => Some(("m", false)),
        VirtualKeyCode::N => Some(("n", false)),
        VirtualKeyCode::O => Some(("o", false)),
        VirtualKeyCode::P => Some(("p", false)),
        VirtualKeyCode::Q => Some(("q", false)),
        VirtualKeyCode::R => Some(("r", false)),
        VirtualKeyCode::S => Some(("s", false)),
        VirtualKeyCode::T => Some(("t", false)),
        VirtualKeyCode::U => Some(("u", false)),
        VirtualKeyCode::V => Some(("v", false)),
        VirtualKeyCode::W => Some(("w", false)),
        VirtualKeyCode::X => Some(("x", false)),
        VirtualKeyCode::Y => Some(("y", false)),
        VirtualKeyCode::Z => Some(("z", false)),
        VirtualKeyCode::Escape => Some(("ESC", true)),
        VirtualKeyCode::F1 => Some(("F1", true)),
        VirtualKeyCode::F2 => Some(("F2", true)),
        VirtualKeyCode::F3 => Some(("F3", true)),
        VirtualKeyCode::F4 => Some(("F4", true)),
        VirtualKeyCode::F5 => Some(("F5", true)),
        VirtualKeyCode::F6 => Some(("F6", true)),
        VirtualKeyCode::F7 => Some(("F7", true)),
        VirtualKeyCode::F8 => Some(("F8", true)),
        VirtualKeyCode::F9 => Some(("F9", true)),
        VirtualKeyCode::F10 => Some(("F10", true)),
        VirtualKeyCode::F11 => Some(("F11", true)),
        VirtualKeyCode::F12 => Some(("F12", true)),
        VirtualKeyCode::F13 => Some(("F13", true)),
        VirtualKeyCode::F14 => Some(("F14", true)),
        VirtualKeyCode::F15 => Some(("F15", true)),
        VirtualKeyCode::F16 => Some(("F16", true)),
        VirtualKeyCode::F17 => Some(("F17", true)),
        VirtualKeyCode::F18 => Some(("F18", true)),
        VirtualKeyCode::F19 => Some(("F19", true)),
        VirtualKeyCode::F20 => Some(("F20", true)),
        VirtualKeyCode::F21 => Some(("F21", true)),
        VirtualKeyCode::F22 => Some(("F22", true)),
        VirtualKeyCode::F23 => Some(("F23", true)),
        VirtualKeyCode::F24 => Some(("F24", true)),
        VirtualKeyCode::Insert => Some(("Insert", true)),
        VirtualKeyCode::Home => Some(("Home", true)),
        VirtualKeyCode::Delete => Some(("Delete", true)),
        VirtualKeyCode::End => Some(("End", true)),
        VirtualKeyCode::PageDown => Some(("PageDown", true)),
        VirtualKeyCode::PageUp => Some(("PageUp", true)),
        VirtualKeyCode::Left => Some(("Left", true)),
        VirtualKeyCode::Up => Some(("Up", true)),
        VirtualKeyCode::Right => Some(("Right", true)),
        VirtualKeyCode::Down => Some(("Down", true)),
        VirtualKeyCode::Back => Some(("BS", true)),
        VirtualKeyCode::Return => Some(("Enter", true)),
        VirtualKeyCode::Space => Some(("Space", true)),
        VirtualKeyCode::Caret => Some(("^", false)),
        VirtualKeyCode::Apostrophe => Some(("'", false)),
        VirtualKeyCode::Backslash => Some(("Bslash", true)),
        VirtualKeyCode::Colon => Some((":", false)),
        VirtualKeyCode::Comma => Some((",", false)),
        VirtualKeyCode::Equals => Some(("=", false)),
        VirtualKeyCode::Grave => Some(("`", false)),
        VirtualKeyCode::LBracket => Some(("[", false)),
        VirtualKeyCode::Minus => Some(("-", false)),
        VirtualKeyCode::Period => Some((".", false)),
        VirtualKeyCode::RBracket => Some(("]", false)),
        VirtualKeyCode::Semicolon => Some((";", false)),
        VirtualKeyCode::Slash => Some(("/", false)),
        VirtualKeyCode::Tab => Some(("Tab", true)),
        _ => None
    }
}

fn append_modifiers(modifiers: ModifiersState, keycode_text: &str, special: bool) -> String {
    let mut result = keycode_text.to_string();
    let mut special = special;

    if modifiers.shift() {
        result = match result.as_ref() {
            "1" => "!".to_string(),
            "2" => "@".to_string(),
            "3" => "#".to_string(),
            "4" => "$".to_string(),
            "5" => "%".to_string(),
            "6" => "^".to_string(),
            "7" => "&".to_string(),
            "8" => "*".to_string(),
            "9" => "(".to_string(),
            "0" => ")".to_string(),
            "'" => "\"".to_string(),
            "Bslash" => {
                special = false;
                "|".to_string()
            },
            "," => {
                special = true;
                "lt".to_string()
            },
            "=" => "+".to_string(),
            "`" => "~".to_string(),
            "[" => "{".to_string(),
            "-" => "_".to_string(),
            "." => ">".to_string(),
            "]" => "}".to_string(),
            ";" => ":".to_string(),
            "/" => "?".to_string(),
            other => {
                special = true;
                format!("S-{}", other)
            }
        };
    }
    if modifiers.ctrl() {
        special = true;
        result = format!("C-{}", result);
    }
    if modifiers.alt() {
        special = true;
        result = format!("M-{}", result);
    }
    if modifiers.logo() {
        special = true;
        result = format!("D-{}", result);
    }

    if special {
        result = format!("<{}>", result);
    }

    result
}

pub fn construct_keybinding_string(input: KeyboardInput) -> Option<String> {
    match input {
        KeyboardInput {
            state: ElementState::Pressed,
            virtual_keycode: Some(keycode),
            modifiers,
            ..
        } => {
            if let Some((keycode_text, special)) = parse_keycode(keycode) {
                Some(append_modifiers(modifiers, keycode_text, special))
            } else {
                None
            }
        },
        _ => None
    }
}
