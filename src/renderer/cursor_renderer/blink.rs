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
    transition_time: Instant,
    current_cursor: Option<Cursor>,
}

fn is_static(cursor: &Cursor) -> bool {
    // The documentations says that if any state is zero there's no blinking
    cursor.blinkwait == Some(0)
        || cursor.blinkwait.is_none()
        || cursor.blinkoff == Some(0)
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
            self.state = BlinkState::On;
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

    pub fn should_render(&self) -> bool {
        match self.state {
            BlinkState::Off => false,
            BlinkState::On | BlinkState::Waiting => true,
        }
    }
}
