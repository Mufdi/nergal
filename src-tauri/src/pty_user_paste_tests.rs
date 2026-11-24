use super::*;
use crate::terminal::config::NergalTerminalConfig;
use std::sync::RwLock;

fn session() -> TerminalSession {
    TerminalSession::with_config(
        80,
        24,
        Box::new(std::io::sink()),
        NergalTerminalConfig::with_colors(Arc::new(RwLock::new(None))),
    )
}

fn tracker() -> AuxLineTracker {
    AuxLineTracker {
        start_col: Some(7),
        prefill_chars: 3,
    }
}

#[test]
fn bracketed_user_paste_wraps_text_in_markers() {
    assert_eq!(encode_user_paste("a\nb", true), "\x1b[200~a\nb\x1b[201~");
}

#[test]
fn plain_user_paste_turns_line_endings_into_carriage_returns() {
    assert_eq!(encode_user_paste("a\r\nb\nc\x1b[201~d", false), "a\rb\rcd");
}

#[test]
fn plain_user_paste_keeps_a_lone_carriage_return_single() {
    assert_eq!(encode_user_paste("\r", false), "\r");
}

#[test]
fn plain_user_paste_collapses_only_the_crlf_pair() {
    assert_eq!(encode_user_paste("\r\r\n", false), "\r\r");
}

#[test]
fn plain_user_paste_strips_markers_that_rejoin_after_removal() {
    assert_eq!(
        encode_user_paste("a\u{9b}201~b\x1b[20\x1b[201~1~c", false),
        "abc"
    );
}

#[test]
fn user_paste_payload_brackets_when_session_enabled_2004() {
    let mut s = session();
    s.advance_bytes(b"\x1b[?2004h");
    assert_eq!(user_paste_payload(&s, "x"), "\x1b[200~x\x1b[201~");
}

#[test]
fn user_paste_payload_is_plain_on_fresh_session() {
    assert_eq!(user_paste_payload(&session(), "x"), "x");
}

#[test]
fn session_reports_bracketed_paste_mode_transitions() {
    let mut s = session();
    assert!(!s.bracketed_paste_enabled());
    s.advance_bytes(b"\x1b[?2004h");
    assert!(s.bracketed_paste_enabled());
    s.advance_bytes(b"\x1b[?2004l");
    assert!(!s.bracketed_paste_enabled());
}

#[test]
fn plain_paste_with_newline_resets_tracker_anchor() {
    let mut t = tracker();
    tracker_after_paste(&mut t, false, "a\nb");
    assert_eq!(t.start_col, None);
    assert_eq!(t.prefill_chars, 0);
}

#[test]
fn bracketed_paste_with_newline_leaves_tracker_unchanged() {
    let mut t = tracker();
    tracker_after_paste(&mut t, true, "a\nb");
    assert_eq!(t.start_col, Some(7));
    assert_eq!(t.prefill_chars, 3);
}

#[test]
fn plain_paste_without_newline_leaves_tracker_unchanged() {
    let mut t = tracker();
    tracker_after_paste(&mut t, false, "abc");
    assert_eq!(t.start_col, Some(7));
    assert_eq!(t.prefill_chars, 3);
}

#[test]
fn agent_write_bracketed_still_wraps_multiline_text() {
    let mut out: Vec<u8> = Vec::new();
    write_bracketed(&mut out, "x\ny", false).unwrap();
    assert_eq!(out, b"\x1b[200~x\ny\x1b[201~");
}

#[test]
fn bracketed_user_paste_strips_embedded_end_marker() {
    assert_eq!(
        encode_user_paste("safe\x1b[201~rm -rf ~\r", true),
        "\x1b[200~saferm -rf ~\r\x1b[201~"
    );
}

#[test]
fn bracketed_user_paste_strips_end_marker_reassembled_by_removal() {
    assert_eq!(
        encode_user_paste("a\x1b[20\x1b[201~1~b", true),
        "\x1b[200~ab\x1b[201~"
    );
}

#[test]
fn bracketed_user_paste_strips_c1_markers() {
    assert_eq!(
        encode_user_paste("a\u{9b}201~b\u{9b}200~c", true),
        "\x1b[200~abc\x1b[201~"
    );
}

#[test]
fn bracketed_user_paste_keeps_unrelated_escape_sequences() {
    assert_eq!(
        encode_user_paste("\x1b[31mred\x1b[0m", true),
        "\x1b[200~\x1b[31mred\x1b[0m\x1b[201~"
    );
}
