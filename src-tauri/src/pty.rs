use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

struct PtyInstance {
    writer: Box<dyn Write + Send>,
    pair: portable_pty::PtyPair,
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
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

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

                    let _ = app_clone.emit(
                        "pty:output",
                        PtyOutput {
                            id: read_id.clone(),
                            data: buf[..n].to_vec(),
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });

    let instance = PtyInstance { writer, pair };
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

    let pty_id = format!("pty-{}-{}", session_id, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis());

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

        let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
        if let Some(instance) = instances.get_mut(&pty_id) {
            instance
                .writer
                .write_all(cmd.as_bytes())
                .map_err(|e| e.to_string())?;
            instance.writer.flush().map_err(|e| e.to_string())?;
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

    let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
    let Some(instance) = instances.get_mut(&pty_id) else {
        return Err("PTY instance not found".into());
    };
    instance
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())?;
    instance.writer.flush().map_err(|e| e.to_string())?;
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
    let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get_mut(&id).ok_or("PTY not found")?;
    instance
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())?;
    instance.writer.flush().map_err(|e| e.to_string())?;
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
    Ok(())
}

#[tauri::command]
pub fn pty_kill(state: State<'_, PtyManager>, id: String) -> Result<(), String> {
    let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
    instances.remove(&id);
    Ok(())
}
