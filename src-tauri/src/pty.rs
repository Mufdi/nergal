use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

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
    /// Master side only — the slave is dropped right after spawning the
    /// child. While our process holds a slave fd the master never reaches
    /// EOF, so a shell's `exit` would go undetected forever.
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// Dropping this handle shuts down the emitter task.
    terminal: TerminalHandle,
}

pub struct PtyManager {
    instances: Mutex<HashMap<String, PtyInstance>>,
    /// Maps session_id -> pty_id for idempotency. Aux shells register here
    /// too under `{session_id}::{shell_id}`, so every terminal_* command
    /// addresses both kinds through the same key space.
    session_ptys: Mutex<HashMap<String, String>>,
    /// session_id -> its aux-shell terminal ids, for session teardown.
    aux_shells: Mutex<HashMap<String, Vec<String>>>,
    /// term_id -> tracker for the line being typed in an aux shell, so the
    /// last *submitted* command can persist with the shell's tab def
    /// ("remember the set" across restarts). The command text is read from
    /// the grid at Enter — keystroke mirroring alone misses history recall
    /// and tab completion; only the line-start column is anchored here.
    aux_line_trackers: Mutex<HashMap<String, AuxLineTracker>>,
    /// session_id -> prompt to submit at spawn (deep link `session/new`).
    /// Consumed once by `start_claude_session` when it builds the spawn command.
    pending_prompts: Mutex<HashMap<String, String>>,
    /// Value of `config.terminal_kitty_keyboard` at startup. Applied to every
    /// new `TerminalSession`; runtime toggling would require restarting PTYs.
    kitty_keyboard: bool,
}

impl PtyManager {
    pub fn new(kitty_keyboard: bool) -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
            session_ptys: Mutex::new(HashMap::new()),
            aux_shells: Mutex::new(HashMap::new()),
            aux_line_trackers: Mutex::new(HashMap::new()),
            pending_prompts: Mutex::new(HashMap::new()),
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
///
/// `agent_session` distinguishes the agent terminal from auxiliary (quake)
/// shells: aux shells skip the `CLUIHUD_SESSION_ID` env (an agent manually
/// launched inside one must not route hook events into the owning session's
/// panels) and on EOF emit `shell:exited` instead of the obsidian snapshot.
#[allow(clippy::too_many_arguments)] // Internal helper — every arg is required state for PTY init.
fn spawn_pty(
    app: &AppHandle,
    state: &PtyManager,
    pty_id: String,
    cols: u16,
    rows: u16,
    cwd: Option<&str>,
    session_id: &str,
    agent_session: bool,
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
    if agent_session {
        cmd.env("CLUIHUD_SESSION_ID", session_id);
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

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
    let eof_app = app.clone();
    let eof_session = session_id.to_owned();

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
        if agent_session {
            // PTY closed (shell exited / killed / crashed): a definitive session end
            // that an abnormal exit may have hidden from the SessionEnd hook. Snapshot
            // obsidian once — deduped against the hook by claim_finalization.
            if let Some(db) = eof_app.try_state::<crate::db::SharedDb>() {
                crate::hooks::server::finalize_session_obsidian(db.inner(), Some(&eof_session));
            }
        } else {
            // Emit only for a shell that exited on its own (`exit`, crash):
            // kill paths de-register from session_ptys BEFORE dropping the
            // master, and their EOF must not reach the frontend — it would
            // persist the tab's removal and erase the remembered set.
            let still_registered = eof_app
                .try_state::<PtyManager>()
                .and_then(|m| {
                    m.session_ptys
                        .lock()
                        .ok()
                        .map(|g| g.contains_key(&eof_session))
                })
                .unwrap_or(false);
            if still_registered {
                use tauri::Emitter;
                let _ = eof_app.emit("shell:exited", &eof_session);
            }
        }
    });

    let instance = PtyInstance {
        writer,
        master: pair.master,
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

/// Creates a PTY, waits for shell ready, writes the agent's spawn command,
/// returns pty_id. Idempotent: if session already has a PTY, returns the
/// existing one. The exact command (`claude`, `opencode`, `codex`, …) is
/// produced by the adapter resolved for the session — defaults to the
/// CC adapter for sessions whose agent has not been picked yet (transitional).
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub async fn start_claude_session(
    app: AppHandle,
    state: State<'_, PtyManager>,
    agents: State<'_, crate::agents::state::AgentRuntimeState>,
    db: State<'_, crate::db::SharedDb>,
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
        true,
        Some(ready_tx),
    )?;

    state
        .session_ptys
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id.clone(), pty_id.clone());

