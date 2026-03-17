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

/// A single Claude Code conversation, optionally backed by a git worktree.
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
