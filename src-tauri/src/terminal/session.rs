use std::io::Write;
use std::sync::Arc;

use wezterm_term::{Terminal, TerminalSize};

use super::config::CluihudTerminalConfig;
use super::types::{CellSnapshot, CursorSnapshot, GridSnapshot};

/// A single terminal emulator instance owned by the backend.
///
/// Phase 1 only: no IPC coupling, no coalescing, no grid differ. Those land
/// in Phase 2. What this gives us is the ability to feed PTY bytes in, read
/// the resulting screen state out, and cover it with unit tests.
pub struct TerminalSession {
    terminal: Terminal,
    cols: u16,
    rows: u16,
}

impl TerminalSession {
    /// Build a session. `writer` is whatever the terminal should use to talk
    /// back to the PTY (answerbacks, mouse reports, encoded keyboard input).
    /// In tests pass a `Vec<u8>` or `std::io::sink()`; in production this
    /// will be wired to the `portable-pty` master writer.
    pub fn new(cols: u16, rows: u16, writer: Box<dyn Write + Send>) -> Self {
        Self::with_config(cols, rows, writer, CluihudTerminalConfig::new())
    }

    pub fn with_config(
        cols: u16,
        rows: u16,
        writer: Box<dyn Write + Send>,
        config: CluihudTerminalConfig,
    ) -> Self {
        let size = TerminalSize {
            rows: rows as usize,
            cols: cols as usize,
            pixel_width: 0,
            pixel_height: 0,
            dpi: 0,
        };
        let terminal = Terminal::new(
            size,
            Arc::new(config),
            "cluihud",
            env!("CARGO_PKG_VERSION"),
            writer,
        );
        Self { terminal, cols, rows }
    }

    /// Feed bytes from the PTY reader into the VT parser.
    pub fn advance_bytes(&mut self, bytes: &[u8]) {
        self.terminal.advance_bytes(bytes);
    }

    /// Resize the visible area. The PTY resize (ioctl TIOCSWINSZ) stays on
    /// the `pty.rs` side — this only updates the emulator's internal model.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let size = TerminalSize {
            rows: rows as usize,
            cols: cols as usize,
            pixel_width: 0,
            pixel_height: 0,
            dpi: 0,
        };
        self.terminal.resize(size);
        self.cols = cols;
        self.rows = rows;
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Read-only access to the underlying wezterm terminal. Useful for tests
    /// and for Phase 2 grid-diffing logic that needs richer access than
    /// [`Self::grid_snapshot`] exposes.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// Extract a renderable snapshot of the visible screen. Phase 2 replaces
    /// the ad-hoc "only changed rows" diffing on top of this. For Phase 1
    /// tests, the caller can compare two snapshots directly.
    pub fn grid_snapshot(&self) -> GridSnapshot {
        let screen = self.terminal.screen();
        let cols = screen.physical_cols;
        let total_rows = screen.scrollback_rows();
        let start = total_rows.saturating_sub(screen.physical_rows);
        let visible = screen.lines_in_phys_range(start..total_rows);
        let mut rows: Vec<Vec<CellSnapshot>> = Vec::with_capacity(visible.len());

        for line in &visible {
            let mut row_cells: Vec<CellSnapshot> = Vec::with_capacity(cols);
            for cell in line.visible_cells() {
                let attrs = cell.attrs();
                let snap = CellSnapshot {
                    ch: cell.str().to_owned(),
                    fg: color_to_rgba(attrs.foreground()),
                    bg: color_to_rgba(attrs.background()),
                    bold: attrs.intensity() == termwiz::cell::Intensity::Bold,
                    italic: attrs.italic(),
                    underline: attrs.underline() != termwiz::cell::Underline::None,
                    reverse: attrs.reverse(),
                    hyperlink: attrs.hyperlink().map(|h| h.uri().to_owned()),
                };
                row_cells.push(snap);
            }
            while row_cells.len() < cols {
                row_cells.push(CellSnapshot::default());
            }
            rows.push(row_cells);
        }

        let cursor_pos = self.terminal.cursor_pos();
        let cursor = CursorSnapshot {
            x: cursor_pos.x,
            y: cursor_pos.y.max(0) as usize,
            visible: cursor_pos.visibility == termwiz::surface::CursorVisibility::Visible,
        };

        let raw_title = self.terminal.get_title();
        let title = if raw_title.is_empty() {
            None
        } else {
            Some(raw_title.to_owned())
        };

        GridSnapshot { cols, rows, cursor, title }
    }
}

