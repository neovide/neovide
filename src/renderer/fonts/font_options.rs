#[derive(Clone, Debug)]
pub struct FontOptions {
    guifont_setting: Option<String>,
    pub fallback_list: Vec<String>,
    pub size: f32,
}

impl FontOptions {
    pub fn parse(guifont_setting: &str, default_size: f32) -> Option<FontOptions> {
        let mut fallback_list = None;
        let mut size = default_size;

        let mut parts = guifont_setting.split(':').filter(|part| !part.is_empty());

        if let Some(parts) = parts.next() {
            let parsed_fallback_list: Vec<String> = parts
                .split(',')
                .filter(|fallback| !fallback.is_empty())
                .map(|fallback| fallback.to_string())
                .collect();

            if parsed_fallback_list.len() > 0 {
                fallback_list = Some(parsed_fallback_list);
            }
        }

        for part in parts {
            if part.starts_with('h') && part.len() > 1 {
                if let Ok(parsed_size) = part[1..].parse::<f32>() {
                    size = parsed_size
                }
            }
        }

        fallback_list.map(|fallback_list| FontOptions {
            guifont_setting: Some(guifont_setting.to_string()),
            fallback_list,
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
    }
}
