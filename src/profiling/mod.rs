#[cfg(all(feature = "gpu_profiling", target_os = "windows"))]
use crate::platform::windows;
#[cfg(not(feature = "profiling"))]
mod profiling_disabled;
#[cfg(feature = "profiling")]
mod profiling_enabled;

#[cfg(all(feature = "gpu_profiling", target_os = "windows"))]
pub mod d3d {
    pub use crate::platform::windows::profiling::d3d::*;
}
#[cfg(feature = "gpu_profiling")]
pub mod opengl;

#[cfg(not(feature = "profiling"))]
pub use profiling_disabled::*;
#[cfg(feature = "profiling")]
pub use profiling_enabled::*;
