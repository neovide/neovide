use super::atlas::Atlas;
use super::font_loader::FontPair;
use log::trace;
use rayon::ThreadPoolBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use webrender_api::FontKey;
use wr_glyph_rasterizer::{
    profiler::GlyphRasterizeProfiler, FontInstance, GlyphKey, GlyphRasterizer,
};

type CachedGlyphKey = (FontKey, GlyphKey);

pub struct GlyphCache {
    pub rasterizer: GlyphRasterizer,
    glyphs: HashMap<CachedGlyphKey, u32>,
    next_glyph_id: u32,
    pending_rasterize: Vec<u32>,
    atlas: Atlas,
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
            next_glyph_id: 0,
            pending_rasterize: Vec::new(),
            atlas,
        }
    }

    pub fn get_glyph(&mut self, font: &FontPair, id: GlyphKey) -> u32 {
        let key = (font.api_key, id);
        *self.glyphs.entry(key).or_insert_with(|| {
            let font_instance = FontInstance::from_base(font.base_instance.clone());
            self.rasterizer
                .request_glyphs(font_instance, &[id], |_| true);
            let glyph_id = self.next_glyph_id;
            self.pending_rasterize.push(glyph_id);
            self.next_glyph_id += 1;
            glyph_id
        })
    }

    pub fn process(&mut self) {
        self.rasterizer.resolve_glyphs(
            |job, _| {
                if let Ok(glyph) = job.result {
                    trace!("Glyph width {}, height {}", glyph.width, glyph.height);
                    self.atlas.add_glyph(&glyph)
                }
            },
            &mut Profiler,
        );
        self.pending_rasterize.clear();
    }
}
