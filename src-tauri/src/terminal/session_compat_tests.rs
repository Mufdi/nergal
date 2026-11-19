use super::*;
use crate::terminal::config::{ColorCell, NergalTerminalConfig, set_colors};
use std::sync::{Mutex, RwLock};
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

const FG: [u8; 3] = [0x18, 0x18, 0x1b];
const WHITE: [u8; 3] = [0xff, 0xff, 0xff];
const DARK_BG: [u8; 3] = [0x09, 0x09, 0x0b];

fn private_cell() -> ColorCell {
    Arc::new(RwLock::new(None))
}

fn colored_session(cell: &ColorCell) -> (TerminalSession, CapturedWriter) {
    let w = CapturedWriter::default();
    let s = TerminalSession::with_config(
        80,
        24,
        Box::new(w.clone()),
        NergalTerminalConfig::with_colors(Arc::clone(cell)),
    );
    (s, w)
}

fn plain_session() -> TerminalSession {
    TerminalSession::with_config(
        80,
        24,
        Box::new(std::io::sink()),
        NergalTerminalConfig::with_colors(private_cell()),
    )
}

#[test]
fn osc_11_query_answers_the_theme_background() {
    let cell = private_cell();
    set_colors(&cell, FG, WHITE);
    let (mut s, w) = colored_session(&cell);
    s.advance_bytes(b"\x1b]11;?\x1b\\");
    let answer = w.drain();
    assert!(answer.contains("rgb:ffff/ffff/ffff"), "{answer:?}");
}

#[test]
fn osc_10_query_answers_the_theme_foreground() {
    let cell = private_cell();
    set_colors(&cell, FG, WHITE);
    let (mut s, w) = colored_session(&cell);
    s.advance_bytes(b"\x1b]10;?\x1b\\");
    let answer = w.drain();
    assert!(answer.contains("rgb:1818/1818/1b1b"), "{answer:?}");
}

#[test]
fn later_set_colors_changes_the_answer_of_a_live_session() {
    let cell = private_cell();
    set_colors(&cell, FG, WHITE);
    let (mut s, w) = colored_session(&cell);
    s.advance_bytes(b"\x1b]11;?\x1b\\");
    assert!(w.drain().contains("rgb:ffff/ffff/ffff"));

    set_colors(&cell, FG, DARK_BG);
    s.advance_bytes(b"\x1b]11;?\x1b\\");
    let answer = w.drain();
    assert!(answer.contains("rgb:0909/0909/0b0b"), "{answer:?}");
}

#[test]
fn app_set_osc_11_keeps_precedence_over_a_new_set_colors() {
    let cell = private_cell();
    set_colors(&cell, FG, WHITE);
    let (mut s, w) = colored_session(&cell);
    s.advance_bytes(b"\x1b]11;rgb:1111/2222/3333\x1b\\");
    set_colors(&cell, FG, DARK_BG);
    s.advance_bytes(b"\x1b]11;?\x1b\\");
    let answer = w.drain();
    assert!(answer.contains("rgb:1111/2222/3333"), "{answer:?}");
}

#[test]
fn default_colored_cells_snapshot_without_fg_or_bg() {
    let cell = private_cell();
    set_colors(&cell, FG, WHITE);
    let (mut s, _w) = colored_session(&cell);
    s.advance_bytes(b"x");
    let grid = s.grid_snapshot();
    let first = &grid.rows[0][0];
    assert_eq!(first.ch, "x");
    assert_eq!(first.fg, None);
    assert_eq!(first.bg, None);
}

#[test]
fn sync_output_is_some_after_mode_set() {
    let mut s = plain_session();
    assert!(s.sync_output_since().is_none());
    s.advance_bytes(b"\x1b[?2026h");
    assert!(s.sync_output_since().is_some());
}

#[test]
fn sync_output_is_none_after_mode_reset() {
    let mut s = plain_session();
    s.advance_bytes(b"\x1b[?2026h");
    s.advance_bytes(b"\x1b[?2026l");
    assert!(s.sync_output_since().is_none());
}

#[test]
fn sync_output_is_none_after_full_reset() {
    let mut s = plain_session();
    s.advance_bytes(b"\x1b[?2026h");
    s.advance_bytes(b"\x1bc");
    assert!(s.sync_output_since().is_none());
}

#[test]
fn sync_output_detects_the_sequence_split_at_every_inner_position() {
    let seq: &[u8] = b"\x1b[?2026h";
    for split in 1..seq.len() {
        let mut s = plain_session();
        s.advance_bytes(&seq[..split]);
        s.advance_bytes(&seq[split..]);
        assert!(
            s.sync_output_since().is_some(),
            "split at {split} lost the mode"
        );
    }
}

#[test]
fn repeated_mode_set_keeps_the_first_instant() {
    let mut s = plain_session();
    s.advance_bytes(b"\x1b[?2026h");
    let first = s.sync_output_since().expect("mode on");
    std::thread::sleep(Duration::from_millis(100));
    s.advance_bytes(b"\x1b[?2026h");
    assert_eq!(s.sync_output_since(), Some(first));
}

#[test]
fn sync_state_handle_mirrors_the_session() {
    let mut s = plain_session();
    let state = s.sync_state();
    s.advance_bytes(b"\x1b[?2026h");
    assert!(state.lock().unwrap().is_some());
    s.advance_bytes(b"\x1b[?2026l");
    assert!(state.lock().unwrap().is_none());
}

#[test]
fn full_reset_split_across_calls_clears_the_mode() {
    let mut s = plain_session();
    s.advance_bytes(b"\x1b[?2026h");
    assert!(s.sync_output_since().is_some());
    s.advance_bytes(b"\x1b");
    s.advance_bytes(b"c");
    assert!(s.sync_output_since().is_none());
}
