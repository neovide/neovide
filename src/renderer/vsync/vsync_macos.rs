use std::sync::{Arc, Condvar, Mutex};

use log::{error, trace, warn};

use crate::renderer::WindowedContext;

use super::macos_display_link::{
    core_video, get_display_id_of_window, MacosDisplayLink, MacosDisplayLinkCallbackArgs,
};

struct VSyncMacosDisplayLinkUserData {
    vsync_count: Arc<(Mutex<usize>, Condvar)>,
}

fn vsync_macos_display_link_callback(
    _args: &mut MacosDisplayLinkCallbackArgs,
    user_data: &mut VSyncMacosDisplayLinkUserData,
) {
    let (lock, cvar) = &*user_data.vsync_count;
    let mut count = lock.lock().unwrap();
    *count += 1;
    cvar.notify_one();
}

pub struct VSyncMacos {
    old_display: core_video::CGDirectDisplayID,
    display_link: Option<MacosDisplayLink<VSyncMacosDisplayLinkUserData>>,
    vsync_count: Arc<(Mutex<usize>, Condvar)>,
    last_vsync: usize,
}

impl VSyncMacos {
    pub fn new(context: &WindowedContext) -> Self {
        let mut vsync = VSyncMacos {
            old_display: 0,
            display_link: None,
            vsync_count: Arc::new((Mutex::new(0), Condvar::new())),
            last_vsync: 0,
        };

        vsync.display_link = vsync.create_display_link(context);

        vsync
    }

    fn create_display_link(
        self: &mut Self,
        context: &WindowedContext,
    ) -> Option<MacosDisplayLink<VSyncMacosDisplayLinkUserData>> {
        self.old_display = get_display_id_of_window(context.window());

        let vsync_count = self.vsync_count.clone();

        match MacosDisplayLink::new_from_display(
            self.old_display,
            vsync_macos_display_link_callback,
            VSyncMacosDisplayLinkUserData { vsync_count },
        ) {
            Ok(display_link) => {
                trace!("Succeeded to create display link.");
                match display_link.start() {
                    Ok(did) => match did {
                        true => {
                            trace!("Display link started.");
                        }
                        false => {
                            warn!("Display link already started. This does not affect function. But it might be a bug.");
                        }
                    },
                    Err(code) => {
                        error!("Failed to start display link, CVReturn code: {}.", code);
                    }
                }
                Some(display_link)
            }
            Err(code) => {
                error!("Failed to create display link, CVReturn code: {}.", code);
                None
            }
        }
    }

    pub fn wait_for_vsync(&mut self) {
        let (lock, cvar) = &*self.vsync_count;
        let count = cvar
            .wait_while(lock.lock().unwrap(), |count| *count < self.last_vsync + 1)
            .unwrap();
        self.last_vsync = *count;
    }

    pub fn update(&mut self, context: &WindowedContext) {
        let new_display = get_display_id_of_window(context.window());
        if new_display != self.old_display {
            trace!("Window moved to a new screen, try to re-create the display link.");
            self.display_link = self.create_display_link(context);
        }
    }
}
