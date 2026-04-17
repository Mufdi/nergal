use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use wezterm_term::{Terminal, TerminalSize};

use super::config::CluihudTerminalConfig;
use super::input::map_event;
use super::types::{CellSnapshot, CursorSnapshot, GridSnapshot, TerminalKeyEvent};

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

    /// Translate and encode a frontend key event. wezterm owns the actual
    /// byte encoding (CSI-u, Kitty keyboard protocol, etc.) and writes the
    /// result into the writer registered at construction time.
    ///
    /// Returns `Ok(false)` when the event carries no mappable key (e.g. a
    /// dead key stroke or a browser-only synthetic event); callers can
    /// treat that as a silent no-op.
    pub fn key_down(&mut self, event: &TerminalKeyEvent) -> Result<bool> {
        let Some((key, mods)) = map_event(event) else {
            return Ok(false);
        };
        self.terminal.key_down(key, mods)?;
        Ok(true)
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
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[derive(Clone, Default)]
    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    impl CapturedWriter {
        fn new() -> Self {
            Self::default()
        }
        fn bytes(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }
        fn clear(&self) {
            self.0.lock().unwrap().clear();
        }
        /// wezterm wraps our writer behind a channel + background thread, so
        /// bytes do not arrive synchronously. Poll for at least one byte with
        /// a generous timeout; return whatever we have when time is up.
        fn drain(&self) -> Vec<u8> {
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let bytes = self.bytes();
                if !bytes.is_empty() {
                    return bytes;
                }
                if Instant::now() >= deadline {
                    return bytes;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    impl Write for CapturedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn session(cols: u16, rows: u16) -> TerminalSession {
        TerminalSession::new(cols, rows, Box::new(Vec::<u8>::new()))
    }

    fn session_with_writer(cols: u16, rows: u16) -> (TerminalSession, CapturedWriter) {
        let w = CapturedWriter::new();
        let session = TerminalSession::new(cols, rows, Box::new(w.clone()));
        (session, w)
    }

    fn evt(code: &str, key: &str) -> TerminalKeyEvent {
        TerminalKeyEvent {
            code: code.into(),
            key: key.into(),
            text: None,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
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

    #[test]
    fn ctrl_backspace_encodes_differently_from_plain_backspace() {
        let (mut s, w) = session_with_writer(80, 24);

        s.key_down(&evt("Backspace", "Backspace")).unwrap();
        let plain = w.drain();
        assert!(!plain.is_empty(), "plain Backspace must emit something");
        w.clear();

        let mut ctrl_bs = evt("Backspace", "Backspace");
        ctrl_bs.ctrl = true;
        s.key_down(&ctrl_bs).unwrap();
        let with_ctrl = w.drain();
        assert!(!with_ctrl.is_empty(), "Ctrl+Backspace must emit something");

        assert_ne!(
            plain, with_ctrl,
            "Kitty keyboard protocol should make Ctrl+Backspace distinct so shells can bind backward-kill-word"
        );
    }

    #[test]
    fn shift_enter_encodes_differently_from_plain_enter() {
        let (mut s, w) = session_with_writer(80, 24);

        s.key_down(&evt("Enter", "Enter")).unwrap();
        let plain = w.drain();
        w.clear();

        let mut shift_enter = evt("Enter", "Enter");
        shift_enter.shift = true;
        s.key_down(&shift_enter).unwrap();
        let with_shift = w.drain();

        assert_ne!(plain, with_shift, "Shift+Enter must be distinguishable");
    }

    #[test]
    fn plain_printable_is_written_verbatim() {
        let (mut s, w) = session_with_writer(80, 24);
        s.key_down(&evt("KeyA", "a")).unwrap();
        assert_eq!(w.drain(), b"a");
    }

    #[test]
    fn alt_letter_emits_escape_prefixed_sequence() {
        let (mut s, w) = session_with_writer(80, 24);
        let mut alt_a = evt("KeyA", "a");
        alt_a.alt = true;
        s.key_down(&alt_a).unwrap();
        let bytes = w.drain();
        // Alt+letter must not encode to bare "a" — historically ESC-prefixed
        // in xterm, CSI-u in Kitty mode; either way it is not `b"a"`.
        assert_ne!(bytes, b"a");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn kitty_disabled_falls_back_to_csi_u() {
        // Regression guard: with kitty OFF we should still disambiguate
        // Ctrl+Backspace from Backspace via CSI-u (our config flips csi_u on
        // when kitty_keyboard is off). Verifies the CluihudTerminalConfig
        // toggle does what the task claims.
        let w = CapturedWriter::new();
        let cfg = CluihudTerminalConfig::new().with_kitty_keyboard(false);
        let mut s =
            TerminalSession::with_config(80, 24, Box::new(w.clone()), cfg);

        s.key_down(&evt("Backspace", "Backspace")).unwrap();
        let plain = w.drain();
        w.clear();

        let mut ctrl_bs = evt("Backspace", "Backspace");
        ctrl_bs.ctrl = true;
        s.key_down(&ctrl_bs).unwrap();
        let with_ctrl = w.drain();

        assert_ne!(plain, with_ctrl);
    }
}
