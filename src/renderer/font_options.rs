#[derive(Clone, PartialEq, Debug)]
pub struct FontOptions {
    previous_guifont_setting: Option<String>,
    pub fallback_list: Vec<String>,
    pub size: f32,
}

impl FontOptions {
    pub fn new(name: String, size: f32) -> FontOptions {
        FontOptions {
            previous_guifont_setting: None,
            fallback_list: vec![name],
            size,
        }
    }

    pub fn update(self: &mut FontOptions, guifont_setting: &str) -> bool {
        if self.previous_guifont_setting.is_some()
            && guifont_setting == self.previous_guifont_setting.as_ref().unwrap()
        {
            return false;
        }
        self.previous_guifont_setting = Some(guifont_setting.to_string());

        let mut parts = guifont_setting.split(':').filter(|part| !part.is_empty());
        let mut updated = false;

        if let Some(parts) = parts.next() {
            let parsed_fallback_list: Vec<String> = parts
                .split(',')
                .filter(|fallback| !fallback.is_empty())
                .map(|fallback| fallback.to_string())
                .collect();

            if parsed_fallback_list.len() > 0 && self.fallback_list != parsed_fallback_list {
                self.fallback_list = parsed_fallback_list;
                updated = true;
            }
        }

        for part in parts {
            if part.starts_with('h') && part.len() > 1 {
                if let Some(size) = part[1..].parse::<f32>().ok() {
                    if (self.size - size).abs() > std::f32::EPSILON {
                        self.size = size;
                        updated = true;
                    }
                }
            }
        }

        updated
    }
}
