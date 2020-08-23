use cfg_if::cfg_if as define;
use font_kit::{
    family_handle::FamilyHandle,
    font::Font,
    metrics::Metrics,
    properties::{Properties, Stretch, Style, Weight},
    source::SystemSource,
};
use log::{trace, warn};
use lru::LruCache;
use skribo::{FontCollection, FontFamily, FontRef as SkriboFont, LayoutSession, TextStyle};
use skulpin::skia_safe::{Data, Font as SkiaFont, TextBlob, TextBlobBuilder, Typeface};

use std::collections::HashMap;
use std::iter;

use rand::Rng;

use super::font_options::FontOptions;

const STANDARD_CHARACTER_STRING: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

define! {
    if #[cfg(target_os = "windows")] {
        const SYSTEM_DEFAULT_FONT: &str = "Consolas";
        const SYSTEM_SYMBOL_FONT: &str = "Segoe UI Symbol";
        const SYSTEM_EMOJI_FONT: &str = "Segoe UI Emoji";
    } else if #[cfg(target_os = "linux")] {
        const SYSTEM_DEFAULT_FONT: &str = "Noto Sans Mono";
        const SYSTEM_SYMBOL_FONT: &str = "Noto Sans Mono";
        const SYSTEM_EMOJI_FONT: &str = "Noto Color Emoji";
    } else if #[cfg(target_os = "macos")] {
        const SYSTEM_DEFAULT_FONT: &str = "Menlo";
        const SYSTEM_SYMBOL_FONT: &str = "Apple Symbols";
        const SYSTEM_EMOJI_FONT: &str = "Apple Color Emoji";
    }
}

const EXTRA_SYMBOL_FONT: &str = "Extra Symbols.otf";
const MISSING_GLYPH_FONT: &str = "Missing Glyphs.otf";

#[cfg(any(feature = "embed-fonts", test))]
#[derive(RustEmbed)]
#[folder = "assets/fonts/"]
struct Asset;

const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(Clone)]
pub struct ExtendedFontFamily {
    pub fonts: Vec<SkriboFont>,
}

impl Default for ExtendedFontFamily {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtendedFontFamily {
    pub fn new() -> ExtendedFontFamily {
        ExtendedFontFamily { fonts: Vec::new() }
    }

    pub fn add_font(&mut self, font: SkriboFont) {
        self.fonts.push(font);
    }

    pub fn get(&self, props: Properties) -> Option<&Font> {
        if let Some(first_handle) = &self.fonts.first() {
            for handle in &self.fonts {
                let font = &handle.font;
                let properties = font.properties();

                if properties.weight == props.weight && properties.style == props.style {
                    return Some(&font);
                }
            }

            return Some(&first_handle.font);
        }

        None
    }
}

impl From<FamilyHandle> for ExtendedFontFamily {
    fn from(handle: FamilyHandle) -> Self {
        handle
            .fonts()
            .iter()
            .fold(ExtendedFontFamily::new(), |mut family, font| {
                if let Ok(font) = font.load() {
                    family.add_font(SkriboFont::new(font));
                }
                family
            })
    }
}

impl From<ExtendedFontFamily> for FontFamily {
    fn from(extended_font_family: ExtendedFontFamily) -> Self {
        extended_font_family
            .fonts
            .iter()
            .fold(FontFamily::new(), |mut new_family, font| {
                new_family.add_font(font.clone());
                new_family
            })
    }
}

pub struct FontLoader {
    cache: LruCache<String, ExtendedFontFamily>,
    source: SystemSource,
    random_font_name: Option<String>,
}

impl FontLoader {
    pub fn new() -> FontLoader {
        FontLoader {
            cache: LruCache::new(10),
            source: SystemSource::new(),
            random_font_name: None,
        }
    }

    fn get(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        self.cache.get(&String::from(font_name)).cloned()
    }

