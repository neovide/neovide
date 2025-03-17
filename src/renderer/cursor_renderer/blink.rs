use std::time::{Duration, Instant};

use crate::{editor::Cursor, window::ShouldRender};

#[derive(Debug, PartialEq)]
pub enum BlinkState {
    Waiting,
    On,
    Off,
}

pub struct BlinkStatus {
    state: BlinkState,
    /// When the cursor should change to the next [`BlinkState`]
    transition_time: Instant,
    current_cursor: Option<Cursor>,
}

fn is_static(cursor: &Cursor) -> bool {
    // The documentations says that if any state is zero there's no blinking
    // But blinkwait shuld be allowed to be 0, see https://github.com/neovim/neovim/issues/31687
    cursor.blinkoff == Some(0)
        || cursor.blinkoff.is_none()
        || cursor.blinkon == Some(0)
        || cursor.blinkon.is_none()
}

impl BlinkStatus {
    pub fn new() -> BlinkStatus {
        BlinkStatus {
            state: BlinkState::Waiting,
            transition_time: Instant::now(),
            current_cursor: None,
        }
    }

    fn get_delay(&self) -> Duration {
        let delay_ms = if let Some(c) = &self.current_cursor {
            match self.state {
                BlinkState::Waiting => c.blinkwait.unwrap_or(0),
                BlinkState::Off => c.blinkoff.unwrap_or(0),
                BlinkState::On => c.blinkon.unwrap_or(0),
            }
        } else {
            0
        };
        Duration::from_millis(delay_ms)
    }

    pub fn update_status(&mut self, new_cursor: &Cursor) -> ShouldRender {
        let now = Instant::now();
        if self.current_cursor.is_none() || new_cursor != self.current_cursor.as_ref().unwrap() {
            self.current_cursor = Some(new_cursor.clone());
            if new_cursor.blinkwait.is_some() && new_cursor.blinkwait != Some(0) {
                self.state = BlinkState::Waiting;
            } else {
                self.state = BlinkState::On;
            }
            self.transition_time = now + self.get_delay();
        }

        let current_cursor = self.current_cursor.as_ref().unwrap();

        if is_static(current_cursor) {
            self.state = BlinkState::Waiting;
            ShouldRender::Wait
        } else {
            if self.transition_time <= now {
                self.state = match self.state {
                    BlinkState::Waiting => BlinkState::On,
                    BlinkState::On => BlinkState::Off,
                    BlinkState::Off => BlinkState::On,
                };
                self.transition_time += self.get_delay();
                // In case we are lagging badly...
                if self.transition_time <= now {
                    self.transition_time = now + self.get_delay();
                }
                return ShouldRender::Immediately;
            }
            ShouldRender::Deadline(self.transition_time)
        }
    }

    /// Calculate the opacity the cursor should be drawn with when smooth cursor blink is enabled.
    /// `0.0` is fully transparent, `1.0` is fully opaque.
    pub fn opacity(&self) -> f32 {
        let now = Instant::now();
        if self.state == BlinkState::Waiting {
            return 1.0;
        }
        let total = self.get_delay().as_secs_f32();
        let remaining = (self.transition_time - now).as_secs_f32();
        match self.state {
            BlinkState::Waiting => 1.0,
            BlinkState::On => (remaining / total).clamp(0.0, 1.0),
            BlinkState::Off => (1.0 - remaining / total).clamp(0.0, 1.0),
        }
    }

    /// Whether or not the cursor is in a state that should be animated (only applicable when
    /// smooth blink is enabled).
    pub fn should_animate(&self) -> bool {
        match self.state {
            BlinkState::Waiting => false,
            BlinkState::On | BlinkState::Off => true,
        }
    }

    /// Whether or not the cursor should be drawn (only applicable when smooth blink is disabled).
    pub fn should_render(&self) -> bool {
        match self.state {
            BlinkState::Off => false,
            BlinkState::On | BlinkState::Waiting => true,
        }
    }
}
