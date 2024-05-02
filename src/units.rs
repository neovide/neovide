use std::ops::{Div, Mul};

use euclid::{Box2D, Point2D, Size2D, Vector2D};

/// Coordinates in grid units (multiplies of the font size)
pub struct Grid;
/// Coordinates in pixel units
pub struct Pixel;

pub type GridVec<T> = Vector2D<T, Grid>;
pub type PixelVec<T> = Vector2D<T, Pixel>;

pub type GridSize<T> = Size2D<T, Grid>;
pub type PixelSize<T> = Size2D<T, Pixel>;

pub type GridPos<T> = Point2D<T, Grid>;
pub type PixelPos<T> = Point2D<T, Pixel>;

pub type GridRect<T> = Box2D<T, Grid>;
pub type PixelRect<T> = Box2D<T, Pixel>;

// The Euclid library doesn't support two dimensional scales, so make our own simplified one
#[derive(Copy, Clone)]
pub struct GridScale(pub PixelSize<f32>);

impl GridScale {
    pub fn height(&self) -> f32 {
        self.0.height
    }

    pub fn width(&self) -> f32 {
        self.0.width
    }
}

impl Mul<GridScale> for GridVec<f32> {
    type Output = PixelVec<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.x * scale.0.width, self.y * scale.0.height)
    }
}

impl Mul<GridScale> for GridSize<f32> {
    type Output = PixelSize<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.width * scale.0.width, self.height * scale.0.height)
    }
}

impl Mul<GridScale> for GridPos<f32> {
    type Output = PixelPos<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.x * scale.0.width, self.y * scale.0.height)
    }
}

impl Mul<GridScale> for GridRect<f32> {
    type Output = PixelRect<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.min * scale, self.max * scale)
    }
}

impl Div<GridScale> for PixelVec<f32> {
    type Output = GridVec<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.x / scale.0.width, self.y / scale.0.height)
    }
}

impl Div<GridScale> for PixelSize<f32> {
    type Output = GridSize<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.width / scale.0.width, self.height / scale.0.height)
    }
}

impl Div<GridScale> for PixelPos<f32> {
    type Output = GridPos<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.x / scale.0.width, self.y / scale.0.height)
    }
}

impl Div<GridScale> for PixelRect<f32> {
    type Output = GridRect<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        Self::Output::new(self.min / scale, self.max / scale)
    }
}
