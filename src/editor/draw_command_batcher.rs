use crate::{editor::DrawCommand, window::EventPayload};

use winit::event_loop::EventLoopProxy;

pub struct DrawCommandBatcher {
    batch: Vec<DrawCommand>,
    enabled: bool,
    queued: Vec<Vec<DrawCommand>>,
}

impl DrawCommandBatcher {
    pub fn new() -> DrawCommandBatcher {
        Self {
            batch: Vec::new(),
            enabled: true,
            queued: Vec::new(),
        }
    }

    pub fn queue(&mut self, draw_command: DrawCommand) {
        self.batch.push(draw_command);
    }

    pub fn set_enabled(
        &mut self,
        enabled: bool,

        winit_window_id: winit::window::WindowId,
        proxy: &EventLoopProxy<EventPayload>,
    ) {
        log::info!("Set redraw {enabled}");
        if enabled && !self.enabled {
            for queued in self.queued.drain(..) {
                proxy
                    .send_event(EventPayload::new(queued.into(), winit_window_id))
                    .ok();
            }
        }
        self.enabled = enabled;
    }

    pub fn send_batch(
        &mut self,
        winit_window_id: winit::window::WindowId,
        proxy: &EventLoopProxy<EventPayload>,
    ) {
        if self.enabled {
            proxy
                .send_event(EventPayload::new(
                    self.batch.split_off(0).into(),
                    winit_window_id,
                ))
                .ok();
        } else {
            self.queued.push(self.batch.split_off(0));
        }
    }
}
