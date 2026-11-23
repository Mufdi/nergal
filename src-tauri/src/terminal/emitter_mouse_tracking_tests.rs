use super::*;
use std::sync::mpsc;
use tauri::ipc::{Channel, InvokeResponseBody};

#[test]
fn attached_channel_receives_mouse_tracking_on_enable() {
    let mut handle = TerminalHandle::new(TerminalSession::new(20, 5, Box::new(std::io::sink())));
    handle.spawn_emitter("s1".to_string());
    let (tx, rx) = mpsc::channel();
    handle.attach(Channel::new(move |body| {
        let _ = tx.send(body);
        Ok(())
    }));

    handle.session.lock().unwrap().advance_bytes(b"\x1b[?1000h");
    handle.wake();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let body = rx
            .recv_timeout(remaining)
            .expect("no update carried mouseTracking:true");
        let InvokeResponseBody::Json(json) = body else {
            panic!("expected a JSON body");
        };
        if json.contains("\"mouseTracking\":true") {
            return;
        }
    }
}
