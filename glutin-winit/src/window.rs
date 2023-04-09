use glutin::context::PossiblyCurrentContext;
use glutin::surface::{
    GlSurface, ResizeableSurface, Surface, SurfaceAttributes, SurfaceAttributesBuilder,
    SurfaceTypeTrait, WindowSurface,
};
use raw_window_handle::HasRawWindowHandle;
use std::num::NonZeroU32;
use winit::window::Window;

/// [`Window`] extensions for working with [`glutin`] surfaces.
pub trait GlWindow {
    /// Build the surface attributes suitable to create a window surface.
    ///
    /// # Panics
    /// Panics if either window inner dimension is zero.
    ///
    /// # Example
    /// ```no_run
    /// use glutin_winit::GlWindow;
    /// # let winit_window: winit::window::Window = unimplemented!();
    ///
    /// let attrs = winit_window.build_surface_attributes(<_>::default());
    /// ```
    fn build_surface_attributes(
        &self,
        builder: SurfaceAttributesBuilder<WindowSurface>,
    ) -> SurfaceAttributes<WindowSurface>;

    /// Resize the surface to the window inner size.
    ///
    /// No-op if either window size is zero.
    ///
    /// # Example
    /// ```no_run
    /// use glutin_winit::GlWindow;
    /// # use glutin::surface::{Surface, WindowSurface};
    /// # let winit_window: winit::window::Window = unimplemented!();
    /// # let (gl_surface, gl_context): (Surface<WindowSurface>, _) = unimplemented!();
    ///
    /// winit_window.resize_surface(&gl_surface, &gl_context);
    /// ```
    fn resize_surface(
        &self,
        surface: &Surface<impl SurfaceTypeTrait + ResizeableSurface>,
        context: &PossiblyCurrentContext,
    );
}

impl GlWindow for Window {
    fn build_surface_attributes(
        &self,
        builder: SurfaceAttributesBuilder<WindowSurface>,
    ) -> SurfaceAttributes<WindowSurface> {
        let (w, h) = self
            .inner_size()
            .non_zero()
            .expect("invalid zero inner size");
        builder.build(self.raw_window_handle(), w, h)
    }

    fn resize_surface(
        &self,
        surface: &Surface<impl SurfaceTypeTrait + ResizeableSurface>,
        context: &PossiblyCurrentContext,
    ) {
        if let Some((w, h)) = self.inner_size().non_zero() {
            surface.resize(context, w, h)
        }
    }
}

/// [`winit::dpi::PhysicalSize<u32>`] non-zero extensions.
trait NonZeroU32PhysicalSize {
    /// Converts to non-zero `(width, height)`.
    fn non_zero(self) -> Option<(NonZeroU32, NonZeroU32)>;
}
impl NonZeroU32PhysicalSize for winit::dpi::PhysicalSize<u32> {
    fn non_zero(self) -> Option<(NonZeroU32, NonZeroU32)> {
        let w = NonZeroU32::new(self.width)?;
        let h = NonZeroU32::new(self.height)?;
        Some((w, h))
    }
}
