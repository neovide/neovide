#[cfg_attr(feature = "sdl2", path = "sdl2.rs")]
#[cfg_attr(feature = "winit", path = "winit.rs")]
mod window_wrapper;

pub use window_wrapper::*;
