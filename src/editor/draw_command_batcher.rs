use crate::{
    editor::DrawCommand,
    window::{EventPayload, RouteId},
};

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
        route_id: RouteId,
        proxy: &EventLoopProxy<EventPayload>,
    ) {
        log::info!("Set redraw {enabled}");
        if enabled && !self.enabled {
            for queued in self.queued.drain(..) {
                proxy
                    .send_event(EventPayload::for_route(queued.into(), route_id))
                    .ok();
            }
        }
        self.enabled = enabled;
    }

    pub fn send_batch(&mut self, route_id: RouteId, proxy: &EventLoopProxy<EventPayload>) {
        if self.enabled {
            proxy
                .send_event(EventPayload::for_route(
                    self.batch.split_off(0).into(),
                    route_id,
                ))
                .ok();
        } else {
            self.queued.push(self.batch.split_off(0));
        }
    }
}
