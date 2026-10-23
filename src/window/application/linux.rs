use winit::event_loop::EventLoopProxy;

use crate::window::EventPayload;

pub struct State;

impl State {
    pub fn new(_proxy: &EventLoopProxy<EventPayload>) -> Self { Self }
    pub fn resumed(&mut self, _proxy: &EventLoopProxy<EventPayload>) {}
    pub fn exiting(&mut self) {}
}
