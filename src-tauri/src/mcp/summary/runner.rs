//! Detached, debounced summary runner triggered on `Stop`.
//!
//! Unlike the post-session MOC runner (a separate detached *process* because
//! MOC builds must survive app-close), summaries are best-effort and
//! regenerate on the next `Stop`, so this runs as an in-process async task: it
//! never blocks the hook response, and a missed run is harmless. Two guards
//! keep it cheap — a per-session debounce window and a single-flight set.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dashmap::{DashMap, DashSet};

use crate::agents::state::AgentRuntimeState;
use crate::config::{Config, SummaryBackend};
use crate::db::SharedDb;

/// Minimum interval between summaries for one session.
const DEBOUNCE: Duration = Duration::from_secs(60);

fn last_run() -> &'static DashMap<String, Instant> {
    static M: OnceLock<DashMap<String, Instant>> = OnceLock::new();
    M.get_or_init(DashMap::new)
}

fn in_flight() -> &'static DashSet<String> {
    static M: OnceLock<DashSet<String>> = OnceLock::new();
    M.get_or_init(DashSet::new)
}

/// Decide whether to summarize this session's `Stop`, and if so spawn the
/// generation detached. Non-blocking. No-op when: no transcript path, the
/// backend is `Off` for the session's project, the debounce window is still
/// open, or a run is already in flight.
pub fn maybe_spawn(
    db: &SharedDb,
    agents: &AgentRuntimeState,
    session_id: &str,
    transcript_path: Option<&str>,
) {
    let Some(transcript_path) = transcript_path else {
        return;
    };
    let transcript = PathBuf::from(transcript_path);

    // Per-project backend decision is keyed by the workspace repo path (matching
    // how other per-project config is keyed), so worktree sessions inherit their
    // repo's setting rather than a phantom worktree path.
    let project = {
        let Ok(guard) = db.lock() else {
            return;
        };
        let Ok(Some(session)) = guard.find_session(session_id) else {
            return;
        };
        match guard.workspace_repo_path(&session.workspace_id) {
            Ok(Some(p)) => p,
            _ => return,
        }
    };

    let cfg = Config::load();
    let backend = cfg.effective_summary_backend(&project);
    if backend == SummaryBackend::Off {
        return;
    }

    // Resolve the default agent's verified headless command (used only by the
    // AgentCli backend when no explicit summary command is set). Honors the
    // agent marked default in Settings; falls back to Claude Code.
    let default_agent = cfg.default_agent.as_deref().unwrap_or("claude-code");
    let agent_cmd = agents.headless_print_command(default_agent);

    if let Some(prev) = last_run().get(session_id)
        && prev.elapsed() < DEBOUNCE
    {
        return;
    }
    // `insert` returns false when already present → another run owns it.
    if !in_flight().insert(session_id.to_string()) {
        return;
    }

    let db = db.clone();
    let sid = session_id.to_string();
    let summary_cfg = cfg.summary.clone();
    tracing::info!(session_id = %sid, ?backend, "generating session summary");
    tauri::async_runtime::spawn(async move {
        let result = crate::mcp::summary::summarize_transcript(
            backend,
            &summary_cfg,
            agent_cmd,
            &transcript,
        )
        .await;
        match result {
            Ok(s) => {
                if let Ok(guard) = db.lock()
                    && let Err(e) =
                        guard.set_session_summary(&sid, &s.text, s.model.as_deref(), s.token_cost)
                {
                    tracing::warn!(session_id = %sid, "persisting session summary failed: {e:#}");
                }
                tracing::info!(session_id = %sid, "session summary stored");
                last_run().insert(sid.clone(), Instant::now());
            }
            Err(e) => {
                // Best-effort: log and move on; the next Stop retries.
                tracing::warn!(session_id = %sid, "session summary generation failed: {e:#}");
            }
        }
        in_flight().remove(&sid);
    });
}
