use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use glamour::Point2;
use winit::event::WindowEvent;
use winit::event_loop::EventLoopProxy;
use winit::platform::macos;
use winit::platform::macos::WindowExtMacOS;
use winit::window::Window;
use winit::window::WindowId;

use super::EventPayload;
use super::Route;
use super::RouteWindow;
use super::ShortcutHandling;
use super::WindowPadding;
use super::WindowSettingsChanged;
use super::WinitWindowWrapper;
use crate::CmdLineSettings;
use crate::bridge::NeovimHandler;
use crate::bridge::OpenArgs;
use crate::error_msg;
use crate::renderer::Renderer;
use crate::renderer::rendered_window::BASE_GRID_ID;
use crate::renderer::rendered_window::NO_MULTIGRID_GRID_ID;
use crate::settings::Settings;
use crate::units::GridPos;
use crate::units::Pixel;
use crate::units::PixelPos;
use crate::units::PixelRect;
use crate::units::PixelSize;
use crate::window::ForceClickKind;
use crate::window::MacShortcutCommand;
use crate::window::macos::EditorSwitcherRow;
use crate::window::macos::MacosWindowFeature;
use crate::window::macos::ScrollbarLayoutInput;
use crate::window::macos::TouchpadStage;
use crate::window::macos::close_editor_switcher_if_open;
use crate::window::macos::editor_switcher_key_event_matches;
use crate::window::macos::hide_application;
use crate::window::macos::native_tab_bar_enabled;
use crate::window::macos::show_editor_switcher_panel;
use crate::window::macos::tab_navigation::TabNavigationAction;
use crate::window::macos::tab_navigation::TabNavigationHotkeys;
use crate::window::settings::platform::OptionAsMeta;

pub(super) fn create_window_feature(
    window: &Rc<Window>,
    settings: Arc<Settings>,
    proxy: EventLoopProxy<EventPayload>,
    neovim_handler: NeovimHandler,
    renderer: &Rc<RefCell<Box<Renderer>>>,
    creating_initial_window: bool,
) -> MacosWindowFeature {
    let mut feature =
        MacosWindowFeature::from_winit_window(window, settings, proxy, neovim_handler);
    feature.apply_scrollbar_snapshot(&renderer.borrow());
    if creating_initial_window {
        feature.activate_and_focus();
    }
    feature
}

pub(super) struct WindowNavigationState {
    pub(super) window_mru: VecDeque<WindowId>,
    pub(super) focus_return_target: Option<WindowId>,
    ignore_next_focus_gain: bool,
    tab_navigation_hotkeys: TabNavigationHotkeys,
}

impl WindowNavigationState {
    pub(super) fn new(settings: &Settings) -> Self {
        Self {
            window_mru: VecDeque::new(),
            focus_return_target: None,
            ignore_next_focus_gain: false,
            tab_navigation_hotkeys: TabNavigationHotkeys::new(settings),
        }
    }
}

impl WinitWindowWrapper {
    pub(super) fn handle_platform_window_settings_changed(
        &mut self,
        window_ids: &[WindowId],
        changed_setting: WindowSettingsChanged,
    ) {
        match changed_setting {
            WindowSettingsChanged::InputMacosOptionKeyIsMeta(option) => {
                for window_id in window_ids {
                    self.set_macos_option_as_meta(*window_id, option);
                }
            }
            WindowSettingsChanged::InputMacosAltIsMeta(true) => {
                error_msg!(concat!(
                    "neovide_input_macos_alt_is_meta has now been removed. ",
                    "Use neovide_input_macos_option_key_is_meta instead. ",
                    "Please check https://neovide.dev/configuration.html#macos-option-key-is-meta for more information.",
                ));
            }
            WindowSettingsChanged::MacosSimpleFullscreen(fullscreen) => {
                for window_id in window_ids {
                    self.set_simple_fullscreen(*window_id, fullscreen);
                }
            }
            WindowSettingsChanged::SystemScrollbar(_) => {
                for window_id in window_ids {
                    if let Some(route) = self.routes.get(window_id) {
                        let renderer = route.window.renderer.borrow();
                        route.window.with_macos_feature_mut(|feature| {
                            feature.apply_scrollbar_snapshot(&renderer);
                        });
                    }
                }
            }
            _ => {}
        }

        for window_id in window_ids {
            if let Some(feature) = self.macos_feature_for_window(*window_id) {
                feature.borrow_mut().handle_settings_changed(changed_setting.clone());
            }
        }
    }