    #[cfg(any(feature = "embed-fonts", test))]
    fn load_from_asset(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        let mut family = ExtendedFontFamily::new();

        if let Some(font) = Asset::get(font_name)
            .and_then(|font_data| Font::from_bytes(font_data.to_vec().into(), 0).ok())
        {
            family.add_font(SkriboFont::new(font));
            self.cache.put(String::from(font_name), family);
            self.get(font_name)
        } else {
            None
        }
    }

    #[cfg(not(any(feature = "embed-fonts", test)))]
    fn load_from_asset(&self, font_name: &str) -> Option<ExtendedFontFamily> {
        warn!(
            "Tried to load {} from assets but build didn't include embed-fonts feature",
            font_name
        );
        None
    }

    fn load(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        let handle = match self.source.select_family_by_name(font_name) {
            Ok(it) => it,
            _ => return None,
        };

        if !handle.is_empty() {
            let family = ExtendedFontFamily::from(handle);
            self.cache.put(String::from(font_name), family);
            self.get(font_name)
        } else {
            None
        }
    }

    fn get_random_system_font_family(&mut self) -> Option<ExtendedFontFamily> {
        if let Some(font) = self.random_font_name.clone() {
            self.get(&font)
        } else {
            let font_names = self.source.all_families().expect("fonts exist");
            let n = rand::thread_rng().gen::<usize>() % font_names.len();
            let font_name = &font_names[n];
            self.random_font_name = Some(font_name.clone());
            self.load(&font_name)
        }
    }

    pub fn get_or_load(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        if let Some(cached) = self.get(font_name) {
            Some(cached)
        } else if let Some(loaded) = self.load(font_name) {
            Some(loaded)
        } else {
            self.load_from_asset(font_name)
        }
    }

    pub fn build_collection_by_font_name(
        &mut self,
        fallback_list: &[String],
        properties: Properties,
    ) -> FontCollection {
        let mut collection = FontCollection::new();

        let gui_fonts = fallback_list
            .iter()
            .map(|fallback_item| fallback_item.as_ref())
            .chain(iter::once(SYSTEM_DEFAULT_FONT));

        for font_name in gui_fonts {
            if let Some(family) = self.get_or_load(font_name) {
                if let Some(font) = family.get(properties) {
                    collection.add_family(FontFamily::new_from_font(font.clone()));
                }
            }
        }

        for font in &[
            SYSTEM_SYMBOL_FONT,
            SYSTEM_EMOJI_FONT,
            EXTRA_SYMBOL_FONT,
            MISSING_GLYPH_FONT,
        ] {
            if let Some(family) = self.get_or_load(font) {
                collection.add_family(FontFamily::from(family));
            }
        }

        if self.cache.is_empty() {
            let font_family = self.get_random_system_font_family();
            collection.add_family(FontFamily::from(font_family.expect("font family loaded")));
        }
        collection
    }
}

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

pub fn build_properties(bold: bool, italic: bool) -> Properties {
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if italic { Style::Italic } else { Style::Normal };
    Properties {
        weight,
        style,
        stretch: Stretch::NORMAL,
    }
}

struct FontSet {
    normal: FontCollection,
    bold: FontCollection,
    italic: FontCollection,
}

impl FontSet {
    fn new(fallback_list: &[String], loader: &mut FontLoader) -> FontSet {
        FontSet {
            normal: loader
                .build_collection_by_font_name(fallback_list, build_properties(false, false)),
            bold: loader
                .build_collection_by_font_name(fallback_list, build_properties(true, false)),
            italic: loader
                .build_collection_by_font_name(fallback_list, build_properties(false, true)),
        }
    }

