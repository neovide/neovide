use std::time::Instant;

use crate::{renderer::GridRenderer, settings::ParseFromValue};
use neovide_derive::SettingGroup;
use skia_safe::{Canvas, Color4f, Paint, Rect};

/// Minimum active time required before the progress earns to be shown.
const REVEAL_DELAY: f32 = 0.1;

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
        Self { enabled: true, height: 3.0, animation_speed: 100.0, hide_delay: 0.5 }
    }
}

enum ProgressBarState {
    Idle,
    /// Progress has started, but the bar has not been shown yet.
    ///
    /// it could happen that a producer has a very short progress or just sent
    /// only a final completion update showing updates that could take a 0-100
    /// animation bar after its work has been finished or by an unknown behavior,
    /// adding actually only noise without giving useful user progress feedback.
    ///
    /// we keep the bar hidden until progress remains active based on a
    /// REVEAL_DELAY making it much more meaninful to show progress that
    /// has been earned to be shown instead.
    Pending {
        elapsed: f32,
    },
    Animating,
    Completing {
        completion_time: Instant,
    },
}

pub struct ProgressBar {
    current_percent: f32,
    target_percent: f32,
    id: Option<String>,
    state: ProgressBarState,
}

impl ProgressBar {
    pub fn new() -> Self {
        Self { current_percent: 0.0, target_percent: 0.0, id: None, state: ProgressBarState::Idle }
    }

    pub fn is_active(&self) -> bool {
        !matches!(self.state, ProgressBarState::Idle)
    }

    fn is_visible(&self) -> bool {
        matches!(self.state, ProgressBarState::Animating | ProgressBarState::Completing { .. })
    }

    fn reset(&mut self) {
        self.current_percent = 0.0;
        self.target_percent = 0.0;
        self.id = None;
        self.state = ProgressBarState::Idle;
    }

    fn set_target_percent(&mut self, percent: f32) {
        self.target_percent = percent.clamp(0.0, 100.0);
        if self.target_percent < self.current_percent {
            self.current_percent = self.target_percent;
        }
    }

    pub fn start(&mut self, id: Option<&str>, percent: f32) {
        if percent >= 100.0 {
            self.finish(id);
            return;
        }

        let is_idle = matches!(self.state, ProgressBarState::Idle);
        let is_completing = matches!(self.state, ProgressBarState::Completing { .. });

        self.id = id.map(ToOwned::to_owned);
        self.set_target_percent(percent);

        if is_idle {
            self.state = ProgressBarState::Pending { elapsed: 0.0 };
        } else if is_completing {
            self.state = ProgressBarState::Animating;
        }
    }

    pub fn finish(&mut self, id: Option<&str>) {
        let current_id = self.id.as_deref();
        let id_mismatch = id.zip(current_id).is_some_and(|(id, current_id)| id != current_id);

        if !self.is_active() || id_mismatch {
            return;
        }

        if matches!(&self.state, ProgressBarState::Pending { .. }) {
            self.reset();
            return;
        }

        self.id = id.or(current_id).map(ToOwned::to_owned);
        self.set_target_percent(100.0);
        self.state = ProgressBarState::Animating;
    }

    pub fn animate(&mut self, settings: &ProgressBarSettings, dt: f32) {
        match &mut self.state {
            ProgressBarState::Idle => {}
            ProgressBarState::Pending { elapsed } => {
                *elapsed += dt;
                if *elapsed >= REVEAL_DELAY {
                    self.state = ProgressBarState::Animating;
                }
            }
            ProgressBarState::Animating => {
                if self.current_percent < self.target_percent {
                    self.current_percent += settings.animation_speed * dt;
                    // here we clamp to the target to prevent overshooting.
                    self.current_percent = self.current_percent.min(self.target_percent);
                }
                if self.current_percent >= 100.0 {
                    self.state = ProgressBarState::Completing { completion_time: Instant::now() };
                }
            }
            ProgressBarState::Completing { completion_time } => {
                if completion_time.elapsed().as_secs_f32() > settings.hide_delay {
                    self.reset();
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
        if !self.is_visible() || !settings.enabled {
            return;
        }

        let width = grid_size.width as f32 * grid_renderer.grid_scale.width();
        let height = settings.height;
        let x = 0.0;
        let y = 0.0;
        let foreground_color = grid_renderer.default_style.colors.foreground.unwrap().to_color();

        let mut paint = Paint::new(Color4f::from(foreground_color), None);
        paint.set_anti_alias(true);

        let rect = Rect::from_xywh(x, y, width * (self.current_percent / 100.0), height);
        canvas.draw_rect(rect, &paint);
    }
}
