use super::*;
use std::time::Duration;

#[derive(Clone, Default)]
struct CapturedWriter(Arc<Mutex<Vec<Vec<u8>>>>);

impl Write for CapturedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().push(buf.to_vec());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl CapturedWriter {
    fn chunks(&self) -> Vec<Vec<u8>> {
        self.0.lock().unwrap().clone()
    }

    fn bytes(&self) -> Vec<u8> {
        self.chunks().concat()
    }
}

// The writer is a capture sink instead of the PTY master so the test reads exactly what the agent would receive.
fn instance_with_captured_writer() -> (PtyInstance, CapturedWriter) {
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = NativePtySystem::default().openpty(size).unwrap();
    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg("sleep 30");
    let child = pair.slave.spawn_command(cmd).unwrap();
    let child_pid = child.process_id();
    drop(pair.slave);

    let captured = CapturedWriter::default();
    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(captured.clone())));
    let session = TerminalSession::new(size.cols, size.rows, Box::new(std::io::sink()));
    let instance = PtyInstance {
        writer,
        master: pair.master,
        child_pid,
        child: Some(child),
        terminal: TerminalHandle::new(session),
    };
    (instance, captured)
}

fn register(mgr: &PtyManager, session_id: &str, pty_id: &str, instance: PtyInstance) {
    mgr.instances
        .lock()
        .unwrap()
        .insert(pty_id.to_string(), instance);
    mgr.session_ptys
        .lock()
        .unwrap()
        .insert(session_id.to_string(), pty_id.to_string());
}

fn reap(mgr: &PtyManager, pty_id: &str) {
    let mut instance = mgr.instances.lock().unwrap().remove(pty_id).unwrap();
    if let Some(mut child) = instance.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[tokio::test]
async fn deliver_prompt_refuses_an_aux_shell_id_and_writes_nothing() {
    let mgr = PtyManager::new();
    let (instance, captured) = instance_with_captured_writer();
    register(&mgr, "sess::shell", "pty-aux", instance);

    let result = deliver_prompt(&mgr, "sess::shell", "hello", Duration::from_millis(10)).await;

    assert!(result.is_err());
    assert!(captured.bytes().is_empty());
    reap(&mgr, "pty-aux");
}

#[tokio::test]
async fn deliver_prompt_errors_when_the_session_has_no_pty() {
    let mgr = PtyManager::new();

    let result = deliver_prompt(&mgr, "sess-1", "hello", Duration::from_millis(10)).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn deliver_prompt_pastes_then_submits_with_the_cr_last() {
    let mgr = PtyManager::new();
    let (instance, captured) = instance_with_captured_writer();
    register(&mgr, "sess-1", "pty-1", instance);

    let result = deliver_prompt(&mgr, "sess-1", "x\ny", Duration::from_millis(10)).await;

    assert!(result.is_ok());
    assert_eq!(captured.bytes(), b"\x1b[200~x\ny\x1b[201~\r".to_vec());
    // A CR in the same write as the paste-end marker can land before the TUI leaves paste mode.
    let chunks = captured.chunks();
    assert!(chunks.len() >= 2);
    assert_eq!(chunks.last().unwrap().as_slice(), b"\r");
    reap(&mgr, "pty-1");
}

#[test]
fn paste_submit_settle_is_250_ms() {
    assert_eq!(PASTE_SUBMIT_SETTLE, Duration::from_millis(250));
}
