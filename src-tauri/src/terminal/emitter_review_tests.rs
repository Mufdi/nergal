use super::*;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;
use tauri::ipc::{Channel, InvokeResponseBody};

fn spawn() -> (TerminalHandle, Receiver<(Instant, String)>) {
    let mut handle = TerminalHandle::new(TerminalSession::new(20, 5, Box::new(std::io::sink())));
    handle.spawn_emitter("s1".to_string());
    let (tx, rx) = mpsc::channel();
    handle.attach(Channel::new(move |body| {
        if let InvokeResponseBody::Json(json) = body {
            let _ = tx.send((Instant::now(), json));
        }
        Ok(())
    }));
    (handle, rx)
}

fn feed(handle: &TerminalHandle, bytes: &[u8]) {
    handle.session.lock().unwrap().advance_bytes(bytes);
    handle.wake();
}

fn has_any_letter(json: &str) -> bool {
    ('A'..='Z').any(|c| json.contains(&format!("\"ch\":\"{c}\"")))
}

// Agents that redraw continuously close and reopen the mode back to back; if
// every l/h pair restarts the hold, the screen freezes for the whole stream.
#[test]
fn sustained_synchronized_streaming_is_not_starved() {
    let (handle, rx) = spawn();
    while rx.recv_timeout(Duration::from_millis(100)).is_ok() {}

    let started = Instant::now();
    feed(&handle, b"\x1b[?2026hA");

    let mut seen: Vec<(Instant, String)> = Vec::new();
    for letter in 'B'..='P' {
        std::thread::sleep(Duration::from_millis(40));
        feed(
            &handle,
            format!("\x1b[?2026l\x1b[?2026h{letter}").as_bytes(),
        );
        seen.extend(rx.try_iter());
    }
    seen.extend(rx.try_iter());

    let first = seen
        .iter()
        .find(|(_, json)| has_any_letter(json))
        .map(|(at, _)| at.duration_since(started));
    match first {
        Some(elapsed) => assert!(
            elapsed <= Duration::from_millis(300),
            "first frame arrived after {elapsed:?}, emitter held until the stream stopped"
        ),
        None => {
            let late = rx
                .recv_timeout(Duration::from_secs(2))
                .map(|(at, _)| at.duration_since(started));
            panic!("no frame during the stream; first one arrived at {late:?}");
        }
    }
}