    // Wait for shell to produce first output (ready), with timeout
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx).await;

    // Small delay to let shell finish initialization prompts
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    {
        // Leading space guards against first-byte loss from PTY race conditions
        // (SIGWINCH during readline init). If lost, the space is sacrificed, not the binary's first byte.
        // If not lost, zsh treats leading-space commands normally (just skips history).
        //
        // Resolve order:
        //   1. In-memory cache (set on session creation; lives until app exit)
        //   2. DB session row (covers app restart — cache is gone but row persists)
        //   3. CC fallback (defensive — every legacy row has agent_id="claude-code")
        // Without (2), resuming a Pi/OpenCode session after a cluihud restart
        // would silently spawn `claude --continue` because the cache miss
        // falls straight to the default.
        let agent_id = agents
            .resolve(&session_id)
            .or_else(|| {
                let stored = db
                    .lock()
                    .ok()
                    .and_then(|g| g.find_session(&session_id).ok().flatten())
                    .map(|s| s.agent_id);
                let parsed = stored.and_then(|s| crate::agents::AgentId::new(&s).ok());
                if let Some(ref id) = parsed {
                    // Re-populate the cache so subsequent hook events route
                    // through the right adapter without another DB hit.
                    agents.register_session(&session_id, id.clone());
                }
                parsed
            })
            .unwrap_or_else(crate::agents::AgentId::claude_code);
        let adapter = agents
            .registry
            .get(&agent_id)
            .ok_or_else(|| format!("no adapter registered for {agent_id}"))?;

        let cwd_path = cwd
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        // For "continue" intent, prefer the agent-internal session id (e.g. Pi
        // UUID, Codex rollout id) the adapter previously harvested and the
        // runtime persisted to the DB. Some agents' generic `--continue` flag
        // doesn't reliably resolve a session by cwd alone, so passing the
        // exact id avoids "No conversation found" surprises after a cluihud
        // restart.
        let resume_owned: Option<String> = match resume.as_deref() {
            Some("continue") => {
                let stored = db
                    .lock()
                    .ok()
                    .and_then(|g| g.find_session(&session_id).ok().flatten())
                    .and_then(|s| s.agent_internal_session_id);
                Some(stored.unwrap_or_else(|| "continue".to_string()))
            }
            other => other.map(|s| s.to_string()),
        };
        // Deep link `session/new` stashes a prompt for the fresh session; the
        // adapter folds it into the launch command so it submits on spawn
        // without a timing race against the agent's REPL coming up.
        let initial_prompt = state
            .pending_prompts
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&session_id));
        // Pinned vault notes seed the agent's context. Running here covers both
        // fresh and resume spawns through the same path, so resume re-injection
        // is automatic. The adapter's context_injection() tier decides folding.
        let injected_context: Option<String> = db.lock().ok().and_then(|g| {
            let session = g.find_session(&session_id).ok().flatten()?;
            if session.pinned_note_paths.is_empty() {
                return None;
            }
            let vault_root = crate::obsidian::config::resolve(&session.workspace_id, |w| {
                g.get_obsidian_config(w)
            })
            .ok()
            .and_then(|cfg| cfg.vault_root);
            crate::obsidian::pinned_notes::assemble_context(
                &session.pinned_note_paths,
                vault_root.as_deref(),
            )
        });
        // Launch options persist on the session row, so resume re-applies
        // them through this same path (preset flags + startup prelude).
        let launch_options: Option<crate::models::LaunchOptions> = db
            .lock()
            .ok()
            .and_then(|g| g.find_session(&session_id).ok().flatten())
            .and_then(|s| s.launch_options);
        let spawn_ctx = crate::agents::SpawnContext {
            session_id: &session_id,
            cwd: &cwd_path,
            resume_from: resume_owned.as_deref(),
            initial_prompt: initial_prompt.as_deref(),
            injected_context: injected_context.as_deref(),
            launch_options: launch_options.as_ref(),
        };
        let spec = adapter.spawn(&spawn_ctx).map_err(|e| e.to_string())?;

        // Force the cwd at the shell prompt before invoking the agent.
        // `cmd.cwd()` on the spawned shell only sets the *initial* cwd;
        // login shells (`zsh -l`) frequently change directory via .zprofile
        // / .zshrc / chpwd hooks. When that happens, agents like `pi` and
        // `opencode` — which scope sessions by cwd — fail to find a previous
        // conversation on `--continue`. Anchoring the cwd via an explicit
        // `cd` right before the binary closes that gap.
        let cwd_str = cwd_path.display().to_string();
        let cwd_escaped = cwd_str.replace('\'', "'\\''");
        // The user's startup prelude runs between `cd` and the agent binary,
        // verbatim (it's the user's own shell on their own machine — same
        // trust level as typing it). A failing prelude aborts the launch via
        // `&&`, which is the honest outcome: the user asked for setup first.
        let prelude = launch_options
            .as_ref()
            .and_then(|o| o.startup_command.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let mut cmd = match prelude {
            Some(p) => format!(" cd '{cwd_escaped}' && {p} && {}", spec.binary.display()),
            None => format!(" cd '{cwd_escaped}' && {}", spec.binary.display()),
        };
        for arg in &spec.args {
            // Quote args containing whitespace/special chars; for the args we
            // emit today (`--continue`, `--resume`, plain UUIDs) this is a
            // no-op pass-through, but it keeps us safe against future args.
            if arg
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '$' | '`' | '\\'))
            {
                let escaped = arg.replace('\'', "'\\''");
                cmd.push_str(&format!(" '{escaped}'"));
            } else {
                cmd.push(' ');
                cmd.push_str(arg);
            }
        }
        cmd.push('\n');

        {
            let instances = state.instances.lock().map_err(|e| e.to_string())?;
            if let Some(instance) = instances.get(&pty_id) {
                let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
                w.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
                w.flush().map_err(|e| e.to_string())?;
            }
        }

        // Activate the adapter's event pump. CC + Codex are no-ops here
        // (their hook events arrive on the shared Unix socket); OpenCode's
        // SSE consumer spins up here, Pi's JSONL tail starts. Failures are
        // logged rather than propagated — the session is still usable in
        // the terminal even if the event pump can't start (e.g. binary
        // missing for OpenCode); the user just won't see structured
        // events in the activity drawer.
        let sink = agents.event_sink.clone();
        if let Err(e) = adapter.start_event_pump(&session_id, sink).await {
            tracing::warn!(
                session_id = %session_id,
                agent = %agent_id,
                error = %e,
                "adapter.start_event_pump failed; session continues without event pump",
            );
        }

        // The adapter harvests the agent-internal session id (Pi UUID from
        // JSONL header; OpenCode session id from SSE event) asynchronously,
        // so a synchronous read right after start_event_pump usually misses.
        // Spawn a background poller that waits up to ~6s and persists once
        // the id appears. Resuming after a cluihud restart then prefers
        // `--session <id>` over the agent's `--continue` heuristic.
        {
            let adapter_for_persist = adapter.clone();
            let session_id_for_persist = session_id.clone();
            let db_for_persist: crate::db::SharedDb = (*db).clone();
            tauri::async_runtime::spawn(async move {
                for _ in 0..30 {
                    if let Some(id) =
                        adapter_for_persist.agent_internal_session_id(&session_id_for_persist)
                    {
                        if let Ok(g) = db_for_persist.lock()
                            && let Err(e) =
                                g.update_agent_internal_session_id(&session_id_for_persist, &id)
                        {
                            tracing::warn!(
                                session_id = %session_id_for_persist,
                                error = %e,
                                "failed to persist agent_internal_session_id",
                            );
                        }
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            });
        }
    }

    Ok(StartClaudeResult { pty_id })
}

