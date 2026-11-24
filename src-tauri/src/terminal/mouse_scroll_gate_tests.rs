use super::*;
use crate::terminal::config::NergalTerminalConfig;
use std::io::Write;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    // wezterm may write from a background thread, so a "no bytes" assertion waits out the window.
    fn drain(&self) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_millis(300);
        loop {
            let bytes = std::mem::take(&mut *self.0.lock().unwrap());
            if !bytes.is_empty() || Instant::now() >= deadline {
                return bytes;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn tracking_session_with_history() -> (TerminalSession, Captured) {
    let out = Captured::default();
    let mut s = TerminalSession::with_config(
        80,
        24,
        Box::new(out.clone()),
        NergalTerminalConfig::with_colors(Arc::new(RwLock::new(None))),
    );
    s.advance_bytes(b"\x1b[?1000h");
    for i in 0..200 {
        s.advance_bytes(format!("line {i}\r\n").as_bytes());
    }
    out.drain();
    (s, out)
}

#[test]
fn right_press_is_not_reported_while_scrolled_into_history() {
    let (mut s, out) = tracking_session_with_history();
    assert!(
        s.scroll_by(30) > 0,
        "fixture must have scrollback to scroll into"
    );

    s.mouse_button(
        PrimaryButton::Right,
        PrimaryKind::Press,
        4,
        2,
        MouseMods::default(),
    )
    .unwrap();

    assert!(
        out.drain().is_empty(),
        "viewport rows do not map to live rows in history; nothing may reach the PTY"
    );
}

#[test]
fn right_press_is_reported_at_the_live_bottom() {
    let (mut s, out) = tracking_session_with_history();
    s.scroll_by(30);
    s.scroll_by(-30);
    assert_eq!(s.scroll_offset(), 0);

    s.mouse_button(
        PrimaryButton::Right,
        PrimaryKind::Press,
        4,
        2,
        MouseMods::default(),
    )
    .unwrap();

    assert!(
        !out.drain().is_empty(),
        "live bottom must emit a mouse report"
    );
}

#[test]
fn release_of_a_button_pressed_at_the_bottom_is_reported_after_scrolling_into_history() {
    let (mut s, out) = tracking_session_with_history();
    s.mouse_button(
        PrimaryButton::Right,
        PrimaryKind::Press,
        4,
        2,
        MouseMods::default(),
    )
    .unwrap();
    assert!(
        !out.drain().is_empty(),
        "press at the live bottom must be reported"
    );

    assert!(
        s.scroll_by(30) > 0,
        "fixture must have scrollback to scroll into"
    );
    s.mouse_button(
        PrimaryButton::Right,
        PrimaryKind::Release,
        4,
        2,
        MouseMods::default(),
    )
    .unwrap();

    assert!(
        !out.drain().is_empty(),
        "the release must go out so the app never sees an unbalanced press"
    );
}

#[test]
fn release_with_no_held_button_is_not_reported_while_scrolled_into_history() {
    let (mut s, out) = tracking_session_with_history();
    assert!(
        s.scroll_by(30) > 0,
        "fixture must have scrollback to scroll into"
    );

    s.mouse_button(
        PrimaryButton::Right,
        PrimaryKind::Press,
        4,
        2,
        MouseMods::default(),
    )
    .unwrap();
    assert!(out.drain().is_empty(), "press in history must be gated");

    s.mouse_button(
        PrimaryButton::Right,
        PrimaryKind::Release,
        4,
        2,
        MouseMods::default(),
    )
    .unwrap();

    assert!(
        out.drain().is_empty(),
        "a release with no held button must not reach the PTY"
    );
}
