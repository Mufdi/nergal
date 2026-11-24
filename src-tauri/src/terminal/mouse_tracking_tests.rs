use super::*;
use crate::terminal::config::NergalTerminalConfig;
use std::sync::{Arc, RwLock};

fn session() -> TerminalSession {
    TerminalSession::with_config(
        80,
        24,
        Box::new(std::io::sink()),
        NergalTerminalConfig::with_colors(Arc::new(RwLock::new(None))),
    )
}

#[test]
fn fresh_session_does_not_report_mouse_tracking() {
    assert!(!session().grid_snapshot().mouse_tracking);
}

#[test]
fn x10_mode_1000_toggles_mouse_tracking() {
    let mut s = session();
    s.advance_bytes(b"\x1b[?1000h");
    assert!(s.grid_snapshot().mouse_tracking);
    s.advance_bytes(b"\x1b[?1000l");
    assert!(!s.grid_snapshot().mouse_tracking);
}

#[test]
fn button_event_mode_1002_toggles_mouse_tracking() {
    let mut s = session();
    s.advance_bytes(b"\x1b[?1002h");
    assert!(s.grid_snapshot().mouse_tracking);
    s.advance_bytes(b"\x1b[?1002l");
    assert!(!s.grid_snapshot().mouse_tracking);
}

#[test]
fn any_event_mode_1003_toggles_mouse_tracking() {
    let mut s = session();
    s.advance_bytes(b"\x1b[?1003h");
    assert!(s.grid_snapshot().mouse_tracking);
    s.advance_bytes(b"\x1b[?1003l");
    assert!(!s.grid_snapshot().mouse_tracking);
}
