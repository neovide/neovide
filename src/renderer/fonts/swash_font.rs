use swash::{CacheKey, FontRef};

pub struct SwashFont {
    data: Vec<u8>,
    offset: u32,
    pub key: CacheKey,
}

impl SwashFont {
    pub fn from_data(data: Vec<u8>, index: usize) -> Option<Self> {
        let font = FontRef::from_index(&data, index)?;
        let (offset, key) = (font.offset, font.key);
        Some(Self { data, offset, key })
    }

    pub fn as_ref(&self) -> FontRef {
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }
}
