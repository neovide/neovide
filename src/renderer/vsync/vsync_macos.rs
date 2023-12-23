use log::{error, trace, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use winit::event_loop::EventLoopProxy;

use crate::{renderer::WindowedContext, window::UserEvent};

use super::macos_display_link::{
    core_video, get_display_id_of_window, MacosDisplayLink, MacosDisplayLinkCallbackArgs,
};

struct VSyncMacosDisplayLinkUserData {
    proxy: EventLoopProxy<UserEvent>,
    redraw_requested: Arc<AtomicBool>,
}

fn vsync_macos_display_link_callback(
    _args: &mut MacosDisplayLinkCallbackArgs,
    user_data: &mut VSyncMacosDisplayLinkUserData,
) {
    if user_data.redraw_requested.swap(false, Ordering::Relaxed) {
        let _ = user_data.proxy.send_event(UserEvent::RedrawRequested);
    }
}

pub struct VSyncMacos {
    old_display: core_video::CGDirectDisplayID,
    display_link: Option<MacosDisplayLink<VSyncMacosDisplayLinkUserData>>,
    proxy: EventLoopProxy<UserEvent>,
    redraw_requested: Arc<AtomicBool>,
}

impl VSyncMacos {
    pub fn new(context: &WindowedContext, proxy: EventLoopProxy<UserEvent>) -> VSyncMacos {
        let redraw_requested = AtomicBool::new(false).into();
        let mut vsync = VSyncMacos {
            old_display: 0,
            display_link: None,
            proxy,
            redraw_requested,
        };

        vsync.create_display_link(context);

        vsync
    }

    fn create_display_link(&mut self, context: &WindowedContext) {
        self.old_display = get_display_id_of_window(context.window());

        let display_link = match MacosDisplayLink::new_from_display(
            self.old_display,
            vsync_macos_display_link_callback,
            VSyncMacosDisplayLinkUserData {
                proxy: self.proxy.clone(),
                redraw_requested: Arc::clone(&self.redraw_requested),
            },
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
        };
        self.display_link = display_link;
    }

    pub fn wait_for_vsync(&mut self) {}

    pub fn request_redraw(&mut self) {
        self.redraw_requested.store(true, Ordering::Relaxed);
    }

    pub fn update(&mut self, context: &WindowedContext) {
        let new_display = get_display_id_of_window(context.window());
        if new_display != self.old_display {
            trace!("Window moved to a new screen, try to re-create the display link.");
            self.create_display_link(context);
        }
    }
}
