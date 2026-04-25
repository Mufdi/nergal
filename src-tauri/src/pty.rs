use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

use crate::terminal::{CluihudTerminalConfig, TerminalHandle, TerminalKeyEvent, TerminalSession};

/// Shared PTY writer. wezterm-term (for answerbacks + key encoding) and the
/// small number of command-layer writers (`start_claude_session` boot
/// command, `write_to_session_pty`, bracketed paste) all serialize writes
/// through this mutex.
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
    /// Dropping this handle shuts down the emitter task.
    terminal: TerminalHandle,
}

pub struct PtyManager {
    instances: Mutex<HashMap<String, PtyInstance>>,
    /// Maps session_id -> pty_id for idempotency
    session_ptys: Mutex<HashMap<String, String>>,
    /// Value of `config.terminal_kitty_keyboard` at startup. Applied to every
    /// new `TerminalSession`; runtime toggling would require restarting PTYs.
    kitty_keyboard: bool,
}

impl PtyManager {
    pub fn new(kitty_keyboard: bool) -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
            session_ptys: Mutex::new(HashMap::new()),
            kitty_keyboard,
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct StartClaudeResult {
    pty_id: String,
}

/// Internal: spawn a PTY, attach a wezterm-term session, spawn the emitter
/// task, wire up the reader thread, store the instance.
fn spawn_pty(
    app: &AppHandle,
    state: &PtyManager,
    pty_id: String,
    cols: u16,
    rows: u16,
    cwd: Option<&str>,
    session_id: &str,
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
    cmd.env("CLUIHUD_SESSION_ID", session_id);

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let raw_writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let writer: SharedWriter = Arc::new(Mutex::new(raw_writer));

    let config = CluihudTerminalConfig::new().with_kitty_keyboard(state.kitty_keyboard);
    let session = TerminalSession::with_config(
        cols,
        rows,
        Box::new(SharedWriterAdapter(Arc::clone(&writer))),
        config,
    );
    let mut terminal = TerminalHandle::new(session);
    terminal.spawn_emitter(app.clone(), session_id.to_owned());

    let reader_session = Arc::clone(&terminal.session);
    let reader_notify = Arc::clone(&terminal.notify);
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

                    match reader_session.lock() {
                        Ok(mut guard) => guard.advance_bytes(&buf[..n]),
                        Err(err) => {
                            tracing::error!(error = %err, "session mutex poisoned; dropping chunk");
                        }
                    }
                    reader_notify.notify_one();
                }
                Err(_) => break,
            }
        }
    });

    let instance = PtyInstance {
        writer,
        pair,
        terminal,
    };
    state
        .instances
        .lock()
        .map_err(|e| e.to_string())?
        .insert(pty_id, instance);

    Ok(())
}

