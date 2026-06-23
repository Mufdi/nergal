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
/// session token when distinct from nergal's id (Pi/Codex UUIDs); CC
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
    /// Absolute vault-note paths pinned to this session; their bodies seed the
    /// agent's context at spawn + resume. Persisted as a JSON array column
    /// (migration `010`).
    #[serde(default)]
    pub pinned_note_paths: Vec<String>,
    /// Per-session launch options chosen at creation. Persisted as a JSON
    /// column (migration `011`) so resume re-applies them — the spawn path is
    /// shared between fresh and resumed sessions.
    #[serde(default)]
    pub launch_options: Option<LaunchOptions>,
    /// Environment shells `(label, command)` spawned in the quake terminal:
    /// auto-run at creation, pre-filled on re-open. Persisted as a JSON
    /// column (migration `013`).
    #[serde(default)]
    pub env_shells: Vec<EnvShellDef>,
    /// The single bound ClickUp task: the write-back target and session-tab
    /// indicator. Persisted as a nullable column (migration `018`).
    #[serde(default)]
    pub active_clickup_task_id: Option<String>,
    /// ClickUp tasks pinned as context-only (injected alongside the active
    /// task, never the write-back subject). Persisted as a JSON array column
    /// (migration `018`), same pattern as `pinned_note_paths`.
    #[serde(default)]
    pub pinned_clickup_task_ids: Vec<String>,
    /// The single bound Linear issue: the write-back target and session-tab
    /// indicator. Persisted as a nullable column (migration `024`).
    #[serde(default)]
    pub active_linear_issue_id: Option<String>,
    /// Linear issues pinned as context-only (injected alongside the active
    /// issue, never the write-back subject). Persisted as a JSON array column
    /// (migration `024`), same pattern as `pinned_clickup_task_ids`.
    #[serde(default)]
    pub pinned_linear_issue_ids: Vec<String>,
}

/// One environment shell: a long-running command (`pnpm dev`, `docker
/// compose up`) that gets its own quake shell — never the agent terminal,
/// where a non-exiting command would block the agent launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvShellDef {
    pub label: String,
    pub command: String,
    /// Working directory for the shell when it differs from the session cwd
    /// (front/back split repos). `~` expands; relative paths resolve against
    /// the workspace root at spawn.
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Initial permission *mode* for the session — exactly one value, mirroring
/// CC's `--permission-mode` (whose `bypassPermissions` value is what
/// `--dangerously-skip-permissions` aliases — per the CLI reference they are
/// equivalent, so "skip" is a mode, NOT a flag composable with plan/accept:
/// passing both silently starts in bypass). Adapters map each preset to
/// their native flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionPreset {
    #[default]
    Default,
    Plan,
    AcceptEdits,
    /// CC `--permission-mode auto` (v2.1.83+).
    Auto,
    /// Skip all permission prompts — CC `--dangerously-skip-permissions`,
    /// Codex `--dangerously-bypass-approvals-and-sandbox`.
    Bypass,
}

/// Options applied when the agent process is launched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaunchOptions {
    #[serde(default)]
    pub permission_preset: PermissionPreset,
    /// CC `--allow-dangerously-skip-permissions`: adds bypassPermissions to
    /// the Shift+Tab mode cycle WITHOUT starting in it ("plan now, bypass
    /// later"). Unlike `Bypass`, this composes with any preset — and is
    /// redundant when the preset already is `Bypass`.
    #[serde(default)]
    pub allow_skip_in_cycle: bool,
    /// Shell prelude run in the PTY before the agent binary (`nvm use`,
    /// `source .env`, …). Long-running commands don't belong here — they
    /// would block the agent from ever starting.
    #[serde(default)]
    pub startup_command: Option<String>,
}

impl LaunchOptions {
    /// `None`-equivalent check so callers can skip persisting empty options.
    pub fn is_noop(&self) -> bool {
        self.permission_preset == PermissionPreset::Default
            && !self.allow_skip_in_cycle
            && self
                .startup_command
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
    }
}

fn default_agent_id_string() -> String {
    "claude-code".to_string()
}

/// A workspace rooted at a directory — usually a git repo, but plain
/// directories are first-class too (git features gate on `is_git`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub repo_path: PathBuf,
    pub sessions: Vec<Session>,
    pub created_at: u64,
    /// Runtime property, never persisted: re-checked on load so an
    /// "Init git" conversion needs no schema migration.
    #[serde(default)]
    pub is_git: bool,
}