    fn get(&self, bold: bool, italic: bool) -> &FontCollection {
        match (bold, italic) {
            (true, _) => &self.bold,
            (false, false) => &self.normal,
            (false, true) => &self.italic,
        }
    }
}

pub struct CachingShaper {
    pub options: FontOptions,
    font_set: FontSet,
    font_loader: FontLoader,
    font_cache: LruCache<String, SkiaFont>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
}

fn build_skia_font_from_skribo_font(skribo_font: &SkriboFont, base_size: f32) -> Option<SkiaFont> {
    let font_data = skribo_font.font.copy_font_data()?;
    let skia_data = Data::new_copy(&font_data[..]);
    let typeface = Typeface::from_data(skia_data, None)?;

    Some(SkiaFont::from_typeface(typeface, base_size))
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        let options = FontOptions::new(String::from(SYSTEM_DEFAULT_FONT), DEFAULT_FONT_SIZE);
        let mut loader = FontLoader::new();
        let font_set = FontSet::new(&options.fallback_list, &mut loader);

        CachingShaper {
            options,
            font_set,
            font_loader: loader,
            font_cache: LruCache::new(10),
            blob_cache: LruCache::new(10000),
        }
    }

    fn get_skia_font(&mut self, skribo_font: &SkriboFont) -> Option<&SkiaFont> {
        let font_name = skribo_font.font.postscript_name()?;
        if !self.font_cache.contains(&font_name) {
            let font = build_skia_font_from_skribo_font(skribo_font, self.options.size)?;
            self.font_cache.put(font_name.clone(), font);
        }

        self.font_cache.get(&font_name)
    }

    fn metrics(&self) -> Metrics {
        self.font_set
            .normal
            .itemize("a")
            .next()
            .expect("Cannot get font metrics")
            .1
            .font
            .metrics()
    }

    pub fn shape(&mut self, text: &str, bold: bool, italic: bool) -> Vec<TextBlob> {
        let style = TextStyle {
            size: self.options.size,
        };
        let session = LayoutSession::create(text, &style, &self.font_set.get(bold, italic));
        let metrics = self.metrics();
        let ascent = metrics.ascent * self.options.size / metrics.units_per_em as f32;
        let mut blobs = Vec::new();

        for layout_run in session.iter_all() {
            let skribo_font = layout_run.font();

            if let Some(skia_font) = self.get_skia_font(&skribo_font) {
                let mut blob_builder = TextBlobBuilder::new();
                let count = layout_run.glyphs().count();
                let (glyphs, positions) =
                    blob_builder.alloc_run_pos_h(&skia_font, count, ascent, None);

                for (i, glyph) in layout_run.glyphs().enumerate() {
                    glyphs[i] = glyph.glyph_id as u16;
                    positions[i] = glyph.offset.x();
                }

                blobs.push(blob_builder.make().unwrap());
            } else {
                warn!("Could not load skribo font");
            }
        }

        blobs
    }