/// Internal: resize both the PTY ioctl and the wezterm emulator grid.
fn resize_pty(state: &PtyManager, pty_id: &str, cols: u16, rows: u16) -> Result<(), String> {
    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(pty_id).ok_or("PTY not found")?;
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

    // Keep the wezterm emulator in sync with the PTY so its internal grid
    // matches the shell's view.
    match instance.terminal.session.lock() {
        Ok(mut guard) => guard.resize(cols, rows),
        Err(err) => tracing::error!(error = %err, "session mutex poisoned during resize"),
    }
    // Force a full resend on next emission — row count may have changed.
    if let Ok(mut differ) = instance.terminal.differ.lock() {
        differ.invalidate();
    }
    instance.terminal.wake();

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
        session_id.as_str(),
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

/// Write data to a session's PTY. Used for sending synthesized prompts
/// from the sidebar ("/commit …", "/rename …", merge-conflict hints).
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

/// Kill a session's PTY and clean up mappings.
#[tauri::command]
pub fn kill_session_pty(state: State<'_, PtyManager>, session_id: String) -> Result<(), String> {
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

/// Resize the PTY + wezterm emulator for a session. The frontend holds the
/// session_id but not the opaque pty_id, so this is the only resize entry
/// point exposed over IPC.
#[tauri::command]
pub fn resize_session_terminal(
    state: State<'_, PtyManager>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let pty_id = {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "no PTY for session".to_string())?
    };
    resize_pty(&state, &pty_id, cols, rows)
}

/// Write `text` to the OS clipboard from a blocking worker thread. The
/// clipboard plugin's own `write_text` is an `async fn` that invokes
/// `arboard::Clipboard::set_text` synchronously, and on Wayland that call
/// issues blocking I/O to `wl-clipboard`. When it stalls (e.g. compositor
/// pressure), it holds a tokio worker and delays every other async task
/// sharing that thread — including the user's next keydown-triggered
/// IPC. `spawn_blocking` gives us a dedicated thread so nothing else is
/// affected.
#[tauri::command]
pub async fn terminal_clipboard_write(text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.set_text(text).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Paste `text` into the session's PTY wrapped in bracketed-paste markers.
/// Applications that understand bracketed paste (bash, zsh, vim, claude CLI,
/// etc.) can then treat it as a single paste operation instead of a flurry
/// of keystrokes — most importantly they skip autoindent / tab-expansion on
/// the pasted body.
#[tauri::command]
pub fn terminal_paste(
    state: State<'_, PtyManager>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    let pty_id = {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "no PTY for session".to_string())?
    };

    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&pty_id).ok_or("PTY instance not found")?;
    let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
    w.write_all(b"\x1b[200~").map_err(|e| e.to_string())?;
    w.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    w.write_all(b"\x1b[201~").map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Encode a frontend key event via wezterm-term and write the resulting
/// bytes to the PTY. The encoder owns Kitty keyboard protocol, CSI-u,
/// cursor-mode translations, and everything else — the frontend only
/// forwards the raw `KeyboardEvent` properties it captured.
#[tauri::command]
pub fn terminal_input(
    state: State<'_, PtyManager>,
    session_id: String,
    event: TerminalKeyEvent,
) -> Result<(), String> {
    let pty_id = {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "no PTY for session".to_string())?
    };

    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&pty_id).ok_or("PTY instance not found")?;
    let handle = &instance.terminal;

    let mut session = handle.session.lock().map_err(|e| e.to_string())?;
    session.key_down(&event).map_err(|e| e.to_string())?;

    // Nudge the emitter: key events often trigger local echo / cursor moves
    // before any PTY output comes back, and the diff should include them.
    // Note: we deliberately do NOT auto-snap to the live bottom on input —
    // the user often wants to read history while typing (e.g. annotating
    // an old command, replying to a question while reviewing scrollback).
    // Snap-to-bottom is opt-in via Shift+End / Escape.
    drop(session);
    handle.wake();
    Ok(())
}

/// Handle a wheel tick. In alt screen (claude TUI, vim, less, …) we
/// hand the event to wezterm-term so the running app receives a proper
/// mouse report or, for apps without mouse mode, the equivalent
/// arrow-key presses (xterm "alternateScroll" emulation). In the
/// primary screen we navigate our local scrollback — the alt screen
/// has none of its own.
///
/// `delta` follows the same sign convention everywhere: positive = up
/// (older content), negative = down (toward the live bottom). `col`
/// and `row` are the cell coords of the cursor at the time of the
/// event; only used by mouse-mode apps.
#[tauri::command]
pub fn terminal_scroll(
    state: State<'_, PtyManager>,
    session_id: String,
    delta: i32,
    col: u16,
    row: u16,
) -> Result<(), String> {
    let pty_id = {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "no PTY for session".to_string())?
    };

    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&pty_id).ok_or("PTY instance not found")?;
    let handle = &instance.terminal;

    let mut session = handle.session.lock().map_err(|e| e.to_string())?;
    if session.is_alt_screen_active() {
        session
            .mouse_wheel(delta, col, row)
            .map_err(|e| e.to_string())?;
        drop(session);
    } else {
        session.scroll_by(delta);
        drop(session);
        handle
            .differ
            .lock()
            .map_err(|e| e.to_string())?
            .invalidate();
    }
    handle.wake();
    Ok(())
}

/// Snap the scrollback viewport back to the live bottom. Idempotent.
#[tauri::command]
pub fn terminal_scroll_to_bottom(
    state: State<'_, PtyManager>,
    session_id: String,
) -> Result<(), String> {
    let pty_id = {
        let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "no PTY for session".to_string())?
    };

    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&pty_id).ok_or("PTY instance not found")?;
    let handle = &instance.terminal;

    handle
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .scroll_to_bottom();
    handle
        .differ
        .lock()
        .map_err(|e| e.to_string())?
        .invalidate();
    handle.wake();
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
    let handle = &instance.terminal;

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
