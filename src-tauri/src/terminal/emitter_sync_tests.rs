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

fn feed(handle: &TerminalHandle, bytes: &[u8]) -> Instant {
    handle.session.lock().unwrap().advance_bytes(bytes);
    let at = Instant::now();
    handle.wake();
    at
}

fn drain_initial(rx: &Receiver<(Instant, String)>) {
    while rx.recv_timeout(Duration::from_millis(100)).is_ok() {}
}

fn collect_for(rx: &Receiver<(Instant, String)>, window: Duration) -> Vec<(Instant, String)> {
    let deadline = Instant::now() + window;
    let mut out = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(item) = rx.recv_timeout(left) {
            out.push(item);
        }
    }
    out
}

// Cells serialize one glyph each, so a letter is detected by its `"ch"` entry.
fn has_glyph(json: &str, glyph: char) -> bool {
    json.contains(&format!("\"ch\":\"{glyph}\""))
}

#[test]
fn split_frame_is_never_emitted_torn() {
    let (handle, rx) = spawn();
    drain_initial(&rx);

    feed(&handle, b"\x1b[?2026hAAA");
    std::thread::sleep(Duration::from_millis(20));
    feed(&handle, b"BBB\x1b[?2026l");

    let updates = collect_for(&rx, Duration::from_millis(400));
    assert!(
        updates.iter().any(|(_, j)| has_glyph(j, 'B')),
        "the closed frame never arrived"
    );
    for (_, json) in &updates {
        assert!(
            !has_glyph(json, 'A') || has_glyph(json, 'B'),
            "torn update: {json}"
        );
    }
}

#[test]
fn closing_the_mode_flushes_immediately() {
    let (handle, rx) = spawn();
    drain_initial(&rx);

    feed(&handle, b"\x1b[?2026hAAA");
    std::thread::sleep(Duration::from_millis(20));
    let closed_at = feed(&handle, b"BBB\x1b[?2026l");

    let (arrived, json) = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("no update after closing the mode");
    assert!(has_glyph(&json, 'B'), "{json}");
    assert!(
        arrived.duration_since(closed_at) < Duration::from_millis(100),
        "flush took {:?}",
        arrived.duration_since(closed_at)
    );
}

#[test]
fn unclosed_mode_is_released_after_the_cap() {
    let (handle, rx) = spawn();
    drain_initial(&rx);

    let started = feed(&handle, b"\x1b[?2026hAAA");

    let (arrived, _) = loop {
        let item = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("unclosed frame never released");
        if has_glyph(&item.1, 'A') {
            break item;
        }
    };
    let elapsed = arrived.duration_since(started);
    assert!(
        elapsed >= Duration::from_millis(140),
        "too early: {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(600),
        "too late: {elapsed:?}"
    );
}

#[test]
fn plain_output_is_not_delayed() {
    let (handle, rx) = spawn();
    drain_initial(&rx);

    let started = feed(&handle, b"AAA");

    let (arrived, json) = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("plain output never arrived");
    assert!(has_glyph(&json, 'A'), "{json}");
    assert!(
        arrived.duration_since(started) < Duration::from_millis(200),
        "delayed {:?}",
        arrived.duration_since(started)
    );
}

#[test]
fn back_to_back_frame_inside_the_coalesce_window_is_not_stalled() {
    let (handle, rx) = spawn();
    drain_initial(&rx);

    feed(&handle, b"\x1b[?2026h");
    feed(&handle, b"AAA");
    feed(&handle, b"BBB");
    let closed_at = feed(&handle, b"\x1b[?2026l");

    let (arrived, json) = loop {
        let item = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("back-to-back frame never arrived");
        if has_glyph(&item.1, 'A') && has_glyph(&item.1, 'B') {
            break item;
        }
    };
    assert!(
        arrived.duration_since(closed_at) < Duration::from_millis(100),
        "stalled {:?}: {json}",
        arrived.duration_since(closed_at)
    );
}
