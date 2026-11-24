use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::types::{CellSnapshot, CursorSnapshot, GridRow, GridSnapshot, GridUpdate, ImageRun};

/// Keeps track of the last emitted state so subsequent [`GridDiffer::compute_update`]
/// calls can return only the rows that actually changed. This is the core of
/// the "send only deltas" contract on the grid-update channel.
pub struct GridDiffer {
    row_hashes: Vec<u64>,
    last_cursor: Option<CursorSnapshot>,
    last_title: Option<String>,
    last_mouse_tracking: Option<bool>,
    emitted_first: bool,
}

impl GridDiffer {
    pub fn new() -> Self {
        Self {
            row_hashes: Vec::new(),
            last_cursor: None,
            last_title: None,
            last_mouse_tracking: None,
            emitted_first: false,
        }
    }

    /// Force the next `compute_update` call to treat every row as changed.
    /// Useful after a resize or when the client explicitly asks for the full
    /// grid (see the `terminal_attach` command).
    pub fn invalidate(&mut self) {
        self.row_hashes.clear();
        self.last_cursor = None;
        self.last_title = None;
        self.last_mouse_tracking = None;
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
            let runs = snapshot.images.get(index).map_or(&[][..], Vec::as_slice);
            let hash = hash_row(row, runs);
            let was_different = !self.emitted_first || self.row_hashes[index] != hash;
            if was_different {
                self.row_hashes[index] = hash;
                changed_rows.push(GridRow {
                    index,
                    cells: row.clone(),
                    images: runs.to_vec(),
                });
            }
        }

        let cursor_changed = self.last_cursor.as_ref() != Some(&snapshot.cursor);
        let title_changed = self.last_title != snapshot.title;
        let mouse_tracking_changed = self.last_mouse_tracking != Some(snapshot.mouse_tracking);

        if !self.emitted_first
            || !changed_rows.is_empty()
            || cursor_changed
            || title_changed
            || mouse_tracking_changed
        {
            self.last_cursor = Some(snapshot.cursor);
            self.last_title = snapshot.title.clone();
            self.last_mouse_tracking = Some(snapshot.mouse_tracking);
            self.emitted_first = true;
            Some(GridUpdate {
                session_id: session_id.to_owned(),
                cols: snapshot.cols,
                total_rows: total,
                rows: changed_rows,
                cursor: snapshot.cursor,
                title: snapshot.title.clone(),
                scroll_offset: snapshot.scroll_offset,
                is_alt_screen: snapshot.is_alt_screen,
                mouse_tracking: snapshot.mouse_tracking,
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

fn hash_row(row: &[CellSnapshot], runs: &[ImageRun]) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.hash(&mut hasher);
    runs.hash(&mut hasher);
    hasher.finish()
}

impl Hash for ImageRun {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
        self.col.hash(state);
        self.span.hash(state);
        self.u0.to_bits().hash(state);
        self.v0.to_bits().hash(state);
        self.u1.to_bits().hash(state);
        self.v1.to_bits().hash(state);
        self.pad_left.hash(state);
        self.pad_top.hash(state);
        self.pad_right.hash(state);
        self.pad_bottom.hash(state);
        self.z.hash(state);
    }
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
            is_alt_screen: false,
            mouse_tracking: false,
            images: vec![],
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

#[cfg(test)]
#[path = "differ_mouse_tracking_tests.rs"]
mod mouse_tracking_tests;

#[cfg(test)]
#[path = "differ_image_tests.rs"]
mod image_tests;
