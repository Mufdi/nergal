//! Session-directory descriptor assembly + the `whoami`/`list_sessions`/
//! `get_session` tool bodies.
//!
//! Freshness with bounded lock scope (design Decision 3): the only locks taken
//! are the concurrent `DashMap` (liveness/bg-tasks, lock-free per entry) and a
//! brief `SharedDb` lock that yields an **owned** `Vec<Workspace>`; the guard is
//! dropped before descriptors are built. No git/subprocess runs on the read
//! path — the branch comes from the persisted `worktree_branch` column, so the
//! `AgentRuntimeState`/DB locks are never held across blocking work.

use serde::Serialize;
use serde_json::Value;

use super::DaemonContext;
use crate::agents::state::TouchedFile;
use crate::config::Config;
use crate::db::SessionSummary;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Whether a non-live session's last activity falls inside the configured
/// history window. `window_days == 0` means live-only (no dead sessions shown).
///
/// Saturating arithmetic: a future `last_stop_at` (clock skew / system-time
/// rollback) yields `0`, so such a session stays visible until its stamp falls
/// back inside the window. `last_stop_at` is daemon-stamped (`server.rs`), never
/// caller-supplied, so this is a benign skew edge, not an exposure vector.
fn within_window(last_stop_at: u64, now: u64, window_days: u64) -> bool {
    window_days > 0 && now.saturating_sub(last_stop_at) <= window_days.saturating_mul(86_400)
}

/// A session is dirty (needs (re)summarization) when it has no summary yet, or
/// when newer activity (`last_stop_at`) postdates the summary's covered-through.
fn is_dirty(summary_updated_at: Option<u64>, last_stop_at: u64) -> bool {
    summary_updated_at.map(|u| last_stop_at > u).unwrap_or(true)
}

/// One live session as seen by an agent. Unknown fields are null/empty, never
/// fabricated (session-directory spec). `waiting_for`, `recently_touched_files`,
/// `background_tasks`, `session_crons` and `last_assistant_message` are
/// additive: populated as the out-of-band cache / Stop-hook capture land.
#[derive(Debug, Clone, Serialize)]
pub struct SessionDescriptor {
    pub session_id: String,
    pub name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_path: String,
    pub git_branch: Option<String>,
    pub agent: String,
    /// idle | running | needs_attention | completed
    pub mode: String,
    pub waiting_for: Option<String>,
    pub last_activity: u64,
    pub recently_touched_files: Vec<TouchedFile>,
    pub background_tasks: Vec<Value>,
    pub session_crons: Vec<Value>,
    /// Verbatim last assistant message from the Stop hook — an honest "where it
    /// left off", NOT a synthesized recap.
    pub last_assistant_message: Option<String>,
    /// Phase-6 AI recap from `session_summaries`. `None` until a backend is
    /// enabled and a summary has been generated; distinct from the raw
    /// `last_assistant_message` so the two are never conflated. Its timestamp
    /// denotes *activity covered through* (the consumed `last_stop_at`), not
    /// generation wall-clock — see [`SessionSummary`].
    pub summary: Option<String>,
    /// Whether the session is currently running (Revision 1). When false the
    /// session is recently-dead: the in-memory activity side-maps are empty, so
    /// `mode` is the frozen persisted value and `recently_touched_files` /
    /// `background_tasks` / `session_crons` / `last_assistant_message` are empty
    /// — only `summary` and `git_branch` are meaningful. Empty activity fields
    /// here mean "not in memory", NOT "no activity occurred".
    pub is_live: bool,
    /// True when newer activity (`last_stop_at`) postdates the served summary's
    /// timestamp — the recap is stale and a fresh one is being generated lazily.
    pub summary_stale: bool,
}

/// `whoami`: the caller's own resolved identity (or unidentified).
pub fn whoami(ctx: &DaemonContext, identity: Option<&str>) -> Value {
    match identity.and_then(|id| get_session(ctx, id).ok().flatten()) {
        Some(desc) => serde_json::json!({
            "identified": true,
            "session": desc,
        }),
        None => serde_json::json!({
            "identified": false,
            "reason": "caller env hint did not match a live session (connect-before-register race or external process)",
        }),
    }
}

