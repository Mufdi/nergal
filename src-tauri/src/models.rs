use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Status of a Claude Code session within a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Running,
    NeedsAttention,
    Completed,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::NeedsAttention => "needs_attention",
            Self::Completed => "completed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "needs_attention" => Self::NeedsAttention,
            "completed" => Self::Completed,
            _ => Self::Idle,
        }
    }
}

/// A single agent conversation, optionally backed by a git worktree.
///
/// `agent_id` identifies which adapter owns the session (CC, OpenCode, Pi,
/// Codex, …). `agent_capabilities` is the wire-form bitset emitted by the
/// adapter so the frontend can gate UI synchronously without a separate
/// invoke (Decision 7). `agent_internal_session_id` carries the agent's own
/// session token when distinct from cluihud's id (Pi/Codex UUIDs); CC
/// resumes via `--continue` and so leaves it `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub workspace_id: String,
    pub worktree_path: Option<PathBuf>,
    pub worktree_branch: Option<String>,
    pub merge_target: Option<String>,
    pub status: SessionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default = "default_agent_id_string")]
    pub agent_id: String,
    #[serde(default)]
    pub agent_internal_session_id: Option<String>,
    /// Wire-form capability bitset (`Vec<&'static str>` per
    /// [`crate::agents::AgentCapability::serialize`]). Populated from the
    /// adapter the session belongs to; emitted on `session:created` /
    /// `session:activated` so the frontend doesn't need to fetch it
    /// separately.
    #[serde(default)]
    pub agent_capabilities: Vec<String>,
}

fn default_agent_id_string() -> String {
    "claude-code".to_string()
}

/// A workspace tied to a git repository root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub repo_path: PathBuf,
    pub sessions: Vec<Session>,
    pub created_at: u64,
}
