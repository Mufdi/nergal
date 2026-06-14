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

/// One live session as seen by an agent. Unknown fields are null/empty, never
/// fabricated (session-directory spec). `waiting_for`, `recently_touched_files`,
/// `background_tasks`, `session_crons` and `summary` are additive: populated as
/// the out-of-band cache / Stop-hook capture / summarizer land.
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
    pub recently_touched_files: Vec<String>,
    pub background_tasks: Vec<Value>,
    pub session_crons: Vec<Value>,
    pub summary: Option<String>,
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

/// All live sessions across all workspaces (global-read within the uid —
/// Decision 2b). `exclude` drops the caller's own session from the list.
pub fn list_sessions(
    ctx: &DaemonContext,
    exclude: Option<&str>,
) -> anyhow::Result<Vec<SessionDescriptor>> {
    let live = ctx.agents.live_session_ids();
    // Brief DB lock → owned snapshot → guard dropped here.
    let workspaces = {
        let guard = ctx
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        guard.get_workspaces()?
    };
    let mut out = Vec::new();
    for ws in workspaces {
        let ws_path = ws.repo_path.to_string_lossy().to_string();
        for s in ws.sessions {
            if !live.contains(&s.id) {
                continue;
            }
            if Some(s.id.as_str()) == exclude {
                continue;
            }
            out.push(descriptor_from(&ctx.agents, &s, &ws.name, &ws_path));
        }
    }
    Ok(out)
}

/// One live session by id. `Ok(None)` when the id is not a live session.
pub fn get_session(ctx: &DaemonContext, id: &str) -> anyhow::Result<Option<SessionDescriptor>> {
    if !ctx.agents.live_session_ids().contains(id) {
        return Ok(None);
    }
    let workspaces = {
        let guard = ctx
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        guard.get_workspaces()?
    };
    for ws in workspaces {
        let ws_path = ws.repo_path.to_string_lossy().to_string();
        for s in ws.sessions {
            if s.id == id {
                return Ok(Some(descriptor_from(&ctx.agents, &s, &ws.name, &ws_path)));
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
) -> SessionDescriptor {
    let (background_tasks, session_crons) = agents.session_background(&s.id);
    SessionDescriptor {
        session_id: s.id.clone(),
        name: s.name.clone(),
        workspace_id: s.workspace_id.clone(),
        workspace_name: ws_name.to_string(),
        workspace_path: ws_path.to_string(),
        git_branch: s.worktree_branch.clone(),
        agent: s.agent_id.clone(),
        mode: s.status.as_str().to_string(),
        waiting_for: None,
        last_activity: s.updated_at,
        recently_touched_files: Vec::new(),
        background_tasks,
        session_crons,
        summary: None,
    }
}
