use crate::editor::DrawCommand;

pub struct DrawCommandBatcher {
    batch: Vec<DrawCommand>,
    enabled: bool,
    queued: Vec<Vec<DrawCommand>>,
}

impl DrawCommandBatcher {
    pub fn new() -> DrawCommandBatcher {
        Self { batch: Vec::new(), enabled: true, queued: Vec::new() }
    }

    pub fn queue(&mut self, draw_command: DrawCommand) { self.batch.push(draw_command); }

    pub fn set_enabled(&mut self, enabled: bool) -> Vec<Vec<DrawCommand>> {
        log::info!("Set redraw {enabled}");
        let queued =
            if enabled && !self.enabled { std::mem::take(&mut self.queued) } else { Vec::new() };
        self.enabled = enabled;
        queued
    }

    /// Finishes one redraw, keeping its commands together while display is paused.
    pub fn finish_batch(&mut self) -> Option<Vec<DrawCommand>> {
        let batch = std::mem::take(&mut self.batch);
        if self.enabled {
            Some(batch)
        } else {
            self.queued.push(batch);
            None
        }
    }
}