/// All live sessions plus recently-dead ones within the history window
/// (global-read within the uid — Decision 2b). `exclude` drops the caller's own
/// session. Serves cached summaries only — NEVER triggers generation (a
/// directory listing must not fan out into per-session inference); only
/// [`get_session`] does. Dead-session rosters come from the persisted `sessions`
/// table cross-referenced with the pull-markers; a session with no marker (no
/// `Stop` yet) has no `last_stop_at` and is excluded until its next `Stop`.
pub fn list_sessions(
    ctx: &DaemonContext,
    exclude: Option<&str>,
) -> anyhow::Result<Vec<SessionDescriptor>> {
    let window_days = Config::load().summary.history_window_days;
    let now = now_secs();
    let live = ctx.agents.live_session_ids();
    // Brief DB lock → owned snapshot (workspaces + summaries + markers) → drop.
    let (workspaces, summaries, transcripts) = {
        let guard = ctx
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        (
            guard.get_workspaces()?,
            guard.get_all_session_summaries()?,
            guard.get_all_session_transcripts()?,
        )
    };
    let mut out = Vec::new();
    for ws in workspaces {
        let ws_path = ws.repo_path.to_string_lossy().to_string();
        for s in ws.sessions {
            if Some(s.id.as_str()) == exclude {
                continue;
            }
            let is_live = live.contains(&s.id);
            let marker = transcripts.get(&s.id);
            if !is_live {
                // Recently-dead only: requires a marker inside the window.
                let recent = marker
                    .map(|t| within_window(t.last_stop_at, now, window_days))
                    .unwrap_or(false);
                if !recent {
                    continue;
                }
            }
            out.push(descriptor_from(
                &ctx.agents,
                &s,
                &ws.name,
                &ws_path,
                summaries.get(&s.id),
                marker.map(|t| t.last_stop_at),
                is_live,
            ));
        }
    }
    Ok(out)
}

/// One session by id — live, or recently-dead within the history window.
/// `Ok(None)` when neither. As the sole intentional single-session read, this is
/// the ONLY tool that triggers lazy summary generation: when the session is
/// dirty (no summary, or `last_stop_at > summary.updated_at`) and has a marker,
/// it spawns generation detached and returns the current (stale/null) summary
/// without blocking.
pub fn get_session(ctx: &DaemonContext, id: &str) -> anyhow::Result<Option<SessionDescriptor>> {
    let is_live = ctx.agents.live_session_ids().contains(id);
    let (workspaces, summary, marker) = {
        let guard = ctx
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        (
            guard.get_workspaces()?,
            guard.get_session_summary(id)?,
            guard.get_session_transcript(id)?,
        )
    };
    // Visibility: live is always shown; a dead session only within the window.
    // Config is read here (outside any lock) ONLY for the dead case, so live
    // reads stay FS-free.
    if !is_live {
        let window_days = Config::load().summary.history_window_days;
        let recent = marker
            .as_ref()
            .map(|t| within_window(t.last_stop_at, now_secs(), window_days))
            .unwrap_or(false);
        if !recent {
            return Ok(None);
        }
    }
    for ws in workspaces {
        let ws_path = ws.repo_path.to_string_lossy().to_string();
        for s in ws.sessions {
            if s.id == id {
                // Lazy trigger (sole generation entrypoint). A markerless session
                // has nothing to summarize, so it never spawns.
                if let Some(t) = &marker {
                    let dirty = is_dirty(summary.as_ref().map(|sm| sm.updated_at), t.last_stop_at);
                    if dirty {
                        crate::mcp::summary::runner::maybe_spawn(
                            &ctx.db,
                            &ctx.agents,
                            id,
                            &ws.repo_path,
                            &t.transcript_path,
                            t.last_stop_at,
                        );
                    }
                }
                return Ok(Some(descriptor_from(
                    &ctx.agents,
                    &s,
                    &ws.name,
                    &ws_path,
                    summary.as_ref(),
                    marker.as_ref().map(|t| t.last_stop_at),
                    is_live,
                )));
            }
        }
    }
    Ok(None)
}

