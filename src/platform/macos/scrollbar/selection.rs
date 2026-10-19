use crate::editor::{ScrollbarGrid, ScrollbarSnapshot};

/// Grid assignments for the two outer scrollbar lanes, ordered from top to bottom.
///
/// An empty lane still reserves space for its track. None means the lane is
/// disabled, not that its buffers happen to fit inside their viewports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScrollbarLanes {
    pub left: Option<Vec<u64>>,
    pub right: Option<Vec<u64>>,
}

/// Chooses which tiled grids the two outer scrollbar lanes represent.
///
/// for two side-by-side windows, either can be active without changing the
/// assignments meaning that a full-width window above them can appear in
/// both lanes.
pub fn select_lanes(enabled: bool, snapshot: Option<&ScrollbarSnapshot>) -> ScrollbarLanes {
    if !enabled {
        return ScrollbarLanes::default();
    }

    let mut lanes = ScrollbarLanes { left: None, right: Some(Vec::new()) };
    let Some(snapshot) = snapshot else {
        return lanes;
    };

    if has_vertical_split(&snapshot.grids) {
        lanes.left = Some(Vec::new());
    }

    let Some(active) = snapshot.active_grid() else {
        return lanes;
    };
    let rect = active.layout.rect;
    let midpoint = rect.min.x + rect.size().width / 2;

    if lanes.left.is_none() {
        lanes.right = Some(grids_at_column(&snapshot.grids, midpoint));
        return lanes;
    }

    // without the root dimensions, neither the screen center nor the far edge
    // is known. we keep both tracks, but wait for a complete layout to assign them.
    let Some(root) = snapshot.root_grid_size.filter(|size| size.width > 0) else {
        return lanes;
    };

    if midpoint > root.width / 2 {
        lanes.left = Some(matching_grids(&snapshot.grids, |grid| grid.layout.rect.min.x == 0));
        lanes.right = Some(grids_at_column(&snapshot.grids, midpoint));
    } else {
        lanes.left = Some(grids_at_column(&snapshot.grids, midpoint));
        lanes.right =
            Some(matching_grids(&snapshot.grids, |grid| grid.layout.rect.max.x == root.width));
    }

    lanes
}

fn grids_at_column(grids: &[ScrollbarGrid], column: u64) -> Vec<u64> {
    matching_grids(grids, |grid| {
        let rect = grid.layout.rect;
        rect.min.x <= column && column <= rect.max.x
    })
}

fn matching_grids(grids: &[ScrollbarGrid], matches: impl Fn(&ScrollbarGrid) -> bool) -> Vec<u64> {
    let mut selected: Vec<_> = grids.iter().filter(|grid| matches(grid)).collect();
    selected.sort_unstable_by_key(|grid| (grid.layout.rect.min.y, grid.grid));
    selected.into_iter().map(|grid| grid.grid).collect()
}

/// Detects side-by-side tiled windows using layout rather than buffer contents.
fn has_vertical_split(grids: &[ScrollbarGrid]) -> bool {
    grids.iter().enumerate().any(|(index, first)| {
        let a = first.layout.rect;
        grids[index + 1..].iter().any(|second| {
            let b = second.layout.rect;
            let rows_overlap = a.min.y < b.max.y && b.min.y < a.max.y;
            let columns_are_separate = a.max.x <= b.min.x || b.max.x <= a.min.x;
            rows_overlap && columns_are_separate
        })
    })
}
