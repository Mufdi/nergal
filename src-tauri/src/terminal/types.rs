use serde::Serialize;

/// Renderable snapshot of a single cell. Intentionally minimal for Phase 1 —
/// the richer IPC payload (hyperlinks, attribute bitflags, truecolor) lands in
/// Phase 2 alongside the grid-update event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CellSnapshot {
    pub ch: String,
    pub fg: Option<[u8; 4]>,
    pub bg: Option<[u8; 4]>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub hyperlink: Option<String>,
}

impl Default for CellSnapshot {
    fn default() -> Self {
        Self {
            ch: " ".into(),
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            reverse: false,
            hyperlink: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CursorSnapshot {
    pub x: usize,
    pub y: usize,
    pub visible: bool,
}

/// Full grid snapshot used by Phase 1 tests and by the `terminal_get_full_grid`
/// command introduced in Phase 2. `rows[0]` is the top visible row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GridSnapshot {
    pub cols: usize,
    pub rows: Vec<Vec<CellSnapshot>>,
    pub cursor: CursorSnapshot,
    pub title: Option<String>,
}

/// Delta-ish payload emitted over the `terminal:grid-update` Tauri event.
///
/// Only rows that changed since the last emission are included, each paired
/// with their visible-row index. `cursor` and `title` always reflect the
/// current state (they are cheap to send and the frontend needs them every
/// tick to render the caret and the tab label).
///
/// `scroll_offset` is reserved for Phase 3 scrollback support; Phase 2 emits
/// `0` since the viewport is always pinned to the bottom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridUpdate {
    pub session_id: String,
    pub cols: usize,
    pub total_rows: usize,
    pub rows: Vec<GridRow>,
    pub cursor: CursorSnapshot,
    pub title: Option<String>,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridRow {
    pub index: usize,
    pub cells: Vec<CellSnapshot>,
}

/// A key event as delivered by the frontend. Phase 2 defines the shape so the
/// emitter event contract and the input contract co-evolve; the backend
/// handler lands in Phase 3 (task 3.x).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalKeyEvent {
    pub code: String,
    pub key: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}