fn descriptor_from(
    agents: &crate::agents::state::AgentRuntimeState,
    s: &crate::models::Session,
    ws_name: &str,
    ws_path: &str,
    summary: Option<&SessionSummary>,
    last_stop_at: Option<u64>,
    is_live: bool,
) -> SessionDescriptor {
    let (background_tasks, session_crons) = agents.session_background(&s.id);
    // Live mode + last_activity + waiting_for come from the runtime side-map the
    // hook dispatcher feeds; the DB row's status/updated_at only move on
    // lifecycle mutations, so fall back to them only when no event has been seen
    // for this session since the daemon started.
    let (mode, last_activity, waiting_for) = match agents.session_activity(&s.id) {
        Some(a) => (a.mode, a.last_activity, a.waiting_for),
        None => (s.status.as_str().to_string(), s.updated_at, None),
    };
    // Stale when newer activity postdates the served summary's covered-through.
    let summary_stale = match (summary, last_stop_at) {
        (Some(sm), Some(ls)) => ls > sm.updated_at,
        _ => false,
    };
    SessionDescriptor {
        session_id: s.id.clone(),
        name: s.name.clone(),
        workspace_id: s.workspace_id.clone(),
        workspace_name: ws_name.to_string(),
        workspace_path: ws_path.to_string(),
        git_branch: s.worktree_branch.clone(),
        agent: s.agent_id.clone(),
        mode,
        waiting_for,
        last_activity,
        recently_touched_files: agents.session_files(&s.id),
        background_tasks,
        session_crons,
        last_assistant_message: agents.session_last_message(&s.id),
        summary: summary.map(|sm| sm.summary.clone()),
        is_live,
        summary_stale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{AgentId, state::AgentRuntimeState};
    use crate::db::Database;
    use crate::models::{Session, SessionStatus};
    use std::sync::{Arc, Mutex};

    // Epoch+1s — ~56 years ago, outside any sane history window. Used for the
    // "expired dead" cases so the integration tests don't depend on the exact
    // configured window (whose boundary is covered by `within_window` directly).
    const ANCIENT: u64 = 1;

    fn ctx() -> DaemonContext {
        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let agents = AgentRuntimeState::bootstrap().unwrap();
        {
            let g = db.lock().unwrap();
            g.create_workspace("ws1", "ws", "/tmp/repo").unwrap();
        }
        DaemonContext {
            db,
            agents,
            app_uid: 0,
            delivery: std::sync::Arc::new(crate::mcp::delivery::NoopDelivery),
        }
    }

    fn add_session(ctx: &DaemonContext, id: &str) {
        let s = Session {
            id: id.to_string(),
            name: id.to_string(),
            workspace_id: "ws1".into(),
            worktree_path: None,
            worktree_branch: None,
            merge_target: None,
            status: SessionStatus::Idle,
            created_at: 0,
            updated_at: 0,
            agent_id: "claude-code".into(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options: None,
            env_shells: Vec::new(),
            active_clickup_task_id: None,
            pinned_clickup_task_ids: Vec::new(),
            active_linear_issue_id: None,
            pinned_linear_issue_ids: Vec::new(),
        };
        ctx.db.lock().unwrap().create_session(&s).unwrap();
    }

    fn make_live(ctx: &DaemonContext, id: &str) {
        ctx.agents.register_session(id, AgentId::claude_code());
    }

    #[test]
    fn is_dirty_truth_table() {
        assert!(is_dirty(None, 100), "no summary row → dirty");
        assert!(is_dirty(Some(50), 100), "newer activity → dirty");
        assert!(!is_dirty(Some(100), 100), "covered through → clean");
        assert!(!is_dirty(Some(200), 100), "summary ahead → clean");
    }

    #[test]
    fn within_window_boundaries() {
        let now = 1_000_000;
        assert!(within_window(now, now, 7), "just stopped → in window");
        assert!(within_window(now - 6 * 86_400, now, 7), "6d < 7d window");
        assert!(!within_window(now - 8 * 86_400, now, 7), "8d > 7d window");
        assert!(
            !within_window(now, now, 0),
            "window 0 → live-only, nothing dead"
        );
    }

    #[test]
    fn list_includes_live_and_recent_dead_excludes_expired_and_markerless() {
        let ctx = ctx();
        let now = now_secs();
        for id in ["live", "recent", "expired", "markerless"] {
            add_session(&ctx, id);
        }
        make_live(&ctx, "live");
        {
            let g = ctx.db.lock().unwrap();
            g.set_session_transcript("live", "/t/live.jsonl", now)
                .unwrap();
            g.set_session_transcript("recent", "/t/recent.jsonl", now)
                .unwrap();
            g.set_session_transcript("expired", "/t/expired.jsonl", ANCIENT)
                .unwrap();
            // "markerless" gets no transcript row.
        }
        let out = list_sessions(&ctx, None).unwrap();
        let ids: std::collections::HashMap<_, _> = out
            .iter()
            .map(|d| (d.session_id.as_str(), d.is_live))
            .collect();
        assert_eq!(ids.get("live"), Some(&true), "live shown, is_live");
        assert_eq!(
            ids.get("recent"),
            Some(&false),
            "recent-dead shown, !is_live"
        );
        assert!(!ids.contains_key("expired"), "expired dead excluded");
        assert!(!ids.contains_key("markerless"), "markerless dead excluded");
    }

    #[test]
    fn list_flags_summary_stale() {
        let ctx = ctx();
        let now = now_secs();
        add_session(&ctx, "stale");
        add_session(&ctx, "fresh");
        make_live(&ctx, "stale");
        make_live(&ctx, "fresh");
        {
            let g = ctx.db.lock().unwrap();
            // stale: activity after the summary's covered-through.
            g.set_session_summary("stale", "old recap", None, None, now - 100)
                .unwrap();
            g.set_session_transcript("stale", "/t/s.jsonl", now)
                .unwrap();
            // fresh: summary covers the latest activity.
            g.set_session_summary("fresh", "current recap", None, None, now)
                .unwrap();
            g.set_session_transcript("fresh", "/t/f.jsonl", now - 100)
                .unwrap();
        }
        let out = list_sessions(&ctx, None).unwrap();
        let stale = out.iter().find(|d| d.session_id == "stale").unwrap();
        let fresh = out.iter().find(|d| d.session_id == "fresh").unwrap();
        assert!(stale.summary_stale, "newer activity than summary → stale");
        assert!(!fresh.summary_stale, "summary covers activity → not stale");
    }

    #[test]
    fn get_session_excludes_expired_dead() {
        let ctx = ctx();
        add_session(&ctx, "old");
        ctx.db
            .lock()
            .unwrap()
            .set_session_transcript("old", "/t/old.jsonl", ANCIENT)
            .unwrap();
        assert!(
            get_session(&ctx, "old").unwrap().is_none(),
            "dead session outside the window is not found"
        );
    }

    #[test]
    fn get_session_returns_clean_recent_dead_without_triggering() {
        // A clean (summary covers activity) recently-dead session: get_session
        // returns its descriptor with is_live=false and does NOT trigger
        // generation (is_dirty is false), so the test never spawns a backend.
        let ctx = ctx();
        let now = now_secs();
        add_session(&ctx, "yesterday");
        {
            let g = ctx.db.lock().unwrap();
            g.set_session_summary("yesterday", "did the thing", None, None, now)
                .unwrap();
            g.set_session_transcript("yesterday", "/t/y.jsonl", now)
                .unwrap();
        }
        let d = get_session(&ctx, "yesterday").unwrap().unwrap();
        assert!(!d.is_live);
        assert!(!d.summary_stale);
        assert_eq!(d.summary.as_deref(), Some("did the thing"));
    }
}
