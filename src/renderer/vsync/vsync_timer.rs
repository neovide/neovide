use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use spin_sleep::SpinSleeper;

use crate::{settings::Settings, window::WindowSettings};

pub struct VSyncTimer {
    sleeper: SpinSleeper,
    last_refresh: Instant,
    settings: Arc<Settings>,
}

impl VSyncTimer {
    pub fn new(settings: Arc<Settings>) -> Self {
        VSyncTimer {
            sleeper: SpinSleeper::default(),
            last_refresh: Instant::now(),
            settings,
        }
    }

    pub fn wait_for_vsync(&mut self) {
        let refresh_duration = Duration::from_secs_f64(
            1.0 / self.settings.get::<WindowSettings>().refresh_rate as f64,
        );
        let next_refresh = self.last_refresh + refresh_duration;
        self.last_refresh = next_refresh;
        let sleep_duration = next_refresh.saturating_duration_since(Instant::now());
        if sleep_duration.as_nanos() > 0 {
            self.sleeper.sleep(sleep_duration);
        }
    }
}
