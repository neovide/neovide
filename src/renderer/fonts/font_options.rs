#[derive(Clone, Debug)]
pub struct FontOptions {
    guifont_setting: Option<String>,
    pub fallback_list: Vec<String>,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl FontOptions {
    pub fn parse(guifont_setting: &str, default_size: f32) -> Option<FontOptions> {
        let mut fallback_list = None;
        let mut size = default_size;
        let mut bold = false;
        let mut italic = false;

        let mut parts = guifont_setting.split(':').filter(|part| !part.is_empty());

        if let Some(parts) = parts.next() {
            let parsed_fallback_list: Vec<String> = parts
                .split(',')
                .filter(|fallback| !fallback.is_empty())
                .map(|fallback| fallback.to_string())
                .collect();

            if !parsed_fallback_list.is_empty() {
                fallback_list = Some(parsed_fallback_list);
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

        fallback_list.map(|fallback_list| FontOptions {
            guifont_setting: Some(guifont_setting.to_string()),
            fallback_list,
            bold,
            italic,
            size,
        })
    }
}

impl PartialEq for FontOptions {
    fn eq(&self, other: &Self) -> bool {
        if self.guifont_setting.is_some() && self.guifont_setting == other.guifont_setting {
            return true;
        }

        self.fallback_list == other.fallback_list
            && (self.size - other.size).abs() < std::f32::EPSILON
            && self.bold == other.bold
            && self.italic == other.italic
    }
}
