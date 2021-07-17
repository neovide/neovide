use super::font_loader::FontSelection;

const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(Clone, Debug)]
pub struct FontOptions {
    pub font_list: Vec<String>,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl FontOptions {
    pub fn parse(guifont_setting: &str) -> FontOptions {
        let mut font_list = Vec::new();
        let mut size = DEFAULT_FONT_SIZE;
        let mut bold = false;
        let mut italic = false;

        let mut parts = guifont_setting.split(':').filter(|part| !part.is_empty());

        if let Some(parts) = parts.next() {
            let parsed_font_list: Vec<String> = parts
                .split(',')
                .filter(|fallback| !fallback.is_empty())
                .map(|fallback| fallback.to_string())
                .collect();

            if !parsed_font_list.is_empty() {
                font_list = parsed_font_list;
            }
        }

        for part in parts {
            if part.starts_with('h') && part.len() > 1 {
                if let Ok(parsed_size) = part[1..].parse::<f32>() {
                    size = parsed_size
                }
            } else if part == "b" {
                bold = true;
            } else if part == "i" {
                italic = true;
            }
        }

        FontOptions {
            font_list,
            bold,
            italic,
            size: points_to_pixels(size),
        }
    }

    pub fn primary_font(&self) -> FontSelection {
        self.font_list
            .first()
            .map(|f| FontSelection::from(f))
            .unwrap_or(FontSelection::Default)
    }
}

impl Default for FontOptions {
    fn default() -> Self {
        FontOptions {
            font_list: Vec::new(),
            bold: false,
            italic: false,
            size: points_to_pixels(DEFAULT_FONT_SIZE),
        }
    }
}

impl PartialEq for FontOptions {
    fn eq(&self, other: &Self) -> bool {
        self.font_list == other.font_list
            && (self.size - other.size).abs() < std::f32::EPSILON
            && self.bold == other.bold
            && self.italic == other.italic
    }
}

fn points_to_pixels(value: f32) -> f32 {
    // Fonts in neovim are using points, not pixels.
    //
    // Skia docs is incorrectly stating it uses points, but uses pixels:
    // https://api.skia.org/classSkFont.html#a7e28a156a517d01bc608c14c761346bf
    // https://github.com/mono/SkiaSharp/issues/1147#issuecomment-587421201
    //
    // So, we need to convert points to pixels.
    //
    // In reality, this depends on DPI/PPI of monitor, but here we only care about converting
    // from points to pixels, so this is standard constant values.
    let pixels_per_inch = 96.0;
    let points_per_inch = 72.0;
    // On macos points == pixels
    #[cfg(target_os = "macos")]
    let points_per_inch = 96.0;

    let pixels_per_point = pixels_per_inch / points_per_inch;

    value * pixels_per_point
}
