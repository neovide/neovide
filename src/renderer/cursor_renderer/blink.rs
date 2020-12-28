use std::time::{Duration, Instant};

use crate::editor::Cursor;
use crate::redraw_scheduler::REDRAW_SCHEDULER;

pub enum BlinkState {
    Waiting,
    On,
    Off,
}

pub struct BlinkStatus {
    state: BlinkState,
    last_transition: Instant,
    previous_cursor: Option<Cursor>,
}

impl BlinkStatus {
    pub fn new() -> BlinkStatus {
        BlinkStatus {
            state: BlinkState::Waiting,
            last_transition: Instant::now(),
            previous_cursor: None,
        }
    }

    pub fn update_status(&mut self, new_cursor: &Cursor) -> bool {
        if self.previous_cursor.is_none() || new_cursor != self.previous_cursor.as_ref().unwrap() {
            self.previous_cursor = Some(new_cursor.clone());
            self.last_transition = Instant::now();
            if new_cursor.blinkwait.is_some() && new_cursor.blinkwait != Some(0) {
                self.state = BlinkState::Waiting;
            } else {
                self.state = BlinkState::On;
            }
        }

        if new_cursor.blinkwait == Some(0)
            || new_cursor.blinkoff == Some(0)
            || new_cursor.blinkon == Some(0)
        {
            return true;
        }

        let delay = match self.state {
            BlinkState::Waiting => new_cursor.blinkwait,
            BlinkState::Off => new_cursor.blinkoff,
            BlinkState::On => new_cursor.blinkon,
        }
        .filter(|millis| *millis > 0)
        .map(Duration::from_millis);

        if delay
            .map(|delay| self.last_transition + delay < Instant::now())
            .unwrap_or(false)
        {
            self.state = match self.state {
                BlinkState::Waiting => BlinkState::On,
                BlinkState::On => BlinkState::Off,
                BlinkState::Off => BlinkState::On,
            };
            self.last_transition = Instant::now();
        }

        let scheduled_frame = (match self.state {
            BlinkState::Waiting => new_cursor.blinkwait,
            BlinkState::Off => new_cursor.blinkoff,
            BlinkState::On => new_cursor.blinkon,
        })
        .map(|delay| self.last_transition + Duration::from_millis(delay));

        if let Some(scheduled_frame) = scheduled_frame {
            REDRAW_SCHEDULER.schedule(scheduled_frame);
        }

        match self.state {
            BlinkState::Waiting | BlinkState::Off => false,
            BlinkState::On => true,
        }
    }
}