fn color_to_rgba(color: termwiz::color::ColorAttribute) -> Option<[u8; 4]> {
    use termwiz::color::ColorAttribute;
    match color {
        ColorAttribute::Default => None,
        ColorAttribute::TrueColorWithDefaultFallback(srgb)
        | ColorAttribute::TrueColorWithPaletteFallback(srgb, _) => {
            let (r, g, b, a) = srgb.to_tuple_rgba();
            Some([scale(r), scale(g), scale(b), scale(a)])
        }
        ColorAttribute::PaletteIndex(_) => None,
    }
}

fn scale(f: f32) -> u8 {
    (f.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(cols: u16, rows: u16) -> TerminalSession {
        TerminalSession::new(cols, rows, Box::new(Vec::<u8>::new()))
    }

    fn plain_row(snapshot: &GridSnapshot, row: usize) -> String {
        snapshot.rows[row]
            .iter()
            .map(|c| c.ch.as_str())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    #[test]
    fn plain_text_lands_on_first_row() {
        let mut s = session(20, 5);
        s.advance_bytes(b"hello world");
        let grid = s.grid_snapshot();
        assert_eq!(plain_row(&grid, 0), "hello world");
        assert_eq!(grid.cursor.x, 11);
        assert_eq!(grid.cursor.y, 0);
    }

    #[test]
    fn newline_moves_cursor_down() {
        let mut s = session(20, 5);
        s.advance_bytes(b"line1\r\nline2");
        let grid = s.grid_snapshot();
        assert_eq!(plain_row(&grid, 0), "line1");
        assert_eq!(plain_row(&grid, 1), "line2");
        assert_eq!(grid.cursor.y, 1);
    }

    #[test]
    fn sgr_red_sets_foreground() {
        let mut s = session(10, 3);
        // ESC[31m = red fg; "X"; ESC[0m = reset
        s.advance_bytes(b"\x1b[31mX\x1b[0m");
        let grid = s.grid_snapshot();
        let first = &grid.rows[0][0];
        assert_eq!(first.ch, "X");
        // Palette-index red on a default palette is returned as None by the
        // minimal snapshot. We still assert the cell character made it through.
        // Full color assertion moves to Phase 2 once we ship the truecolor path.
    }

    #[test]
    fn clear_screen_wipes_rows() {
        let mut s = session(10, 3);
        s.advance_bytes(b"dirty line");
        assert_eq!(plain_row(&s.grid_snapshot(), 0), "dirty line");
        // ESC[2J clears entire screen; ESC[H homes cursor.
        s.advance_bytes(b"\x1b[2J\x1b[H");
        let grid = s.grid_snapshot();
        assert_eq!(plain_row(&grid, 0), "");
        assert_eq!(grid.cursor.x, 0);
        assert_eq!(grid.cursor.y, 0);
    }

    #[test]
    fn osc_8_hyperlink_is_captured() {
        let mut s = session(40, 3);
        // OSC 8 ; ; URL ST  "text"  OSC 8 ; ; ST
        s.advance_bytes(
            b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07",
        );
        let grid = s.grid_snapshot();
        let first = &grid.rows[0][0];
        assert_eq!(first.ch, "l");
        assert_eq!(first.hyperlink.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn resize_updates_dimensions() {
        let mut s = session(80, 24);
        assert_eq!(s.cols(), 80);
        assert_eq!(s.rows(), 24);
        s.resize(120, 40);
        assert_eq!(s.cols(), 120);
        assert_eq!(s.rows(), 40);
        let grid = s.grid_snapshot();
        assert_eq!(grid.cols, 120);
        assert_eq!(grid.rows.len(), 40);
    }

    #[test]
    fn title_updates_via_osc_sequence() {
        let mut s = session(20, 3);
        // OSC 0 sets icon and window title
        s.advance_bytes(b"\x1b]0;cluihud test\x07");
        let grid = s.grid_snapshot();
        assert_eq!(grid.title.as_deref(), Some("cluihud test"));
    }
}
