use super::*;

fn session() -> TerminalSession {
    let mut s = TerminalSession::new(80, 24, Box::new(std::io::sink()));
    s.set_status_reply(Box::new(std::io::sink()));
    s
}

#[test]
fn a_query_chunk_is_reported_once_then_forgotten() {
    let mut s = session();
    s.advance_bytes(b"\x1b]7501;?\x1b\\");

    assert!(s.take_query_seen());
    assert!(!s.take_query_seen());
}

#[test]
fn a_report_only_chunk_is_not_a_query() {
    let mut s = session();
    s.advance_bytes(b"\x1b]7501;state=working\x1b\\");

    assert!(!s.take_query_seen());
}
