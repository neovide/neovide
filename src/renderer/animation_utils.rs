use skulpin::skia_safe::Point;

#[allow(dead_code)]
pub fn ease_linear(t: f32) -> f32 {
    t
}

#[allow(dead_code)]
pub fn ease_in_quad(t: f32) -> f32 {
    t * t
}

#[allow(dead_code)]
pub fn ease_out_quad(t: f32) -> f32 {
    -t * (t - 2.0)
}

#[allow(dead_code)]
pub fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        let n = t * 2.0 - 1.0;
        -0.5 * (n * (n - 2.0) - 1.0)
    }
}

pub fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

pub fn ease(ease_func: fn(f32) -> f32, start: f32, end: f32, t: f32) -> f32 {
    lerp(start, end, ease_func(t))
}

pub fn ease_point(ease_func: fn(f32) -> f32, start: Point, end: Point, t: f32) -> Point {
    Point {
        x: ease(ease_func, start.x, end.x, t),
        y: ease(ease_func, start.y, end.y, t),
    }
}