    pub(super) fn handle_touchpad_pressure_command(
        &mut self,
        window_id: WindowId,
        col: i64,
        row: i64,
        entity: String,
        guifont: String,
        kind: ForceClickKind,
    ) {
        let Some(feature) = self.macos_feature_for_window(window_id) else {
            log::warn!("Touchpad pressure received before macOS feature initialization");
            return;
        };

        let titlebar_height = feature.borrow().system_titlebar_height as f32;
        let window_padding = self.calculate_window_padding(window_id);
        let pixel_position = self.grid_to_pixel_position(window_id, col, row);
        let Some(grid_scale_height) = self.routes.get(&window_id).map(|route| {
            let renderer = route.window.renderer.borrow();
            renderer.grid_renderer.grid_scale.height()
        }) else {
            return;
        };
        let point = self.apply_padding_to_position(pixel_position, window_padding, titlebar_height);

        feature.borrow_mut().handle_force_click_target(
            &entity,
            kind,
            point,
            guifont,
            grid_scale_height,
        );
    }

    pub(super) fn handle_highlight_matching_pair_command(
        &self,
        window_id: WindowId,
        grid: u64,
        row: u64,
        column: u64,
        text: Option<String>,
    ) {
        let target_grid = if grid == NO_MULTIGRID_GRID_ID { BASE_GRID_ID } else { grid };
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let rect = {
            let renderer = route.window.renderer.borrow();
            let grid_scale = renderer.grid_renderer.grid_scale;
            let cell_size = PixelSize::new(grid_scale.width(), grid_scale.height());
            let grid_pos = GridPos::new(column as f32, row as f32);
            renderer.rendered_windows.get(&target_grid).map(|window| {
                let mut adjusted_grid = grid_pos + window.grid_current_position.to_vector();
                adjusted_grid.y -= window.scroll_animation.position;
                let origin = adjusted_grid * grid_scale;
                PixelRect::from_origin_and_size(origin, cell_size)
            })
        };

        if let (Some(rect), Some(feature)) = (rect, self.macos_feature_for_window(window_id)) {
            feature.borrow_mut().show_find_indicator_for_rect(rect, text.as_deref());
        }
    }

    pub(super) fn handle_platform_shortcuts(
        &mut self,
        window_id: WindowId,
        event: &WindowEvent,
    ) -> ShortcutHandling {
        if let WindowEvent::KeyboardInput { event: key_event, is_synthetic: false, .. } = event {
            let modifiers = self.keyboard_manager.current_modifiers();
            if editor_switcher_key_event_matches(key_event, &modifiers) {
                self.show_editor_switcher();
                return ShortcutHandling { overlay_consumed: true, ..Default::default() };
            }
        }

        if native_tab_bar_enabled()
            && let WindowEvent::KeyboardInput { event: key_event, .. } = event
        {
            let modifiers = self.keyboard_manager.current_modifiers();
            if let Some(action) =
                self.window_navigation.tab_navigation_hotkeys.action_for(key_event, &modifiers)
                && let Some(route) = self.routes.get(&window_id)
                && let Some(feature) = &route.window.macos_feature
            {
                let feature_ref = feature.borrow();
                if feature_ref.can_navigate_tabs() {
                    match action {
                        TabNavigationAction::Next => feature_ref.select_next_tab(),
                        TabNavigationAction::Previous => feature_ref.select_previous_tab(),
                    }
                    return ShortcutHandling { keyboard_consumed: true, ..Default::default() };
                }
            }
        }

        ShortcutHandling::default()
    }

    pub fn handle_mac_shortcut(&mut self, command: MacShortcutCommand) {
        match command {
            MacShortcutCommand::TogglePinnedWindow => self.toggle_pinned_window(),
            MacShortcutCommand::ShowEditorSwitcher => self.show_editor_switcher(),
        }
    }

    pub(super) fn record_window_usage(&mut self, window_id: WindowId) {
        self.window_navigation.window_mru.retain(|id| *id != window_id);
        self.window_navigation.window_mru.push_front(window_id);
    }

    fn cleanup_window_mru(&mut self) {
        self.window_navigation.window_mru.retain(|id| self.routes.contains_key(id));
    }

    pub(super) fn handle_focus_gain_for_shortcuts(&mut self, window_id: WindowId) {
        if self.window_navigation.ignore_next_focus_gain {
            self.window_navigation.ignore_next_focus_gain = false;
        } else {
            self.record_window_usage(window_id);
        }
    }

