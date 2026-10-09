#[cfg(not(feature = "profiling"))]
mod profiling_disabled;
#[cfg(feature = "profiling")]
mod profiling_enabled;
#[cfg(feature = "profiling")]
mod scroll;

#[cfg(all(feature = "gpu_profiling", target_os = "windows"))]
pub mod d3d;
#[cfg(feature = "gpu_profiling")]
pub mod opengl;

#[cfg(not(feature = "profiling"))]
pub use profiling_disabled::*;
#[cfg(feature = "profiling")]
pub use profiling_enabled::*;
#[cfg(feature = "profiling")]
pub use scroll::scroll_tick_count;
