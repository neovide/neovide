use std::time::Instant;

use crate::{renderer::GridRenderer, settings::ParseFromValue};
use neovide_derive::SettingGroup;
use skia_safe::{Canvas, Color4f, Paint, Rect};

#[derive(Clone, SettingGroup)]
#[setting_prefix = "progress_bar"]
pub struct ProgressBarSettings {
    pub enabled: bool,
    pub height: f32,
    pub animation_speed: f32,
    pub hide_delay: f32,
}

impl Default for ProgressBarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            height: 3.0,
            animation_speed: 100.0,
            hide_delay: 0.5,
        }
    }
}

enum ProgressBarState {
    Idle,
    Animating,
    Completing { completion_time: Instant },
}

pub struct ProgressBar {
    current_percent: f32,
    target_percent: f32,
    state: ProgressBarState,
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            current_percent: 0.0,
            target_percent: 0.0,
            state: ProgressBarState::Idle,
        }
    }

    pub fn is_animating(&self) -> bool {
        !matches!(self.state, ProgressBarState::Idle)
    }

    pub fn start(&mut self, percent: f32) {
        self.target_percent = percent.clamp(0.0, 100.0);
        if self.target_percent < self.current_percent {
            self.current_percent = self.target_percent;
        }
        self.state = ProgressBarState::Animating;
    }

    pub fn animate(&mut self, settings: &ProgressBarSettings, dt: f32) {
        match &self.state {
            ProgressBarState::Idle => {}
            ProgressBarState::Animating => {
                if self.current_percent < self.target_percent {
                    self.current_percent += settings.animation_speed * dt;
                    // here we clamp to the target to prevent overshooting.
                    self.current_percent = self.current_percent.min(self.target_percent);
                }
                if self.current_percent >= 100.0 {
                    self.state = ProgressBarState::Completing {
                        completion_time: Instant::now(),
                    };
                }
            }
            ProgressBarState::Completing { completion_time } => {
                if completion_time.elapsed().as_secs_f32() > settings.hide_delay {
                    self.state = ProgressBarState::Idle;
                    // Reset percents for next time
                    self.current_percent = 0.0;
                    self.target_percent = 0.0;
                }
            }
        }
    }

    pub fn draw(
        &self,
        settings: &ProgressBarSettings,
        canvas: &Canvas,
        grid_renderer: &GridRenderer,
        grid_size: crate::units::GridSize<u32>,
    ) {
        if !self.is_animating() || !settings.enabled {
            return;
        }

        let width = grid_size.width as f32 * grid_renderer.grid_scale.width();
        let height = settings.height;
        let x = 0.0;
        let y = 0.0;
        let foreground_color = grid_renderer
            .default_style
            .colors
            .foreground
            .unwrap()
            .to_color();

        let mut paint = Paint::new(Color4f::from(foreground_color), None);
        paint.set_anti_alias(true);

        let rect = Rect::from_xywh(x, y, width * (self.current_percent / 100.0), height);
        canvas.draw_rect(rect, &paint);
    }
}
