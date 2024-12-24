use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::{Div, Mul},
};

use glamour::{Box2, Point2, Scalar, Size2, Transform2, TransformMap, Unit, Vector2};

/// Coordinates in grid units (multiplies of the font size)
pub struct Grid<T> {
    phantom: PhantomData<T>,
}

impl<T: Scalar> Unit for Grid<T> {
    type Scalar = T;
}
/// Coordinates in pixel units
pub struct Pixel<T> {
    phantom: PhantomData<T>,
}
impl<T: Scalar> Unit for Pixel<T> {
    type Scalar = T;
}

pub type GridVec<T> = Vector2<Grid<T>>;
pub type PixelVec<T> = Vector2<Pixel<T>>;

pub type GridSize<T> = Size2<Grid<T>>;
pub type PixelSize<T> = Size2<Pixel<T>>;

pub type GridPos<T> = Point2<Grid<T>>;
pub type PixelPos<T> = Point2<Pixel<T>>;

pub type GridRect<T> = Box2<Grid<T>>;
pub type PixelRect<T> = Box2<Pixel<T>>;

pub fn to_skia_point(pos: PixelPos<f32>) -> skia_safe::Point {
    skia_safe::Point::new(pos.x, pos.y)
}

pub fn to_skia_rect(rect: &PixelRect<f32>) -> skia_safe::Rect {
    skia_safe::Rect {
        left: rect.min.x,
        top: rect.min.y,
        right: rect.max.x,
        bottom: rect.max.y,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GridScale {
    transform: Transform2<Grid<f32>, Pixel<f32>>,
}

impl GridScale {
    pub fn new(scale: PixelSize<f32>) -> Self {
        Self {
            transform: Transform2::from_scale(scale.as_vector().cast()),
        }
    }

    pub fn height(&self) -> f32 {
        self.transform.matrix.y_axis.y
    }

    pub fn width(&self) -> f32 {
        self.transform.matrix.x_axis.x
    }
}

impl<T: Scalar> Mul<GridScale> for GridVec<T> {
    type Output = PixelVec<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        let grid_vec: GridVec<f32> = self.try_cast().unwrap();
        scale.transform.map(grid_vec)
    }
}

impl<T: Scalar> Mul<GridScale> for GridSize<T> {
    type Output = PixelSize<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        (*self.as_vector() * scale).to_size()
    }
}

impl<T: Scalar> Mul<GridScale> for GridPos<T> {
    type Output = PixelPos<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        (*self.as_vector() * scale).to_point()
    }
}

impl<T: Scalar> Mul<GridScale> for GridRect<T> {
    type Output = PixelRect<f32>;

    #[inline]
    fn mul(self, scale: GridScale) -> Self::Output {
        PixelRect::new(
            (*self.min.as_vector() * scale).into(),
            (*self.max.as_vector() * scale).into(),
        )
    }
}

impl<T: Scalar> Div<GridScale> for PixelVec<T> {
    type Output = GridVec<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        let pixel_vec: PixelVec<f32> = self.try_cast().unwrap();
        scale.transform.inverse().map(pixel_vec)
    }
}

impl<T: Scalar> Div<GridScale> for PixelSize<T> {
    type Output = GridSize<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        (*self.as_vector() / scale).to_size()
    }
}

impl<T: Scalar> Div<GridScale> for PixelPos<T> {
    type Output = GridPos<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        (*self.as_vector() / scale).to_point()
    }
}

impl<T: Scalar> Div<GridScale> for PixelRect<T> {
    type Output = GridRect<f32>;

    #[inline]
    fn div(self, scale: GridScale) -> Self::Output {
        GridRect::new(
            (*self.min.as_vector() / scale).into(),
            (*self.max.as_vector() / scale).into(),
        )
    }
}
