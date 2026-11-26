// `build_terminal_session` takes the `SharedWriter` itself (not a boxed writer), so the
// emulator and the status reply can share one write queue over it.
use super::*;
use crate::agents::state::StatusChange;
use crate::terminal::program_status::{ProgramStatus, RootState};
use serde_json::json;
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
    fn drain_len(&self, len: usize) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let bytes = self.0.lock().unwrap().clone();
            if bytes.len() >= len || Instant::now() >= deadline {
                return bytes;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn key(code: &str, name: &str) -> TerminalKeyEvent {
    TerminalKeyEvent {
        code: code.into(),
        key: name.into(),
        text: None,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

fn status(state: RootState, kind: Option<&str>, msg: Option<&str>) -> ProgramStatus {
    ProgramStatus {
        state,
        kind: kind.map(str::to_string),
        msg: msg.map(str::to_string),
    }
}

#[test]
fn keys_and_the_osc_7501_answer_both_reach_the_shared_writer_through_the_queue() {
    let inner = CapturedWriter::default();
    let shared: SharedWriter = Arc::new(Mutex::new(Box::new(inner.clone())));
    let mut session = build_terminal_session(80, 24, None, None, shared);

    session.key_down(&key("Enter", "Enter")).unwrap();
    assert_eq!(inner.drain_len(1), b"\r");

    let reply = b"\x1b]7501;?\x1b\\";
    session.advance_bytes(b"\x1b]7501;?\x1b\\");
    // wezterm's own writer thread sits in front of the queue, so no order between a key and an
    // answer is guaranteed; only that both arrive.
    let all = inner.drain_len(1 + reply.len());
    assert_eq!(all.len(), 1 + reply.len());
    assert!(contains(&all, b"\r"));
    assert!(contains(&all, reply));
}

#[test]
fn the_payload_of_a_blocked_status_carries_state_kind_and_msg() {
    let change = StatusChange {
        previous: None,
        current: Some(status(
            RootState::Blocked,
            Some("permission"),
            Some("Allow Bash?"),
        )),
    };
    assert_eq!(
        status_payload("sess-1", &change),
        json!({
            "session_id": "sess-1",
            "state": "blocked",
            "kind": "permission",
            "msg": "Allow Bash?",
        })
    );
}

#[test]
fn the_payload_of_a_status_without_kind_or_msg_has_nulls() {
    let change = StatusChange {
        previous: Some(status(RootState::Idle, None, None)),
        current: Some(status(RootState::Working, None, None)),
    };
    assert_eq!(
        status_payload("sess-2", &change),
        json!({
            "session_id": "sess-2",
            "state": "working",
            "kind": null,
            "msg": null,
        })
    );
}

#[test]
fn the_payload_of_a_removed_status_has_a_null_state() {
    let change = StatusChange {
        previous: Some(status(
            RootState::Blocked,
            Some("question"),
            Some("Pick one"),
        )),
        current: None,
    };
    assert_eq!(
        status_payload("sess-3", &change),
        json!({
            "session_id": "sess-3",
            "state": null,
            "kind": null,
            "msg": null,
        })
    );
}
