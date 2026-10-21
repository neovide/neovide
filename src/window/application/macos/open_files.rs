use std::path::Path;

use winit::event_loop::ActiveEventLoop;

use super::super::Application;
use crate::bridge::OpenArgs;
use crate::bridge::send_or_queue_file_drop;
use crate::bridge::set_active_route_handler;
use crate::cmd_line::CmdLineSettings;
use crate::utils::resolve_relative_path;
use crate::window::UserEvent;

impl Application {
    pub(in crate::window::application) fn handle_open_files(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: UserEvent,
    ) {
        let UserEvent::OpenFiles {
            files,
            cwd,
            caller_cwd,
            tabs,
            new_window,
            neovim_bin,
            neovim_args,
        } = event
        else {
            unreachable!("only OpenFiles events reach the macOS file handler");
        };

        let cwd = cwd.as_deref().map(Path::new);
        let caller_cwd = caller_cwd.as_deref().map(Path::new);
        let open_args = OpenArgs {
            files_to_open: files
                .into_iter()
                .map(|path| resolve_relative_path(&path, caller_cwd))
                .collect(),
            tabs,
            neovim_bin,
            neovim_args,
        };

        self.prepare_open_files(event_loop, new_window, cwd, open_args);
    }

    fn activate_focused_route(&self) {
        let Some(window_id) = self.window_wrapper.get_focused_route() else {
            return;
        };

        if let Some(route_id) = self.window_wrapper.route_id_for_window(window_id) {
            set_active_route_handler(route_id);
        }

        self.window_wrapper.activate_and_focus_window(window_id);
    }

    fn prepare_open_files(
        &mut self,
        event_loop: &ActiveEventLoop,
        new_window: bool,
        cwd: Option<&Path>,
        args: OpenArgs,
    ) {
        if !new_window {
            self.activate_focused_route();
            self.send_file_drops(args);
            return;
        }

        if self.settings.get::<CmdLineSettings>().server.is_some() {
            self.window_wrapper.try_create_window(event_loop, &self.proxy, cwd, None);
            self.mark_should_render_all();
            self.send_file_drops(args);
            return;
        }

        self.window_wrapper.try_create_window(event_loop, &self.proxy, cwd, Some(args));
        self.mark_should_render_all();
    }

    fn send_file_drops(&self, args: OpenArgs) {
        for path in args.files_to_open {
            send_or_queue_file_drop(path, Some(args.tabs));
        }
    }
}
