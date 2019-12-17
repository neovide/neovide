use std::time::Instant;

pub struct FpsTracker {
    last_record_time: Instant,
    frame_count: usize,
    pub fps: usize
}

impl FpsTracker {
    pub fn new() -> FpsTracker {
        FpsTracker {
            fps: 0,
            last_record_time: Instant::now(),
            frame_count: 0
        }
    }

    pub fn record_frame(&mut self) {
        self.frame_count = self.frame_count + 1;
        let now = Instant::now();
        let time_since = (now - self.last_record_time).as_secs_f32();
        if time_since > 1.0 {
            self.fps = self.frame_count;
            self.last_record_time = now;
            self.frame_count = 0;
        }
    }
}
