use lru::LruCache;
use skulpin::skia_safe::{Shaper, TextBlob, Font, Point};

pub struct CachingShaper {
    shaper: Shaper,
    cache: LruCache<(String, u16), TextBlob>
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            shaper: Shaper::new(None),
            cache: LruCache::new(10000)
        }
    }

    pub fn shape(&self, text: &str, font: &Font) -> TextBlob {
        let (blob, _) = self.shaper.shape_text_blob(text, font, true, 1000000.0, Point::default()).unwrap();
        blob
    }

    pub fn shape_cached(&mut self, text: String, scale: u16, font: &Font) -> &TextBlob {
        let key = (text.clone(), scale);
        if !self.cache.contains(&key) {
            self.cache.put(key.clone(), self.shape(&text, &font));
        }

        self.cache.get(&key).unwrap()
    }

// Re-enable once fonts change
//     pub fn clear(&mut self) {
//         self.cache.clear();
//     }
}
