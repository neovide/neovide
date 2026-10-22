use winit::window::WindowId;

#[derive(Clone, Debug)]
pub struct EditorSwitcherRow {
    pub window_id: WindowId,
    pub title: String,
    pub subtitle: String,
    pub modified: bool,
    pub is_current: bool,
}

pub fn editor_switcher_row_matches(row: &EditorSwitcherRow, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let title = row.title.to_lowercase();
    let subtitle = row.subtitle.to_lowercase();
    let modified_state = if row.modified { "modified" } else { "" };
    query.split_whitespace().all(|term| {
        title.contains(term) || subtitle.contains(term) || modified_state.contains(term)
    })
}
