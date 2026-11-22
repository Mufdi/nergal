// API assumed for the Builder (src/pty.rs, private, reached through `super::*`):
//   validate_grid_size(cols: u16, rows: u16) -> Result<(), String>
//     called first by resize_session_terminal, start_claude_session and spawn_aux_shell
//     (those need an AppHandle, so the validator is the testable seam).
//   resize_pty rejects 0 columns or 0 rows itself, before touching the master or the emulator.
use super::*;

fn insert_instance(mgr: &PtyManager, id: &str) {
    let size = pty_size_for(80, 24, None, None);
    let pair = NativePtySystem::default().openpty(size).unwrap();
    let mut cmd = CommandBuilder::new("sh");
    cmd.arg("-c");
    cmd.arg("sleep 30");
    let child = pair.slave.spawn_command(cmd).unwrap();
    let child_pid = child.process_id();
    drop(pair.slave);
    let writer: SharedWriter = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    let session = TerminalSession::new_with_pixels(80, 24, None, None, Box::new(std::io::sink()));
    let instance = PtyInstance {
        writer,
        master: pair.master,
        child_pid,
        child: Some(child),
        terminal: TerminalHandle::new(session),
    };
    mgr.instances
        .lock()
        .unwrap()
        .insert(id.to_string(), instance);
}

fn reap(mgr: &PtyManager, id: &str) {
    let mut instance = mgr.instances.lock().unwrap().remove(id).unwrap();
    if let Some(mut child) = instance.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn master_grid(mgr: &PtyManager, id: &str) -> (u16, u16) {
    let size = mgr.instances.lock().unwrap()[id].master.get_size().unwrap();
    (size.cols, size.rows)
}

#[test]
fn resize_pty_rejects_zero_cols_and_keeps_the_master_size() {
    let mgr = PtyManager::new();
    insert_instance(&mgr, "zc");
    assert!(resize_pty(&mgr, "zc", 0, 24, Some(640), Some(384)).is_err());
    assert_eq!(master_grid(&mgr, "zc"), (80, 24));
    reap(&mgr, "zc");
}

#[test]
fn resize_pty_rejects_zero_rows_and_keeps_the_master_size() {
    let mgr = PtyManager::new();
    insert_instance(&mgr, "zr");
    assert!(resize_pty(&mgr, "zr", 80, 0, Some(640), Some(384)).is_err());
    assert_eq!(master_grid(&mgr, "zr"), (80, 24));
    reap(&mgr, "zr");
}

#[test]
fn validate_grid_size_rejects_zero_cols() {
    assert!(validate_grid_size(0, 24).is_err());
}

#[test]
fn validate_grid_size_rejects_zero_rows() {
    assert!(validate_grid_size(80, 0).is_err());
}

#[test]
fn validate_grid_size_accepts_a_normal_grid() {
    assert!(validate_grid_size(80, 24).is_ok());
}
