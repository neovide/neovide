use lru::LruCache;
use skulpin::skia_safe::{Shaper, TextBlob, Font, Point};

pub struct CachingShaper {
    shaper: Shaper,
    cache: LruCache<String, TextBlob>
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            shaper: Shaper::new(None),
            cache: LruCache::new(1000)
        }
    }

    pub fn shape(&self, text: &str, font: &Font) -> TextBlob {
        let (blob, _) = self.shaper.shape_text_blob(text, font, true, 1000000.0, Point::default()).unwrap();
        blob
    }

    pub fn shape_cached(&mut self, text: String, font: &Font) -> &TextBlob {
        if !self.cache.contains(&text) {
            self.cache.put(text.clone(), self.shape(&text, &font));
        }

        self.cache.get(&text).unwrap()
    }
}
