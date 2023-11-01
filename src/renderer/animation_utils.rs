use skia_safe::Point;

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

#[allow(dead_code)]
pub fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

#[allow(dead_code)]
pub fn ease_out_cubic(t: f32) -> f32 {
    let n = t - 1.0;
    n * n * n + 1.0
}

#[allow(dead_code)]
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let n = 2.0 * t;
    if n < 1.0 {
        0.5 * n * n * n
    } else {
        let n = n - 2.0;
        0.5 * (n * n * n + 2.0)
    }
}

#[allow(dead_code)]
pub fn ease_in_expo(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else {
        2.0f32.powf(10.0 * (t - 1.0))
    }
}

#[allow(dead_code)]
pub fn ease_out_expo(t: f32) -> f32 {
    if (t - 1.0).abs() < std::f32::EPSILON {
        1.0
    } else {
        1.0 - 2.0f32.powf(-10.0 * t)
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

pub struct CriticallyDampedSpringAnimation {
    pub position: f32,
    start_position: f32,
    velocity: f32,
    scroll_t: f32,
}

impl CriticallyDampedSpringAnimation {
    pub fn new() -> Self {
        Self {
            position: 0.0,
            start_position: 0.0,
            velocity: 0.0,
            scroll_t: 2.0,
        }
    }

    pub fn update(&mut self, dt: f32, animation_length: f32) -> bool {
        if self.scroll_t == 2.0 && self.position != 0.0 {
            self.start_position = self.position;
            self.scroll_t = 0.0;
        }

        if 1.0 - self.scroll_t < std::f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation.
            self.scroll_t = 2.0;
        } else {
            self.scroll_t = (self.scroll_t + dt / animation_length).min(1.0);
        }

        // For short animations use a standard ease function
        // This prevents precision errors, and division by zero
        if animation_length < 0.05 {
            self.position = ease(ease_out_expo, self.start_position, 0.0, self.scroll_t);
        } else {
            // Simulate critically damped spring, also known as a PD controller.
            // For more details of why this was chosen, see this:
            // https://gdcvault.com/play/1027059/Math-In-Game-Development-Summit
            // < 1 underdamped,  1 critically damped, > 1 overdamped
            let zeta = 1.0;
            // The omega is calculated so that the destination is reached with a 2% tolerance in
            // animation_length time.
            let omega = 4.0 / (zeta * animation_length);
            let k_p = omega * omega;
            let k_d = -2.0 * zeta * omega;
            let acc = -k_p * self.position + k_d * self.velocity;
            self.velocity += acc * dt;
            self.position += self.velocity * dt;
        }

        if self.position.abs() < 0.01 {
            self.reset();
            false
        } else {
            true
        }
    }

    pub fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
        self.scroll_t = 2.0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_lerp() {
        assert_eq!(lerp(1.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_ease_linear() {
        assert_eq!(ease(ease_linear, 1.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_ease_in_quad() {
        assert_eq!(ease(ease_in_quad, 1.00, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_ease_out_quad() {
        assert_eq!(ease(ease_out_quad, 1.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_ease_in_expo() {
        assert_eq!(ease(ease_in_expo, 1.0, 0.0, 1.0), 0.0);
        assert_eq!(ease(ease_in_expo, 1.0, 0.0, 0.0), 1.0);
    }

    #[test]
    fn test_ease_out_expo() {
        assert_eq!(ease(ease_out_expo, 1.0, 0.0, 1.0), 0.0);
        assert_eq!(ease(ease_out_expo, 1.0, 0.0, 1.1), 0.00048828125);
    }

    #[test]
    fn test_ease_in_out_quad() {
        assert_eq!(ease(ease_in_out_quad, 1.0, 0.0, 1.0), 0.0);
        assert_eq!(ease(ease_in_out_quad, 1.00, 0.0, 0.4), 0.67999995);
    }

    #[test]
    fn test_ease_in_cubic() {
        assert_eq!(ease(ease_in_cubic, 1.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_ease_out_cubic() {
        assert_eq!(ease(ease_out_cubic, 1.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_ease_in_out_cubic() {
        assert_eq!(ease(ease_in_out_cubic, 1.0, 0.0, 1.0), 0.0);
        assert_eq!(ease(ease_in_out_cubic, 1.0, 0.0, 0.25), 0.9375);
    }

    #[test]
    fn test_ease_point_linear() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        assert_eq!(ease_point(ease_linear, start, end, 1.0), end);
    }

    #[test]
    fn test_ease_point_in_quad() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        assert_eq!(ease_point(ease_in_quad, start, end, 1.0), end);
    }

    #[test]
    fn test_ease_point_out_quad() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        assert_eq!(ease_point(ease_out_quad, start, end, 1.0), end);
    }

    #[test]
    fn test_ease_point_in_out_quad() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        let expected = Point {
            x: 0.68000007,
            y: 0.68000007,
        };
        assert_eq!(ease_point(ease_in_out_quad, start, end, 1.0), end);
        assert_eq!(ease_point(ease_in_out_quad, start, end, 1.4), expected);
    }

    #[test]
    fn test_ease_point_in_cubic() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        assert_eq!(ease_point(ease_in_cubic, start, end, 1.0), end);
    }

    #[test]
    fn test_ease_point_out_cubic() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        assert_eq!(ease_point(ease_out_cubic, start, end, 1.0), end);
    }

    #[test]
    fn test_ease_point_in_out_cubic() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        let expected = Point {
            x: 0.0625,
            y: 0.0625,
        };
        assert_eq!(ease_point(ease_in_out_cubic, start, end, 1.0), end);
        assert_eq!(ease_point(ease_in_out_cubic, start, end, 0.25), expected);
    }

    #[test]
    fn test_ease_point_in_expo() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        assert_eq!(ease_point(ease_in_expo, start, end, 1.0), end);
        assert_eq!(ease_point(ease_in_expo, start, end, 0.0), start);
    }

    #[test]
    fn test_ease_point_out_expo() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 1.0, y: 1.0 };
        let expected = Point {
            x: 0.9995117,
            y: 0.9995117,
        };
        assert_eq!(ease_point(ease_out_expo, start, end, 1.0), end);
        assert_eq!(ease_point(ease_out_expo, start, end, 1.1), expected);
    }
}
