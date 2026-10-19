use std::collections::HashMap;

use crate::{
    bridge::ScrollbarWindowMetadata, renderer::rendered_window::BASE_GRID_ID, units::GridSize,
};

use super::{TiledWindowLayout, Window, WindowViewport};

/// A visible tiled grid, including its layout when viewport data has not arrived yet.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollbarGrid {
    pub grid: u64,
    pub layout: TiledWindowLayout,
    pub grid_size: GridSize<u64>,
    pub viewport: Option<WindowViewport>,
    pub metadata: Option<ScrollbarWindowMetadata>,
    pub top_margin: u64,
    pub bottom_margin: u64,
}

/// Scrollbar state belonging to one completed neovim redraw.
///
/// This travels in the draw batch rather than as separate window events. A paused
/// redraw therefore keeps both the rendered grids and their controls at the same
/// revision. layout does not depend on a buffer having usable viewport data.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollbarSnapshot {
    pub root_grid_size: Option<GridSize<u64>>,
    /// The last valid tiled focus, retained while the cursor is in a float or message grid.
    pub active_tiled_grid: Option<u64>,
    pub grids: Vec<ScrollbarGrid>,
}

/// This keeps the pre layout metadata and invalidates only viewports for a replaced buffer.
pub fn update_window_metadata(
    windows: &mut HashMap<u64, Window>,
    metadata_by_window: &mut HashMap<u64, ScrollbarWindowMetadata>,
    window: u64,
    metadata: Option<ScrollbarWindowMetadata>,
) {
    let previous = match metadata {
        Some(metadata) => metadata_by_window.insert(window, metadata),
        None => metadata_by_window.remove(&window),
    };

    if previous.is_some_and(|previous| Some(previous.buffer) != metadata.map(|value| value.buffer))
    {
        for grid in windows.values_mut() {
            grid.clear_viewport_for_window(window);
        }
    }
}

/// Resolves tiled focus from the completed layout, not intermediate cursor events.
pub fn retain_tiled_focus(
    windows: &HashMap<u64, Window>,
    focused_grid: u64,
    previous_grid: Option<u64>,
) -> Option<u64> {
    std::iter::once(focused_grid)
        .chain(previous_grid)
        .find(|grid| windows.get(grid).and_then(Window::visible_tiled_layout).is_some())
}

impl ScrollbarSnapshot {
    pub fn capture(
        windows: &HashMap<u64, Window>,
        active_grid: Option<u64>,
        metadata_by_window: &HashMap<u64, ScrollbarWindowMetadata>,
    ) -> Self {
        let root_grid_size = windows
            .get(&BASE_GRID_ID)
            .map(|window| GridSize::new(window.get_width(), window.get_height()));
        let mut grids: Vec<_> = windows
            .iter()
            .filter_map(|(&grid, window)| {
                let layout = window.visible_tiled_layout()?;
                let (top_margin, bottom_margin) = window.viewport_margins();
                Some(ScrollbarGrid {
                    grid,
                    layout,
                    grid_size: GridSize::new(window.get_width(), window.get_height()),
                    viewport: window.viewport().filter(|viewport| viewport.window == layout.window),
                    metadata: metadata_by_window.get(&layout.window).copied(),
                    top_margin,
                    bottom_margin,
                })
            })
            .collect();

        grids.sort_unstable_by_key(|record| record.grid);
        let active_tiled_grid =
            active_grid.filter(|grid| grids.iter().any(|record| record.grid == *grid));

        Self { root_grid_size, active_tiled_grid, grids }
    }

    pub fn active_grid(&self) -> Option<&ScrollbarGrid> {
        let active = self.active_tiled_grid?;
        self.grids.iter().find(|record| record.grid == active)
    }
}
