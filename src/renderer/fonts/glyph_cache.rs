use super::atlas::{Atlas, AtlasCoordinate};
use super::font_loader::FontPair;
use log::trace;
use rayon::ThreadPoolBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use webrender_api::FontKey;
use wgpu::{Device, Queue};
use wr_glyph_rasterizer::{
    profiler::GlyphRasterizeProfiler, FontInstance, GlyphKey, GlyphRasterizer,
};

type CachedGlyphKey = (FontKey, GlyphKey);

pub struct GlyphCache {
    pub rasterizer: GlyphRasterizer,
    glyphs: HashMap<CachedGlyphKey, u32>,
    glyph_coordinates: Vec<Option<AtlasCoordinate>>,
    pub atlas: Atlas,
}

struct Profiler;

impl GlyphRasterizeProfiler for Profiler {
    fn start_time(&mut self) {}
    fn end_time(&mut self) -> f64 {
        0.
    }
    fn set(&mut self, _value: f64) {}
}

impl GlyphCache {
    pub fn new() -> Self {
        // Leave 2 cores free for other tasks, but always spawn at least 2 workers
        let num_threads = (num_cpus::get() - 2).max(2);
        let workers = {
            let worker = ThreadPoolBuilder::new()
                .thread_name(|idx| format!("WRWorker#{}", idx))
                .num_threads(num_threads)
                .build();
            Arc::new(worker.unwrap())
        };
        let rasterizer = GlyphRasterizer::new(workers, true);
        let atlas = Atlas::new();
        Self {
            rasterizer,
            glyphs: HashMap::new(),
            atlas,
            glyph_coordinates: Vec::new(),
        }
    }

    pub fn get_glyph(&mut self, font: &FontPair, id: GlyphKey) -> u32 {
        let key = (font.api_key, id);
        *self.glyphs.entry(key).or_insert_with(|| {
            let font_instance = FontInstance::from_base(font.base_instance.clone());
            self.rasterizer
                .request_glyphs(font_instance, &[id], |_| true);
            let glyph_id = self.glyph_coordinates.len();
            self.glyph_coordinates.push(None);
            glyph_id as u32
        })
    }

    pub fn process(&mut self, device: &Device, queue: &Queue) {
        self.rasterizer.resolve_glyphs(
            |job, _| {
                let key = (job.font.base.font_key, job.key);
                let glyph_id = self.glyphs.get(&key).unwrap();
                let glyph_coordinate = if let Ok(glyph) = job.result {
                    trace!("Glyph width {}, height {}", glyph.width, glyph.height);
                    Some(self.atlas.add_glyph(device, &glyph))
                } else {
                    None
                };
                self.glyph_coordinates[*glyph_id as usize] = glyph_coordinate;
            },
            &mut Profiler,
        );
        self.atlas.upload(queue);
    }

    pub fn get_glyph_coordinate(&self, glyph: u32) -> &Option<AtlasCoordinate> {
        &self.glyph_coordinates[glyph as usize]
    }
}
