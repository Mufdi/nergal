use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

use crate::terminal::{CluihudTerminalConfig, TerminalHandle, TerminalSession};

/// Shared PTY writer. Both wezterm-term (for answerbacks and key encoding,
/// once Phase 3 lands) and the legacy `pty_write` path hand out bytes; a
/// single Mutex keeps writes serialized.
type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Adapter that implements `std::io::Write` on top of a `SharedWriter`, so
/// it can be handed to `wezterm_term::Terminal::new` as its writer sink.
struct SharedWriterAdapter(SharedWriter);

impl Write for SharedWriterAdapter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|e| std::io::Error::other(e.to_string()))?
            .write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0
            .lock()
            .map_err(|e| std::io::Error::other(e.to_string()))?
            .flush()
    }
}

struct PtyInstance {
    writer: SharedWriter,
    pair: portable_pty::PtyPair,
    /// Present only while the dual-emission wezterm path is live (Phase 2+).
    /// Dropping it shuts the emitter task down.
    terminal: Option<TerminalHandle>,
}

pub struct PtyManager {
    instances: Mutex<HashMap<String, PtyInstance>>,
    /// Maps session_id -> pty_id for idempotency
    session_ptys: Mutex<HashMap<String, String>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
            session_ptys: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct PtyOutput {
    id: String,
    data: Vec<u8>,
}

#[derive(Clone, serde::Serialize)]
pub struct StartClaudeResult {
    pty_id: String,
}

/// Internal: spawn a PTY, wire up the reader thread, store the instance.
/// Returns the pty_id on success.
///
/// When `session_id` is provided, a [`TerminalHandle`] is attached so that
/// incoming PTY bytes are also parsed by wezterm-term and emitted as
/// `terminal:grid-update` events — the legacy `pty:output` byte stream keeps
/// flowing in parallel during the dual-emission migration window.
fn spawn_pty(
    app: &AppHandle,
    state: &PtyManager,
    pty_id: String,
    cols: u16,
    rows: u16,
    cwd: Option<&str>,
    session_id: Option<&str>,
    shell_ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), String> {
    let pty_system = NativePtySystem::default();

    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");

    if let Some(dir) = cwd {
        cmd.cwd(dir);
        cmd.env("MISE_TRUSTED_CONFIG_PATHS", dir);
    }

    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    if let Some(sid) = session_id {
        cmd.env("CLUIHUD_SESSION_ID", sid);
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let raw_writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let writer: SharedWriter = Arc::new(Mutex::new(raw_writer));

    let terminal = if let Some(sid) = session_id {
        let session = TerminalSession::with_config(
            cols,
            rows,
            Box::new(SharedWriterAdapter(Arc::clone(&writer))),
            CluihudTerminalConfig::new(),
        );
        let mut handle = TerminalHandle::new(session);
        handle.spawn_emitter(app.clone(), sid.to_owned());
        Some(handle)
    } else {
        None
    };

    let reader_session = terminal.as_ref().map(|h| Arc::clone(&h.session));
    let reader_notify = terminal.as_ref().map(|h| Arc::clone(&h.notify));

    let read_id = pty_id.clone();
    let app_clone = app.clone();
    let ready_tx = Mutex::new(shell_ready_tx);

    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // On first read, signal shell ready
                    if let Ok(mut guard) = ready_tx.lock()
                        && let Some(tx) = guard.take()
                    {
                        let _ = tx.send(());
                    }

                    let chunk = &buf[..n];

                    // Feed wezterm-term in parallel with the legacy byte stream.
                    // This runs briefly; lock contention with the async emitter
                    // task is minimal because the emitter also locks briefly.
                    if let (Some(session), Some(notify)) =
                        (reader_session.as_ref(), reader_notify.as_ref())
                    {
                        match session.lock() {
                            Ok(mut guard) => guard.advance_bytes(chunk),
                            Err(err) => {
                                tracing::error!(error = %err, "session mutex poisoned; dropping chunk");
                            }
                        }
                        notify.notify_one();
                    }

                    let _ = app_clone.emit(
                        "pty:output",
                        PtyOutput {
                            id: read_id.clone(),
                            data: chunk.to_vec(),
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });

    let instance = PtyInstance { writer, pair, terminal };
    state
        .instances
        .lock()
        .map_err(|e| e.to_string())?
        .insert(pty_id, instance);

    Ok(())
}

/// Creates a PTY, waits for shell ready, writes `claude\n`, returns pty_id.
/// Idempotent: if session already has a PTY, returns the existing one.
#[tauri::command]
pub async fn start_claude_session(
    app: AppHandle,
    state: State<'_, PtyManager>,
    session_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    resume: Option<String>,
) -> Result<StartClaudeResult, String> {
    // Idempotency: check if session already has a PTY
    {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        if let Some(existing_id) = session_ptys.get(&session_id) {
            return Ok(StartClaudeResult {
                pty_id: existing_id.clone(),
            });
        }
    }

    let pty_id = format!(
        "pty-{}-{}",
        session_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    spawn_pty(
        &app,
        &state,
        pty_id.clone(),
        cols,
        rows,
        cwd.as_deref(),
        Some(session_id.as_str()),
        Some(ready_tx),
    )?;

    // Register session -> pty mapping
    state
        .session_ptys
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id, pty_id.clone());

    // Wait for shell to produce first output (ready), with timeout
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx).await;

    // Small delay to let shell finish initialization prompts
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    {
        // Leading space guards against first-byte loss from PTY race conditions
        // (SIGWINCH during readline init). If lost, the space is sacrificed, not the "c".
        // If not lost, zsh treats leading-space commands normally (just skips history).
        let cmd = match resume.as_deref() {
            Some("continue") => " claude --continue\n".to_string(),
            Some("resume_pick") => " claude --resume\n".to_string(),
            _ => " claude\n".to_string(),
        };

        let instances = state.instances.lock().map_err(|e| e.to_string())?;
        if let Some(instance) = instances.get(&pty_id) {
            let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
            w.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }
    }

    Ok(StartClaudeResult { pty_id })
}

/// Write data to a session's PTY
#[tauri::command]
pub fn write_to_session_pty(
    state: State<'_, PtyManager>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
    let Some(pty_id) = session_ptys.get(&session_id) else {
        return Err("no PTY for session".into());
    };
    let pty_id = pty_id.clone();
    drop(session_ptys);

    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let Some(instance) = instances.get(&pty_id) else {
        return Err("PTY instance not found".into());
    };
    let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
    w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Kill a session's PTY and clean up mappings
#[tauri::command]
pub fn kill_session_pty(
    state: State<'_, PtyManager>,
    session_id: String,
) -> Result<(), String> {
    let pty_id = {
        let mut session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys.remove(&session_id)
    };

    if let Some(id) = pty_id {
        let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
        instances.remove(&id);
    }

    Ok(())
}

#[tauri::command]
pub fn pty_create(
    app: AppHandle,
    state: State<'_, PtyManager>,
    id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
) -> Result<(), String> {
    spawn_pty(&app, &state, id, cols, rows, cwd.as_deref(), None, None)
}

#[tauri::command]
pub fn pty_write(state: State<'_, PtyManager>, id: String, data: String) -> Result<(), String> {
    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&id).ok_or("PTY not found")?;
    let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
    w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn pty_resize(
    state: State<'_, PtyManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&id).ok_or("PTY not found")?;
    instance
        .pair
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    // Keep the wezterm emulator in sync with the PTY ioctl resize so its
    // internal grid matches the shell's view.
    if let Some(handle) = instance.terminal.as_ref() {
        match handle.session.lock() {
            Ok(mut guard) => guard.resize(cols, rows),
            Err(err) => tracing::error!(error = %err, "session mutex poisoned during resize"),
        }
        // Force a full resend on next emission — row count may have changed.
        if let Ok(mut differ) = handle.differ.lock() {
            differ.invalidate();
        }
        handle.wake();
    }

    Ok(())
}

#[tauri::command]
pub fn pty_kill(state: State<'_, PtyManager>, id: String) -> Result<(), String> {
    let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
    instances.remove(&id);
    Ok(())
}

/// Return the current full grid for a session. Invalidates the differ so the
/// next `terminal:grid-update` delta will also be a full resend — ensuring
/// the frontend can sync state deterministically on mount/reload.
#[tauri::command]
pub fn terminal_get_full_grid(
    state: State<'_, PtyManager>,
    session_id: String,
) -> Result<crate::terminal::GridUpdate, String> {
    let pty_id = {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "no PTY for session".to_string())?
    };

    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&pty_id).ok_or("PTY instance not found")?;
    let handle = instance
        .terminal
        .as_ref()
        .ok_or("terminal handle not attached")?;

    let snapshot = handle
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .grid_snapshot();

    let mut differ = handle.differ.lock().map_err(|e| e.to_string())?;
    differ.invalidate();
    differ
        .compute_update(&session_id, &snapshot)
        .ok_or_else(|| "differ produced no update after invalidate (unreachable)".to_string())
}
