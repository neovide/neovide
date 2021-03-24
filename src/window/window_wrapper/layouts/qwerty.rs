use crate::window::keyboard::{unsupported_key, Token};
use glutin::keyboard::Key::{self, *};

/// Maps winit keyboard events to Vim tokens
pub fn handle_qwerty_layout(keycode: Key<'static>, shift: bool) -> Option<Token<'static>> {
    let special = |text| Some(Token::new(text, true, true));
    let special_ns = |text| Some(Token::new(text, true, false));
    let normal = |text| Some(Token::new(text, false, true));
    let partial = |text| Some(Token::new(text, false, false));
    match (keycode, shift) {
        (Character("<"), _) => special_ns("lt"),
        (Character(key), _) => partial(key.clone()),
        (ArrowRight, _) => special("Right"),
        (ArrowLeft, _) => special("Left"),
        (ArrowDown, _) => special("Down"),
        (ArrowUp, _) => special("Up"),
        (Backspace, _) => special("BS"),
        (Enter, _) => special("Enter"),
        (Escape, _) => special("Esc"),
        (Space, _) => normal(" "),
        (Tab, _) => special("Tab"),
        (Delete, _) => special("Delete"),
        (F1, _) => special("F1"),
        (F2, _) => special("F2"),
        (F3, _) => special("F3"),
        (F4, _) => special("F4"),
        (F5, _) => special("F5"),
        (F6, _) => special("F6"),
        (F7, _) => special("F7"),
        (F8, _) => special("F8"),
        (F9, _) => special("F9"),
        (F10, _) => special("F10"),
        (F11, _) => special("F11"),
        (F12, _) => special("F12"),
        (F13, _) => special("F13"),
        (F14, _) => special("F14"),
        (F15, _) => special("F15"),
        (F16, _) => special("F16"),
        (F17, _) => special("F17"),
        (F18, _) => special("F18"),
        (F19, _) => special("F19"),
        (F20, _) => special("F20"),
        (F21, _) => special("F21"),
        (F22, _) => special("F22"),
        (F23, _) => special("F23"),
        (F24, _) => special("F24"),
        (Insert, _) => special("Insert"),
        (Home, _) => special("Home"),
        (PageUp, _) => special("PageUp"),
        (End, _) => special("End"),
        (PageDown, _) => special("PageDown"),
        (keycode, _) => unsupported_key(keycode),
    }
}