    pub fn shape_cached(&mut self, text: &str, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let key = ShapeKey::new(text.to_string(), bold, italic);

        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text, bold, italic);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }

    pub fn update_font(&mut self, guifont_setting: &str) -> bool {
        let updated = self.options.update(guifont_setting);
        if updated {
            trace!("Font changed: {:?}", self.options);
            self.font_set = FontSet::new(&self.options.fallback_list, &mut self.font_loader);
            self.font_cache.clear();
            self.blob_cache.clear();
        }
        updated
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let metrics = self.metrics();
        let font_height =
            (metrics.ascent - metrics.descent) * self.options.size / metrics.units_per_em as f32;
        let style = TextStyle {
            size: self.options.size,
        };
        let session =
            LayoutSession::create(STANDARD_CHARACTER_STRING, &style, &self.font_set.normal);
        let layout_run = session.iter_all().next().unwrap();
        let glyph_offsets: Vec<f32> = layout_run.glyphs().map(|glyph| glyph.offset.x()).collect();
        let glyph_advances: Vec<f32> = glyph_offsets
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect();

        let mut amounts = HashMap::new();

        for advance in glyph_advances.iter() {
            amounts
                .entry(advance.to_string())
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }

        let (font_width, _) = amounts.into_iter().max_by_key(|(_, count)| *count).unwrap();
        let font_width = font_width.parse::<f32>().unwrap();

        (font_width, font_height)
    }

    pub fn underline_position(&mut self) -> f32 {
        let metrics = self.metrics();
        -metrics.underline_position * self.options.size / metrics.units_per_em as f32
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const PROPERTIES1: Properties = Properties {
        weight: Weight::NORMAL,
        style: Style::Normal,
        stretch: Stretch::NORMAL,
    };

    const PROPERTIES2: Properties = Properties {
        weight: Weight::BOLD,
        style: Style::Normal,
        stretch: Stretch::NORMAL,
    };

    const PROPERTIES3: Properties = Properties {
        weight: Weight::NORMAL,
        style: Style::Italic,
        stretch: Stretch::NORMAL,
    };

    const PROPERTIES4: Properties = Properties {
        weight: Weight::BOLD,
        style: Style::Italic,
        stretch: Stretch::NORMAL,
    };

    fn dummy_font() -> SkriboFont {
        SkriboFont::new(
            Asset::get(EXTRA_SYMBOL_FONT)
                .and_then(|font_data| Font::from_bytes(font_data.to_vec().into(), 0).ok())
                .unwrap(),
        )
    }

    #[test]
    fn test_build_properties() {
        assert_eq!(build_properties(false, false), PROPERTIES1);
        assert_eq!(build_properties(true, false), PROPERTIES2);
        assert_eq!(build_properties(false, true), PROPERTIES3);
        assert_eq!(build_properties(true, true), PROPERTIES4);
    }

    mod extended_font_family {
        use super::*;

        #[test]
        fn test_add_font() {
            let mut eft = ExtendedFontFamily::new();
            let font = dummy_font();
            eft.add_font(font.clone());
            assert_eq!(
                eft.fonts.first().unwrap().font.full_name(),
                font.font.full_name()
            );
        }

        #[test]
        fn test_get() {
            let mut eft = ExtendedFontFamily::new();
            assert!(eft.get(PROPERTIES1).is_none());

            let font = dummy_font();
            eft.fonts.push(font.clone());
            assert_eq!(
                eft.get(font.font.properties()).unwrap().full_name(),
                font.font.full_name()
            );
        }
    }

    mod font_loader {
        use super::*;

        #[test]
        fn test_load_from_asset() {
            let mut loader = FontLoader::new();

            let font_family = loader.load_from_asset("");
            assert!(font_family.is_none());

            let font = dummy_font();
            let mut eft = ExtendedFontFamily::new();
            eft.add_font(font.clone());
            let font_family = loader.load_from_asset(EXTRA_SYMBOL_FONT);
            let result = font_family.unwrap().fonts.first().unwrap().font.full_name();
            assert_eq!(&result, &eft.fonts.first().unwrap().font.full_name());

            assert_eq!(
                &result,
                &loader
                    .cache
                    .get(&EXTRA_SYMBOL_FONT.to_string())
                    .unwrap()
                    .fonts
                    .first()
                    .unwrap()
                    .font
                    .full_name()
            );
        }

        #[test]
        fn test_load() {
            let mut loader = FontLoader::new();
            let junk_text = "uhasiudhaiudshiaushd";
            let font_family = loader.load(junk_text);
            assert!(font_family.is_none());

            #[cfg(target_os = "linux")]
            const SYSTEM_DEFAULT_FONT: &str = "DejaVu Serif";

            let font_family = loader.load(SYSTEM_DEFAULT_FONT);
            let result = font_family.unwrap().fonts.first().unwrap().font.full_name();
            assert_eq!(
                &result,
                &loader
                    .cache
                    .get(&SYSTEM_DEFAULT_FONT.to_string())
                    .unwrap()
                    .fonts
                    .first()
                    .unwrap()
                    .font
                    .full_name()
            );
        }

        #[test]
        fn test_get_random_system_font() {
            let mut loader = FontLoader::new();

            let font_family = loader.get_random_system_font_family();
            let font_name = loader.random_font_name.unwrap();
            let result = font_family.unwrap().fonts.first().unwrap().font.full_name();
            assert_eq!(
                &result,
                &loader
                    .cache
                    .get(&font_name)
                    .unwrap()
                    .fonts
                    .first()
                    .unwrap()
                    .font
                    .full_name()
            );
        }
    }
}
