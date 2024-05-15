use skia_safe::canvas::SrcRectConstraint;
use skia_safe::{Canvas, ISize, Image, Paint, Rect};
use std::hash::{Hash, Hasher};

use crate::profiling::tracy_zone;

// Custom hashing for Rect
fn hash_rect<H: Hasher>(rect: &Rect, state: &mut H) {
    (rect.left() as i32).hash(state);
    (rect.top() as i32).hash(state);
    (rect.right() as i32).hash(state);
    (rect.bottom() as i32).hash(state);
}

#[derive(Debug, Clone)]
struct CacheKey {
    window_rect: Rect,
    screen_rect: Rect,
    image_id: u32,
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.window_rect == other.window_rect
            && self.screen_rect == other.screen_rect
            && self.image_id == other.image_id
    }
}

impl Eq for CacheKey {}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_rect(&self.window_rect, state);
        hash_rect(&self.screen_rect, state);
        self.image_id.hash(state);
    }
}

pub struct CachedBackgroundRenderer {
    cached_image: Option<Image>, // Store the rendered image
    prev_key: Option<CacheKey>,  // Store the previous key
}

impl CachedBackgroundRenderer {
    pub fn new() -> Self {
        CachedBackgroundRenderer {
            cached_image: None,
            prev_key: None,
        }
    }

    pub fn draw_window_background_image(
        &mut self,
        paint: &Paint,
        canvas: &Canvas,
        image: &Image,
        window_rect: &Rect,
        screen_rect: &Rect,
    ) {
        tracy_zone!("draw_background_image");

        // Generate a unique key for the current state
        let key = CacheKey {
            window_rect: *window_rect,
            screen_rect: *screen_rect,
            image_id: image.unique_id(),
        };

        // Check if we can use the cached version
        if let Some(ref cached_image) = self.cached_image {
            if self.prev_key.as_ref() == Some(&key) {
                // Use cached rendering
                canvas.draw_image_rect(cached_image, None, window_rect, &Paint::default());
                return;
            }
        }

        // Create an off-screen surface
        let mut surface = skia_safe::surfaces::raster_n32_premul(ISize::new(
            screen_rect.width() as i32,
            screen_rect.height() as i32,
        ))
        .expect("Failed to create surface");
        let surface_canvas = surface.canvas();
        let image_width = image.width() as f32;
        let image_height = image.height() as f32;

        // Calculate the cover scale factor to ensure the image covers the whole screen
        let scale_x = screen_rect.width() / image_width;
        let scale_y = screen_rect.height() / image_height;
        let scale = scale_x.max(scale_y); // Use the larger scale factor to ensure the screen is covered

        // Calculate new dimensions after scaling
        let scaled_width = image_width * scale;
        let scaled_height = image_height * scale;

        // Calculate the offset to center the cropped area
        let offset_x = if scaled_width > screen_rect.width() {
            (scaled_width - screen_rect.width()) / 2.0 / scale
        } else {
            0.0
        };

        let offset_y = if scaled_height > screen_rect.height() {
            (scaled_height - screen_rect.height()) / 2.0 / scale
        } else {
            0.0
        };

        // Define the source rectangle to crop the image to fill the screen
        let src_x = (window_rect.left() - screen_rect.left()) / scale + offset_x;
        let src_y = (window_rect.top() - screen_rect.top()) / scale + offset_y;
        let src_width = window_rect.width() / scale;
        let src_height = window_rect.height() / scale;

        let src_rect = Rect::from_xywh(src_x, src_y, src_width, src_height);

        tracy_zone!("draw_background_image_uncached");
        surface_canvas.draw_image_rect(
            image,
            Some((&src_rect, SrcRectConstraint::Strict)),
            Rect::from_wh(screen_rect.width(), screen_rect.height()),
            paint,
        );

        // Extract the image from the surface
        let rendered_image = surface.image_snapshot();
        // Cache the rendered image and update the key
        self.cached_image = Some(rendered_image.clone());
        self.prev_key = Some(key.clone());

        canvas.draw_image_rect(&rendered_image, None, window_rect, paint);
    }
}
