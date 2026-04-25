use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::types::{CellSnapshot, CursorSnapshot, GridRow, GridSnapshot, GridUpdate};

/// Keeps track of the last emitted state so subsequent [`GridDiffer::compute_update`]
/// calls can return only the rows that actually changed. This is the core of
/// the "send only deltas" contract on the `terminal:grid-update` event.
pub struct GridDiffer {
    row_hashes: Vec<u64>,
    last_cursor: Option<CursorSnapshot>,
    last_title: Option<String>,
    emitted_first: bool,
}

impl GridDiffer {
    pub fn new() -> Self {
        Self {
            row_hashes: Vec::new(),
            last_cursor: None,
            last_title: None,
            emitted_first: false,
        }
    }

    /// Force the next `compute_update` call to treat every row as changed.
    /// Useful after a resize or when the client explicitly asks for the full
    /// grid (see the `terminal_get_full_grid` command).
    pub fn invalidate(&mut self) {
        self.row_hashes.clear();
        self.last_cursor = None;
        self.last_title = None;
        self.emitted_first = false;
    }

    pub fn compute_update(
        &mut self,
        session_id: &str,
        snapshot: &GridSnapshot,
    ) -> Option<GridUpdate> {
        let total = snapshot.rows.len();
        if self.row_hashes.len() != total {
            self.row_hashes.resize(total, 0);
            self.emitted_first = false;
        }

        let mut changed_rows: Vec<GridRow> = Vec::new();

        for (index, row) in snapshot.rows.iter().enumerate() {
            let hash = hash_row(row);
            let was_different = !self.emitted_first || self.row_hashes[index] != hash;
            if was_different {
                self.row_hashes[index] = hash;
                changed_rows.push(GridRow {
                    index,
                    cells: row.clone(),
                });
            }
        }

        let cursor_changed = self.last_cursor.as_ref() != Some(&snapshot.cursor);
        let title_changed = self.last_title != snapshot.title;

        if !self.emitted_first || !changed_rows.is_empty() || cursor_changed || title_changed {
            self.last_cursor = Some(snapshot.cursor);
            self.last_title = snapshot.title.clone();
            self.emitted_first = true;
            Some(GridUpdate {
                session_id: session_id.to_owned(),
                cols: snapshot.cols,
                total_rows: total,
                rows: changed_rows,
                cursor: snapshot.cursor,
                title: snapshot.title.clone(),
                scroll_offset: snapshot.scroll_offset,
            })
        } else {
            None
        }
    }
}

impl Default for GridDiffer {
    fn default() -> Self {
        Self::new()
    }
}

fn hash_row(row: &[CellSnapshot]) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.hash(&mut hasher);
    hasher.finish()
}

impl Hash for CellSnapshot {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ch.hash(state);
        self.fg.hash(state);
        self.bg.hash(state);
        self.bold.hash(state);
        self.italic.hash(state);
        self.underline.hash(state);
        self.reverse.hash(state);
        self.hyperlink.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(rows: Vec<Vec<CellSnapshot>>, cursor_x: usize) -> GridSnapshot {
        GridSnapshot {
            cols: rows.first().map(|r| r.len()).unwrap_or(0),
            rows,
            cursor: CursorSnapshot {
                x: cursor_x,
                y: 0,
                visible: true,
            },
            title: None,
            scroll_offset: 0,
        }
    }

    fn row(text: &str, width: usize) -> Vec<CellSnapshot> {
        let mut cells: Vec<CellSnapshot> = text
            .chars()
            .map(|c| CellSnapshot {
                ch: c.to_string(),
                ..Default::default()
            })
            .collect();
        while cells.len() < width {
            cells.push(CellSnapshot::default());
        }
        cells
    }

    #[test]
    fn first_call_emits_every_row() {
        let mut differ = GridDiffer::new();
        let snap = snapshot(vec![row("a", 5), row("b", 5)], 1);
        let update = differ.compute_update("s1", &snap).expect("first emit");
        assert_eq!(update.rows.len(), 2);
        assert_eq!(update.rows[0].index, 0);
        assert_eq!(update.rows[1].index, 1);
    }

    #[test]
    fn identical_snapshot_emits_nothing() {
        let mut differ = GridDiffer::new();
        let snap = snapshot(vec![row("a", 5), row("b", 5)], 1);
        differ.compute_update("s1", &snap);
        let again = differ.compute_update("s1", &snap);
        assert!(again.is_none(), "unchanged state should not emit");
    }

    #[test]
    fn only_changed_row_is_emitted() {
        let mut differ = GridDiffer::new();
        differ.compute_update("s1", &snapshot(vec![row("a", 5), row("b", 5)], 1));
        let next = snapshot(vec![row("a", 5), row("B!", 5)], 2);
        let update = differ.compute_update("s1", &next).expect("diff");
        assert_eq!(update.rows.len(), 1);
        assert_eq!(update.rows[0].index, 1);
        assert_eq!(update.cursor.x, 2);
    }

    #[test]
    fn cursor_change_alone_still_emits() {
        let mut differ = GridDiffer::new();
        let base = snapshot(vec![row("a", 5)], 1);
        differ.compute_update("s1", &base);
        let moved = snapshot(vec![row("a", 5)], 3);
        let update = differ
            .compute_update("s1", &moved)
            .expect("cursor-only move should emit");
        assert!(update.rows.is_empty(), "no row body changed");
        assert_eq!(update.cursor.x, 3);
    }

    #[test]
    fn invalidate_forces_full_resend() {
        let mut differ = GridDiffer::new();
        let snap = snapshot(vec![row("a", 5)], 1);
        differ.compute_update("s1", &snap);
        differ.invalidate();
        let update = differ
            .compute_update("s1", &snap)
            .expect("post-invalidate full resend");
        assert_eq!(update.rows.len(), 1);
    }

    #[test]
    fn row_count_change_triggers_full_resend() {
        let mut differ = GridDiffer::new();
        differ.compute_update("s1", &snapshot(vec![row("a", 5)], 1));
        // Grid grew from 1 row to 3 rows (e.g. user resized the window).
        let resized = snapshot(vec![row("a", 5), row("b", 5), row("c", 5)], 1);
        let update = differ
            .compute_update("s1", &resized)
            .expect("resize must emit");
        assert_eq!(update.rows.len(), 3);
        assert_eq!(update.total_rows, 3);
    }
}