    fn pinned_candidate(&mut self) -> Option<WindowId> {
        self.cleanup_window_mru();
        if let Some(id) = self.window_navigation.window_mru.front().copied() {
            Some(id)
        } else {
            self.routes.keys().next().copied()
        }
    }

    fn editor_switcher_rows(&mut self) -> Vec<EditorSwitcherRow> {
        let focused_route = self.get_focused_route();
        self.cleanup_window_mru();

        let mut ordered_windows: Vec<WindowId> =
            self.window_navigation.window_mru.iter().copied().collect();
        for window_id in self.routes.keys().copied() {
            if !ordered_windows.contains(&window_id) {
                ordered_windows.push(window_id);
            }
        }

        ordered_windows
            .into_iter()
            .filter_map(|window_id| {
                let route = self.routes.get(&window_id)?;
                let title = if route.window.title.is_empty() {
                    "Untitled".to_string()
                } else {
                    route.window.title.clone()
                };

                let subtitle = Self::editor_switcher_subtitle(route);

                Some(EditorSwitcherRow {
                    window_id,
                    title,
                    subtitle,
                    modified: route.window.document.modified,
                    is_current: focused_route == Some(window_id),
                })
            })
            .collect()
    }

    fn editor_switcher_subtitle(route: &Route) -> String {
        if !route.window.document.path.is_empty() {
            route.window.document.path.clone()
        } else if let Some(cwd) = &route.cwd {
            cwd.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    }

    fn capture_focus_target(&mut self, pinned_id: WindowId) {
        let current = self.get_focused_route();
        if current != Some(pinned_id) {
            self.window_navigation.focus_return_target = current;
        } else {
            self.window_navigation.focus_return_target = None;
        }
    }

    fn restore_focus_target(&mut self) -> bool {
        let Some(target_id) = self.window_navigation.focus_return_target.take() else {
            return false;
        };

        let Some(window) =
            self.routes.get(&target_id).map(|route| route.window.winit_window.clone())
        else {
            return false;
        };

        if let Some(feature) = self.macos_feature_for_window(target_id) {
            feature.borrow().activate_application();
        }

        window.focus_window();

        true
    }

    fn toggle_pinned_window(&mut self) {
        let Some(window_id) = self.pinned_candidate() else {
            return;
        };

        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let window = route.window.winit_window.clone();
        let macos_feature = self.macos_feature_for_window(window_id);
        let is_active = {
            if let Some(feature) = &macos_feature {
                feature.borrow().is_key_window()
            } else {
                window.has_focus()
            }
        };

        self.window_navigation.ignore_next_focus_gain = true;

        if is_active {
            let uses_native_tabs = native_tab_bar_enabled()
                && self.settings.get::<CmdLineSettings>().system_native_tabs;

            if uses_native_tabs {
                hide_application();
                return;
            }

            if !self.restore_focus_target() {
                hide_application();
            }
        } else {
            if let Some(feature) = &macos_feature {
                feature.borrow().activate_application();
            }
            self.capture_focus_target(window_id);
            window.focus_window();
            self.record_window_usage(window_id);
        }
    }

    fn show_editor_switcher(&mut self) {
        if self.routes.is_empty() {
            return;
        }

        if close_editor_switcher_if_open() {
            return;
        }

        let rows = self.editor_switcher_rows();
        let icon = self.settings.get::<CmdLineSettings>().icon;
        show_editor_switcher_panel(rows, icon.as_ref());
    }
}

#[derive(Default)]
pub(super) struct RouteLaunchConfig {
    neovim_bin: Option<String>,
    neovim_args: Option<Vec<String>>,
}

#[derive(Default)]
pub(super) struct DocumentState {
    pub(super) path: String,
    pub(super) modified: bool,
}

impl RouteLaunchConfig {
    fn from_open_args(args: &OpenArgs) -> Self {
        Self { neovim_bin: args.neovim_bin.clone(), neovim_args: args.neovim_args.clone() }
    }

    fn from_cmdline(cmdline: CmdLineSettings) -> Self {
        Self {
            neovim_bin: cmdline.neovim_bin,
            neovim_args: (!cmdline.neovim_args.is_empty()).then_some(cmdline.neovim_args),
        }
    }
}

impl RouteWindow {
    pub(super) fn handle_touchpad_pressure(&self, stage: i64) {
        if let Some(feature) = &self.macos_feature {
            match TouchpadStage::from_stage(stage) {
                TouchpadStage::Soft | TouchpadStage::Click => {
                    feature.borrow_mut().set_definition_is_active(false)
                }
                TouchpadStage::ForceClick => feature.borrow_mut().handle_touchpad_force_click(),
            }
        }
    }