/// Composite terminal id for an auxiliary shell. This is the key the
/// frontend passes as `sessionId` to every terminal_* command, and the
/// channel the shell's grid updates are emitted under.
fn aux_term_id(session_id: &str, shell_id: &str) -> String {
    format!("{session_id}::{shell_id}")
}

/// Spawn an auxiliary (quake) shell: the agent-terminal machinery minus the
/// agent command write. `command` is typed into the fresh prompt — executed
/// when `autorun` (environment shells at session creation), pre-filled
/// otherwise so one Enter re-runs it (re-open after restart, where auto-
/// relaunching heavy processes would be unwanted). Idempotent per
/// `(session_id, shell_id)`.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub async fn spawn_aux_shell(
    app: AppHandle,
    state: State<'_, PtyManager>,
    session_id: String,
    shell_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    command: Option<String>,
    autorun: bool,
) -> Result<String, String> {
    let term_id = aux_term_id(&session_id, &shell_id);

    let pty_id = format!(
        "pty-{}-{}",
        term_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );

    // Check-and-reserve under one lock: concurrent invokes for the same
    // term_id (creation-flow autorun racing the quake view) must not spawn
    // two PTYs, where the second insert would orphan the first as an
    // unkillable zombie.
    {
        let mut session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        if session_ptys.contains_key(&term_id) {
            return Ok(term_id);
        }
        session_ptys.insert(term_id.clone(), pty_id.clone());
    }

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    if let Err(e) = spawn_pty(
        &app,
        &state,
        pty_id.clone(),
        cols,
        rows,
        cwd.as_deref(),
        term_id.as_str(),
        false,
        Some(ready_tx),
    ) {
        if let Ok(mut session_ptys) = state.session_ptys.lock() {
            session_ptys.remove(&term_id);
        }
        return Err(e);
    }

    state
        .aux_shells
        .lock()
        .map_err(|e| e.to_string())?
        .entry(session_id)
        .or_default()
        .push(term_id.clone());

    let command = command
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());
    if let Some(cmd) = command {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Same leading-space guard as the agent launch: sacrifices a space
        // (not the command's first byte) to PTY init races, and keeps the
        // command out of zsh history as a bonus.
        let data = if autorun {
            format!(" {cmd}\n")
        } else {
            format!(" {cmd}")
        };
        if !autorun {
            // Register how many chars we pre-typed ourselves: the tracker
            // must anchor the line start BEFORE them, or an edited pre-fill
            // would snapshot only the typed suffix.
            if let Ok(mut trackers) = state.aux_line_trackers.lock() {
                trackers.insert(
                    term_id.clone(),
                    AuxLineTracker {
                        start_col: None,
                        prefill_chars: cmd.chars().count(),
                    },
                );
            }
        }
        let instances = state.instances.lock().map_err(|e| e.to_string())?;
        if let Some(instance) = instances.get(&pty_id) {
            let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
            w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }
    }

    Ok(term_id)
}

