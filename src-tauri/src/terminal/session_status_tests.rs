use super::*;
use std::time::Duration;

const QUERY: &[u8] = b"\x1b]7501;?\x1b\\";
const REPLY: &[u8] = b"\x1b]7501;?\x1b\\";
const DA1_ANSWER: &[u8] = b"\x1b[?65;4;6;18;22c";

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
    // wezterm answers from its own thread, so wait for the expected length instead of reading once.
    fn drain_len(&self, len: usize) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let bytes = self.0.lock().unwrap().clone();
            if bytes.len() >= len || Instant::now() >= deadline {
                return bytes;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

// The same buffer backs wezterm's writer and the status reply, as the queue does in production.
fn session_with_reply() -> (TerminalSession, CapturedWriter) {
    let w = CapturedWriter::default();
    let mut session = TerminalSession::new(80, 24, Box::new(w.clone()));
    session.set_status_reply(Box::new(w.clone()));
    (session, w)
}

#[test]
fn the_osc_7501_query_is_answered_with_exactly_the_reply() {
    let (mut s, w) = session_with_reply();
    s.advance_bytes(QUERY);
    assert_eq!(w.drain_len(REPLY.len()), REPLY);
}

#[test]
fn a_bel_terminated_query_is_answered_too() {
    let (mut s, w) = session_with_reply();
    s.advance_bytes(b"\x1b]7501;?\x07");
    assert_eq!(w.drain_len(REPLY.len()), REPLY);
}

#[test]
fn the_reply_precedes_the_da1_answer_from_the_same_read() {
    let (mut s, w) = session_with_reply();
    let mut chunk = QUERY.to_vec();
    chunk.extend_from_slice(b"\x1b[c");
    s.advance_bytes(&chunk);

    let mut expected = REPLY.to_vec();
    expected.extend_from_slice(DA1_ANSWER);
    assert_eq!(w.drain_len(expected.len()), expected);
}

#[test]
fn a_query_split_across_two_reads_is_answered_once() {
    let (mut s, w) = session_with_reply();
    s.advance_bytes(b"\x1b]75");
    s.advance_bytes(b"01;?\x07");
    assert_eq!(w.drain_len(REPLY.len()), REPLY);
}

#[test]
fn a_status_report_writes_nothing_to_the_pty() {
    let (mut s, w) = session_with_reply();
    s.advance_bytes(b"\x1b]7501;state=working\x1b\\");
    // A DA1 afterwards makes any earlier stray write visible ahead of its own answer.
    s.advance_bytes(b"\x1b[c");
    assert_eq!(w.drain_len(DA1_ANSWER.len()), DA1_ANSWER);
}

#[test]
fn osc_7501_sequences_are_never_drawn() {
    let (mut s, _w) = session_with_reply();
    s.advance_bytes(b"\x1b]7501;state=blocked:kind=question:msg=SGk=\x1b\\ok");
    let grid = s.grid_snapshot();
    let row: String = grid.rows[0].iter().map(|c| c.ch.as_str()).collect();
    assert!(row.starts_with("ok"), "{row:?}");
    assert_eq!(grid.cursor.x, 2);
    assert_eq!(grid.cursor.y, 0);
}

#[test]
fn advance_bytes_returns_the_report_bodies_and_not_the_queries() {
    let (mut s, _w) = session_with_reply();
    let bodies = s.advance_bytes(
        b"\x1b]7501;state=working:app=claude-code\x1b\\\x1b]7501;?\x07\x1b]7501;state=done\x07",
    );
    assert_eq!(
        bodies,
        vec![
            b"state=working:app=claude-code".to_vec(),
            b"state=done".to_vec()
        ]
    );
}

#[test]
fn a_report_split_across_reads_is_returned_by_the_read_that_completes_it() {
    let (mut s, _w) = session_with_reply();
    assert!(s.advance_bytes(b"\x1b]7501;state=wor").is_empty());
    assert_eq!(
        s.advance_bytes(b"king\x1b\\"),
        vec![b"state=working".to_vec()]
    );
}