    pub(super) fn with_macos_feature<R>(
        &self,
        read: impl FnOnce(&MacosWindowFeature) -> R,
    ) -> Option<R> {
        let feature = self.macos_feature.as_ref()?;

        Some(read(feature.borrow().as_ref()))
    }

    pub(super) fn with_macos_feature_mut<R>(
        &self,
        update: impl FnOnce(&mut MacosWindowFeature) -> R,
    ) -> Option<R> {
        let feature = self.macos_feature.as_ref()?;

        Some(update(feature.borrow_mut().as_mut()))
    }
}

impl WinitWindowWrapper {
    pub(super) fn route_launch_config_for_window_creation(
        &self,
        args: Option<&OpenArgs>,
        creating_initial_window: bool,
    ) -> RouteLaunchConfig {
        if let Some(args) = args {
            return RouteLaunchConfig::from_open_args(args);
        }

        if !creating_initial_window {
            return RouteLaunchConfig::default();
        }

        RouteLaunchConfig::from_cmdline(self.settings.get::<CmdLineSettings>())
    }

    pub fn set_macos_option_as_meta(&mut self, window_id: WindowId, option: OptionAsMeta) {
        let winit_option = match option {
            OptionAsMeta::OnlyLeft => macos::OptionAsAlt::OnlyLeft,
            OptionAsMeta::OnlyRight => macos::OptionAsAlt::OnlyRight,
            OptionAsMeta::Both => macos::OptionAsAlt::Both,
            OptionAsMeta::None => macos::OptionAsAlt::None,
        };

        let Some(route) = self.routes.get(&window_id) else {
            return;
        };
        let window = route.window.winit_window.clone();
        if winit_option != window.option_as_alt() {
            window.set_option_as_alt(winit_option);
        }
    }

    pub fn set_simple_fullscreen(&mut self, window_id: WindowId, fullscreen: bool) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let window = route.window.winit_window.clone();
        let Some(feature) = &route.window.macos_feature else {
            window.set_simple_fullscreen(fullscreen);
            return;
        };

        if fullscreen {
            feature.borrow_mut().set_simple_fullscreen_mode(true);
            window.set_simple_fullscreen(true);
        } else {
            window.set_simple_fullscreen(false);
            feature.borrow_mut().set_simple_fullscreen_mode(false);
        }
    }

    pub(super) fn apply_document_state(&self, window_id: WindowId) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let Some(feature) = self.macos_feature_for_window(window_id) else {
            return;
        };

