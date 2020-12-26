/// The keyboard modifiers associated with a keystroke
#[derive(Debug, Copy, Clone)]
pub struct Modifiers {
    /// Shift key
    pub shift: bool,

    /// Control key
    pub control: bool,

    /// Alt on Windows, option on Mac
    pub meta: bool,

    /// Windows key on PC, command key on Mac
    pub logo: bool,
}
