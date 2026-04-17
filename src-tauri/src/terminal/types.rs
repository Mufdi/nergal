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
