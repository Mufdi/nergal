// API assumed for the Builder (src/terminal/session.rs, all `pub`):
//   clamp_pixel_geometry(cols: u16, rows: u16, pixel_width: Option<u16>, pixel_height: Option<u16>) -> (u16, u16)
//     fallback cell 8x16 per axis when None or below one pixel per cell; saturates at 65535.
//   TerminalSession::new_with_pixels(cols, rows, pixel_width: Option<u16>, pixel_height: Option<u16>, writer) -> Self
//   TerminalSession::resize_with_pixels(&mut self, cols, rows, pixel_width: Option<u16>, pixel_height: Option<u16>) -> bool
//     true when the clamped size differs from the stored one and the emulator was resized;
//     false is a full no-op (this is also the predicate resize_pty uses to skip SIGWINCH).
// TerminalSession::new / with_config / resize (cols, rows only) must keep compiling.
use super::session::{TerminalSession, clamp_pixel_geometry};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl CapturedWriter {
    // wezterm writes answerbacks from a background thread, so poll instead of reading once.
    fn drain(&self) -> String {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let bytes = std::mem::take(&mut *self.0.lock().unwrap());
            if !bytes.is_empty() || Instant::now() >= deadline {
                return String::from_utf8_lossy(&bytes).into_owned();
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn session(pw: Option<u16>, ph: Option<u16>) -> (TerminalSession, CapturedWriter) {
    let w = CapturedWriter::default();
    let s = TerminalSession::new_with_pixels(80, 24, pw, ph, Box::new(w.clone()));
    (s, w)
}

fn ask(s: &mut TerminalSession, w: &CapturedWriter, query: &[u8]) -> String {
    s.advance_bytes(query);
    w.drain()
}

fn row_text(s: &mut TerminalSession, row: usize) -> String {
    let grid = s.grid_snapshot();
    grid.rows[row]
        .iter()
        .map(|c| c.ch.as_str())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn clamp_uses_fallback_cell_for_missing_or_sub_cell_geometry_and_saturates() {
    let cases: [(u16, u16, Option<u16>, Option<u16>, (u16, u16)); 5] = [
        (80, 24, None, None, (640, 384)),
        (80, 24, Some(0), Some(10), (640, 384)),
        (9000, 24, None, None, (65535, 384)),
        (80, 24, Some(800), Some(480), (800, 480)),
        (80, 24, Some(80), Some(24), (80, 24)),
    ];
    for (cols, rows, pw, ph, expected) in cases {
        assert_eq!(
            clamp_pixel_geometry(cols, rows, pw, ph),
            expected,
            "cols={cols} rows={rows} pw={pw:?} ph={ph:?}"
        );
    }
}

#[test]
fn missing_geometry_answers_the_fallback_canvas_size() {
    let (mut s, w) = session(None, None);
    assert_eq!(ask(&mut s, &w, b"\x1b[14t"), "\x1b[4;384;640t");
}

#[test]
fn geometry_below_one_pixel_per_cell_answers_the_fallback_canvas_size() {
    let (mut s, w) = session(Some(800), Some(480));
    s.resize_with_pixels(80, 24, Some(0), Some(10));
    assert_eq!(ask(&mut s, &w, b"\x1b[14t"), "\x1b[4;384;640t");
}

#[test]
fn spawn_geometry_is_what_the_emulator_reports() {
    let (mut s, w) = session(Some(800), Some(480));
    assert_eq!(ask(&mut s, &w, b"\x1b[14t"), "\x1b[4;480;800t");
}

#[test]
fn cell_size_report_follows_the_geometry() {
    let (mut s, w) = session(Some(800), Some(480));
    assert_eq!(ask(&mut s, &w, b"\x1b[16t"), "\x1b[6;20;10t");
}

#[test]
fn resize_carries_the_new_geometry() {
    let (mut s, w) = session(None, None);
    assert!(s.resize_with_pixels(100, 30, Some(1000), Some(600)));
    assert_eq!(ask(&mut s, &w, b"\x1b[14t"), "\x1b[4;600;1000t");
}

#[test]
fn unchanged_resize_is_a_no_op_that_keeps_the_scroll_region() {
    let (mut s, _w) = session(Some(800), Some(480));
    s.advance_bytes(b"\x1b[5;10r");
    for row in 1..=24 {
        s.advance_bytes(format!("\x1b[{row};1HR{row:02}").as_bytes());
    }

    assert!(
        !s.resize_with_pixels(80, 24, Some(800), Some(480)),
        "same size must report no change so resize_pty skips SIGWINCH"
    );

    s.advance_bytes(b"\x1b[10;1H\n");
    assert_eq!(row_text(&mut s, 4), "R06", "region 5..10 must still scroll");
    assert_eq!(row_text(&mut s, 9), "");
    assert_eq!(row_text(&mut s, 3), "R04");
    assert_eq!(row_text(&mut s, 10), "R11");
}

#[test]
fn cell_size_change_with_same_cols_and_rows_counts_as_a_change() {
    let (mut s, _w) = session(Some(720), Some(432));
    assert!(s.resize_with_pixels(80, 24, Some(800), Some(480)));
}

// 64x48 solid PNG, generated with zlib/struct; the `image` crate is not a direct dependency.
const PNG_64X48_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAEAAAAAwCAIAAAAuKetIAAAAQElEQVR42u3PQQkAAAgAsetfWiv4FQYrsKZeS0BAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEDgagGlwfSmsRz7tgAAAABJRU5ErkJggg==";

#[test]
fn iterm2_image_with_less_than_one_pixel_per_cell_does_not_panic() {
    let (mut s, _w) = session(None, None);
    s.resize_with_pixels(80, 24, Some(10), Some(10));
    let seq = format!("\x1b]1337;File=inline=1;size=121:{PNG_64X48_B64}\x07");
    s.advance_bytes(seq.as_bytes());
    assert_eq!(s.grid_snapshot().rows.len(), 24);
}