#[tauri::command]
pub fn kill_aux_shell(
    state: State<'_, PtyManager>,
    session_id: String,
    shell_id: String,
) -> Result<(), String> {
    let term_id = aux_term_id(&session_id, &shell_id);
    let pty_id = {
        let mut session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys.remove(&term_id)
    };
    if let Some(id) = pty_id {
        let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
        instances.remove(&id);
    }
    if let Ok(mut trackers) = state.aux_line_trackers.lock() {
        trackers.remove(&term_id);
    }
    let mut aux = state.aux_shells.lock().map_err(|e| e.to_string())?;
    if let Some(ids) = aux.get_mut(&session_id) {
        ids.retain(|t| t != &term_id);
        if ids.is_empty() {
            aux.remove(&session_id);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_aux_shells(
    state: State<'_, PtyManager>,
    session_id: String,
) -> Result<Vec<String>, String> {
    Ok(state
        .aux_shells
        .lock()
        .map_err(|e| e.to_string())?
        .get(&session_id)
        .cloned()
        .unwrap_or_default())
}

fn kill_session_aux_shells(state: &PtyManager, session_id: &str) -> Result<(), String> {
    let term_ids = state
        .aux_shells
        .lock()
        .map_err(|e| e.to_string())?
        .remove(session_id)
        .unwrap_or_default();
    if term_ids.is_empty() {
        return Ok(());
    }
    let mut session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
    let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
    let mut trackers = state.aux_line_trackers.lock().map_err(|e| e.to_string())?;
    for term_id in term_ids {
        if let Some(pty_id) = session_ptys.remove(&term_id) {
            instances.remove(&pty_id);
        }
        trackers.remove(&term_id);
    }
    Ok(())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellCommandPayload {
    term_id: String,
    command: String,
}

/// Per-aux-shell line tracking. `start_col` anchors where the user's input
/// begins on the prompt line: captured from the cursor at the first key of
/// the line — before any echo arrives, so the cursor still sits at the
/// prompt's end. `prefill_chars` discounts text we pre-typed ourselves
/// (re-open pre-fill), which sits on the line before the first user key.
#[derive(Default)]
struct AuxLineTracker {
    start_col: Option<usize>,
    prefill_chars: usize,
}

/// On Enter, read the submitted command off the grid (from the anchored
/// line-start column) and surface it so the frontend persists it with the
/// shell's tab def. Grid extraction — unlike keystroke mirroring — includes
/// history recall and tab completion. Best-effort: wrapped (multi-row)
/// commands truncate to the cursor's row, and alt-screen (TUI) submissions
/// are discarded.
fn track_aux_input(
    app: &AppHandle,
    state: &PtyManager,
    term_id: &str,
    event: &TerminalKeyEvent,
    alt_screen: bool,
    cursor_col: usize,
    line_text: Option<String>,
) {
    let Ok(mut trackers) = state.aux_line_trackers.lock() else {
        return;
    };
    let t = trackers.entry(term_id.to_string()).or_default();

    if event.ctrl || event.alt || event.meta {
        // Ctrl+C / Ctrl+U abandon the line being typed.
        if event.ctrl && (event.code == "KeyC" || event.code == "KeyU") {
            t.start_col = None;
            t.prefill_chars = 0;
        }
        return;
    }
    match event.code.as_str() {
        "Enter" | "NumpadEnter" => {
            // Enter as the line's first key (pre-fill accepted untouched):
            // the cursor still sits at the end of the prefill.
            let col = t
                .start_col
                .take()
                .unwrap_or_else(|| cursor_col.saturating_sub(t.prefill_chars));
            t.prefill_chars = 0;
            if alt_screen {
                return;
            }
            let Some(line) = line_text else { return };
            let cmd: String = line.chars().skip(col).collect();
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                use tauri::Emitter;
                let _ = app.emit(
                    "shell:command",
                    &ShellCommandPayload {
                        term_id: term_id.to_string(),
                        command: cmd.to_string(),
                    },
                );
            }
        }
        _ => {
            if t.start_col.is_none() {
                t.start_col = Some(cursor_col.saturating_sub(t.prefill_chars));
            }
        }
    }
}

/// Write data to a session's PTY. Used for sending synthesized prompts
/// from the sidebar ("/commit …", "/rename …", merge-conflict hints).
fn write_session_data(state: &PtyManager, session_id: &str, data: &str) -> Result<(), String> {
    let session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
    let Some(pty_id) = session_ptys.get(session_id) else {
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

#[tauri::command]
pub fn write_to_session_pty(
    state: State<'_, PtyManager>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    write_session_data(&state, &session_id, &data)
}

/// Drop terminal control bytes (ESC-introduced sequences + raw C0/C1 controls,
/// keeping `\n`/`\t`) before injecting a file body into the PTY, so a crafted
/// note can't drive the agent's terminal via escape codes.
fn sanitize_for_pty(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                // Skip a CSI/OSC-style sequence's parameter+intermediate bytes
                // up to and including its final byte (best-effort).
                for n in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&n) {
                        break;
                    }
                }
            }
            '\n' | '\t' => out.push(c),
            '\x00'..='\x1f' | '\x7f' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Re-read a pinned note and inject its current body into the live PTY as a
/// labeled block (N2 hot reload). Triggered only by the explicit toast action,
/// never automatically — a running agent shouldn't be surprised mid-turn.
/// Guards the path under the session's vault and strips terminal control bytes.
#[tauri::command]
pub fn reinject_pinned_note(
    state: State<'_, PtyManager>,
    db: State<'_, crate::db::SharedDb>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let vault_root = db
        .lock()
        .ok()
        .and_then(|g| crate::commands::vault_root_for_session(&g, &session_id));
    match vault_root {
        Some(root) if crate::obsidian::pinned_notes::is_within_vault(&root, &path) => {}
        _ => return Err("note is outside the configured vault".to_string()),
    }
    let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let name = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    let block = format!(
        "\n> Pinned vault note [[{name}]] (context):\n{}\n",
        sanitize_for_pty(body.trim_end())
    );
    write_session_data(&state, &session_id, &block)
}

/// Stash a prompt to submit when the session's PTY spawns. The deep-link
/// `session/new` flow calls this after `create_session` and before activating
/// the session, so `start_claude_session` finds and folds it into the launch
/// command. No-op for sessions that never start.
#[tauri::command]
pub fn queue_session_prompt(
    state: State<'_, PtyManager>,
    session_id: String,
    prompt: String,
) -> Result<(), String> {
    if prompt.is_empty() {
        return Ok(());
    }
    state
        .pending_prompts
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id, prompt);
    Ok(())
}

/// Kill a session's PTY (and its auxiliary shells) and clean up mappings.
#[tauri::command]
pub fn kill_session_pty(state: State<'_, PtyManager>, session_id: String) -> Result<(), String> {
    kill_session_aux_shells(&state, &session_id)?;

    let pty_id = {
        let mut session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
        session_ptys.remove(&session_id)
    };

    if let Some(id) = pty_id {
        let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
        instances.remove(&id);
    }

    // Drop the ephemeral system-prompt file written for AppendSystemPromptFile
    // adapters; absent (non-CC, or no pins) → harmless no-op.
    let _ = std::fs::remove_file(crate::agents::spawn_context_file(&session_id));

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

    if session_id.contains("::") {
        // A paste can be the line's first input — anchor the start column
        // like a keystroke would (the pasted echo hasn't landed yet).
        let cursor_col = instance
            .terminal
            .session
            .lock()
            .map(|s| s.cursor_col())
            .unwrap_or(0);
        if let Ok(mut trackers) = state.aux_line_trackers.lock() {
            let t = trackers.entry(session_id.clone()).or_default();
            if t.start_col.is_none() {
                t.start_col = Some(cursor_col.saturating_sub(t.prefill_chars));
            }
        }
    }

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
    app: AppHandle,
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

    if session_id.contains("::") {
        let snapshot = handle.session.lock().ok().map(|s| {
            let is_enter = matches!(event.code.as_str(), "Enter" | "NumpadEnter");
            (
                s.is_alt_screen_active(),
                s.cursor_col(),
                is_enter.then(|| s.cursor_line_text()),
            )
        });
        if let Some((alt_screen, cursor_col, line_text)) = snapshot {
            track_aux_input(
                &app,
                &state,
                &session_id,
                &event,
                alt_screen,
                cursor_col,
                line_text,
            );
        }
    }

    // Shift+Enter → LF. Without this, both Enter and Shift+Enter encode to
    // `\r` (wezterm only diverges when the app opts into Kitty via
    // `CSI > 1 u`, which the CLIs we wrap don't reliably do on Linux).
    if (event.code == "Enter" || event.code == "NumpadEnter")
        && event.shift
        && !event.ctrl
        && !event.alt
        && !event.meta
    {
        let mut w = instance.writer.lock().map_err(|e| e.to_string())?;
        w.write_all(b"\n").map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        drop(w);
        handle.wake();
        return Ok(());
    }

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

/// Forward a primary mouse button event (left/middle/right press/release)
/// to wezterm. Required for CC's TUI menus + Ink-based pickers, which only
/// respond when the terminal forwards encoded mouse reports.
#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MouseModsWire {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
}

#[tauri::command]
pub fn terminal_mouse_button(
    state: State<'_, PtyManager>,
    session_id: String,
    button: String,
    kind: String,
    col: u16,
    row: u16,
    mods: MouseModsWire,
) -> Result<(), String> {
    use crate::terminal::{MouseMods, PrimaryButton, PrimaryKind};
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

    let btn = match button.as_str() {
        "left" => PrimaryButton::Left,
        "middle" => PrimaryButton::Middle,
        "right" => PrimaryButton::Right,
        "none" => PrimaryButton::None,
        other => return Err(format!("unknown mouse button: {other}")),
    };
    let kind = match kind.as_str() {
        "press" => PrimaryKind::Press,
        "release" => PrimaryKind::Release,
        "move" => PrimaryKind::Move,
        other => return Err(format!("unknown mouse kind: {other}")),
    };

    let mut session = handle.session.lock().map_err(|e| e.to_string())?;
    session
        .mouse_button(
            btn,
            kind,
            col,
            row,
            MouseMods {
                shift: mods.shift,
                ctrl: mods.ctrl,
                alt: mods.alt,
            },
        )
        .map_err(|e| e.to_string())?;
    drop(session);
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
