// API assumed for the Builder (src/pty.rs, private, reached through `super::*`):
//   pty_size_for(cols: u16, rows: u16, pixel_width: Option<u16>, pixel_height: Option<u16>) -> PtySize
//     built from terminal::session::clamp_pixel_geometry, shared by spawn_pty and resize_pty.
//   resize_pty(state, pty_id, cols, rows, pixel_width: Option<u16>, pixel_height: Option<u16>) -> Result<(), String>
//   TerminalSession::new_with_pixels(cols, rows, pw, ph, writer) (see terminal/pixel_geometry_tests.rs)
use super::*;
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

// The emulator writes to a captured sink instead of the PTY so the test can read its answerbacks.
fn instance_with_captured_emulator(
    size: PtySize,
    pw: Option<u16>,
    ph: Option<u16>,
) -> (PtyInstance, CapturedWriter) {
    let pair = NativePtySystem::default().openpty(size).unwrap();
    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg("sleep 30");
    let child = pair.slave.spawn_command(cmd).unwrap();
    let child_pid = child.process_id();
    drop(pair.slave);

    let writer: SharedWriter = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    let captured = CapturedWriter::default();
    let session =
        TerminalSession::new_with_pixels(size.cols, size.rows, pw, ph, Box::new(captured.clone()));
    let instance = PtyInstance {
        writer,
        master: pair.master,
        child_pid,
        child: Some(child),
        terminal: TerminalHandle::new(session),
    };
    (instance, captured)
}

fn reap(mgr: &PtyManager, pty_id: &str) {
    let mut instance = mgr.instances.lock().unwrap().remove(pty_id).unwrap();
    if let Some(mut child) = instance.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn answer_14t(mgr: &PtyManager, pty_id: &str, captured: &CapturedWriter) -> String {
    {
        let instances = mgr.instances.lock().unwrap();
        let mut session = instances[pty_id].terminal.session.lock().unwrap();
        session.advance_bytes(b"\x1b[14t");
    }
    captured.drain()
}

#[test]
fn spawn_size_carries_the_clamped_pixel_geometry_into_the_pty() {
    let given = pty_size_for(80, 24, Some(800), Some(480));
    assert_eq!((given.pixel_width, given.pixel_height), (800, 480));

    let fallback = pty_size_for(80, 24, None, None);
    assert_eq!((fallback.pixel_width, fallback.pixel_height), (640, 384));

    let pair = NativePtySystem::default().openpty(given).unwrap();
    let reported = pair.master.get_size().unwrap();
    assert_eq!((reported.pixel_width, reported.pixel_height), (800, 480));
}

#[test]
fn resize_pty_applies_the_geometry_to_the_master_and_the_emulator() {
    let mgr = PtyManager::new();
    let (instance, captured) =
        instance_with_captured_emulator(pty_size_for(80, 24, None, None), None, None);
    mgr.instances
        .lock()
        .unwrap()
        .insert("p".to_string(), instance);

    resize_pty(&mgr, "p", 100, 30, Some(1000), Some(600)).unwrap();

    let reported = mgr.instances.lock().unwrap()["p"]
        .master
        .get_size()
        .unwrap();
    assert_eq!(
        (
            reported.cols,
            reported.rows,
            reported.pixel_width,
            reported.pixel_height
        ),
        (100, 30, 1000, 600)
    );
    assert_eq!(answer_14t(&mgr, "p", &captured), "\x1b[4;600;1000t");
    reap(&mgr, "p");
}
