use winit::event_loop::EventLoopProxy;

use crate::window::EventPayload;

pub(super) struct State;

impl State {
    pub(super) fn new(_proxy: &EventLoopProxy<EventPayload>) -> Self { Self }
    pub(super) fn resumed(&mut self, _proxy: &EventLoopProxy<EventPayload>) {}
    pub(super) fn exiting(&mut self) {}
}