        feature
            .borrow()
            .set_document_state(&route.window.document.path, route.window.document.modified);
    }

    pub(super) fn sync_native_tabs_resize(&mut self, source_window_id: WindowId) {
        if !native_tab_bar_enabled() || !self.settings.get::<CmdLineSettings>().system_native_tabs {
            return;
        }

        if self.routes.len() <= 1 {
            return;
        }

        let Some(source_route) = self.routes.get(&source_window_id) else {
            return;
        };

        let shared_inner_size = source_route.window.winit_window.inner_size();
        let window_ids: Vec<WindowId> = self.routes.keys().copied().collect();

        // System native tabs share one visual surface. Keep all routes in sync so
        // fullscreen transitions trigger immediate grid updates for every tab.
        for window_id in window_ids {
            if let Some(route) = self.routes.get_mut(&window_id)
                && let Some(macos_feature) = &mut route.window.macos_feature
            {
                macos_feature.borrow_mut().handle_size_changed();
            }

            let window_padding = self.calculate_window_padding(window_id);
            if let Some(route) = self.routes.get_mut(&window_id) {
                route.state.saved_inner_size = shared_inner_size;
                route.state.window_padding = window_padding;
            }

            self.update_grid_size_from_window(window_id);
        }
    }

    pub fn grid_to_pixel_position(
        &mut self,
        window_id: WindowId,
        col: i64,
        row: i64,
    ) -> Point2<Pixel<f32>> {
        let grid_position = GridPos::new(col, row);
        let mut renderer = {
            let route = self.routes.get(&window_id).expect("window must exist");
            route.window.renderer.borrow_mut()
        };
        let root_region_offset = renderer.window_regions.first().map(|region| region.region.min);

        // Align the lookup point with the actual font baseline instead of a heuristic offset.
        let grid_scale = renderer.grid_renderer.grid_scale;
        let baseline_offset = renderer.grid_renderer.shaper.baseline_offset();
        drop(renderer);

        let mut position = grid_position * grid_scale;

        if let Some(offset) = root_region_offset {
            position.x += offset.x;
            position.y += offset.y;
        }

        position.y += baseline_offset;

        position
    }

    pub fn apply_padding_to_position(
        &self,
        position: Point2<Pixel<f32>>,
        padding: WindowPadding,
        titlebar_height: f32,
    ) -> Point2<Pixel<f32>> {
        let _ = (padding, titlebar_height);
        position
    }

    pub fn focused_route_launch_context(&self) -> Option<(Option<PathBuf>, OpenArgs)> {
        let focused_id = self.get_focused_route()?;
        let route = self.routes.get(&focused_id)?;
        Some((
            route.cwd.clone(),
            OpenArgs {
                files_to_open: vec![],
                tabs: false,
                neovim_bin: route.launch_config.neovim_bin.clone(),
                neovim_args: route.launch_config.neovim_args.clone(),
            },
        ))
    }

    pub fn activate_and_focus_window(&self, window_id: WindowId) -> bool {
        let Some(feature) = self.macos_feature_for_window(window_id) else {
            return false;
        };

        feature.borrow().activate_and_focus();
        true
    }

    pub(super) fn fullscreen_for_new_window(
        &self,
        creating_initial_window: bool,
        fullscreen: bool,
    ) -> bool {
        if !self.should_handle_tabbed_fullscreen(creating_initial_window, fullscreen) {
            return fullscreen;
        }

        // cmd-n creates another top-level NSWindow. if system tabs are enabled,
        // AppKit then merges that new window into the focused fullscreen
        // window's tab group. If we also apply the global fullscreen
        // setting to the newly created window, AppKit tries to perform a
        // second fullscreen transition for a window that is about to become a
        // tab in the existing fullscreen group, which triggers
        // NSWindowStackController assertions. So, we keep the host window fullscreen,
        // but skip the initial fullscreen transition on the new window to avoid that.
        let opening_fullscreen_tab = self
            .get_focused_route()
            .and_then(|window_id| self.macos_feature_for_window(window_id))
            .is_some_and(|feature| feature.borrow().is_native_fullscreen_enabled());

        if opening_fullscreen_tab {
            log::info!("Skipping initial fullscreen for new tab window from fullscreen host");
            return false;
        }

        fullscreen
    }

    fn should_handle_tabbed_fullscreen(
        &self,
        creating_initial_window: bool,
        fullscreen: bool,
    ) -> bool {
        !creating_initial_window
            && fullscreen
            && native_tab_bar_enabled()
            && self.settings.get::<CmdLineSettings>().system_native_tabs
    }

    pub(super) fn macos_feature_for_window(
        &self,
        window_id: WindowId,
    ) -> Option<Rc<RefCell<Box<MacosWindowFeature>>>> {
        self.routes.get(&window_id).and_then(|route| route.window.macos_feature.as_ref().cloned())
    }

    pub(super) fn update_scrollbar_layout_from_window(&self, window_id: WindowId) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let window_padding = route.state.window_padding;

        let Some(macos_feature) = self.macos_feature_for_window(window_id) else {
            return;
        };

        let reserved_width = {
            let macos_feature = macos_feature.borrow();
            macos_feature.scrollbar_reserved_width_in_pixels()
        };

        if reserved_width == 0 {
            macos_feature.borrow_mut().set_scrollbar_layout(ScrollbarLayoutInput::default());
            return;
        }

        let content_size = self.content_pixel_size_from_window(window_id);

        // a resize request is not yet drawable state so we keep the track at the
        // committed root edge until neovim acknowledges the new grid size.
        let renderer = route.window.renderer.borrow();
        let grid_scale = renderer.grid_renderer.grid_scale;
        let root_grid_size = renderer
            .scrollbar_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.root_grid_size)
            .unwrap_or_else(|| renderer.get_grid_size().as_());
        let left_lane_enabled =
            macos_feature.borrow().scrollbar_left_reserved_width_in_pixels() > 0;

        macos_feature.borrow_mut().set_scrollbar_layout(ScrollbarLayoutInput {
            root_grid_size,
            grid_origin: PixelPos::new(
                f64::from(window_padding.left),
                f64::from(window_padding.top),
            ),
            content_size: window_padding.content_size(content_size).as_(),
            cell_size: PixelSize::new(
                f64::from(grid_scale.width()),
                f64::from(grid_scale.height()),
            ),
            left_lane_enabled,
        });
    }
}
