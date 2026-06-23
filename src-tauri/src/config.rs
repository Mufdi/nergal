use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// Map legacy theme ids ("dark"/"light") to the namespaced ones used by the
/// theme registry. Unknown / new theme ids pass through unchanged so the
/// frontend (`normalizeThemeId` in `lib/themes.ts`) is the single source of
/// truth for the registry — adding a theme there should not require Rust
/// changes here.
fn normalize_theme_mode(value: &str) -> String {
    match value {
        "" | "dark" => "v1-dark".to_string(),
        "light" => "v1-light".to_string(),
        other => other.to_string(),
    }
}

/// Application configuration with persistence support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub claude_binary: String,
    pub transcripts_directory: PathBuf,
    pub hook_socket_path: PathBuf,
    pub default_shell: String,
    pub theme_mode: String,
    pub preferred_editor: String,
    /// Enable Kitty keyboard protocol on the wezterm-term emulator. When
    /// enabled (default), Ctrl+Backspace, Shift+Enter, Alt+letter, etc.
    /// encode unambiguously so shells can bind them distinctively.
    #[serde(default = "default_true")]
    pub terminal_kitty_keyboard: bool,
    /// Render the radial dot-grid texture behind the left sidebar islands.
    /// Disable for plainer surfaces in mono / brutalist themes.
    #[serde(default)]
    pub sidebar_dot_grid: bool,
    /// When true, panel focus is communicated as a brief accent-color pulse
    /// that fades out (legacy v3-ux behavior). When false (default), the
    /// active panel keeps a permanent accent border (Omarchy-style).
    #[serde(default)]
    pub panel_focus_pulse: bool,
    /// When true, an accent-color glow halo wraps the active panel borders.
    /// Off by default — opt-in for users who want the Hyprland-y emphasis.
    #[serde(default)]
    pub panel_glow: bool,
    /// Path where scratchpad notes (`.md` files) live. Defaults to
    /// `~/.config/nergal/scratchpad/`. Can be changed at runtime via
    /// `scratchpad_set_path`; the new value is persisted here.
    #[serde(default)]
    pub scratchpad_path: Option<PathBuf>,
    /// Global default agent for new sessions. When `None`, the picker falls
    /// back to `AgentRegistry::priority_list()` (CC > Codex > OpenCode > Pi).
    /// Stored as the agent's string id (e.g. "claude-code").
    #[serde(default)]
    pub default_agent: Option<String>,
    /// Per-project default agent overrides. Keyed by canonicalized
    /// repository path; lookup priority is
    /// `agent_overrides[project] > default_agent > priority_list`.
    /// Paths are canonicalized via [`Config::canonicalize_project_path`] on
    /// both write and read so symlinks / `.` segments / trailing slashes
    /// don't produce phantom entries.
    #[serde(default)]
    pub agent_overrides: HashMap<String, String>,
    /// User-defined themes forked from a builtin base. Override accent
    /// color and font stacks while inheriting the base's surface tokens
    /// (background, card, border, etc.) via `data-theme=<base_id>`.
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
    /// ClickUp mirror poll cadence in seconds. `None` → 45s default; the
    /// poller floors it at 10s to protect the API rate budget.
    #[serde(default)]
    pub clickup_poll_interval_secs: Option<u64>,
    /// Linear mirror poll cadence in seconds. `None` → 45s default; floored at
    /// 10s. Backend-owned (see commands::BACKEND_OWNED_CONFIG_KEYS).
    #[serde(default)]
    pub linear_poll_interval_secs: Option<u64>,
    /// Linear poll window in days. `None`/0 → no window: sync ALL issues in the
    /// selected teams (team selection bounds the volume; matches ClickUp's
    /// "all per space"). A positive value bounds the scope to issues updated
    /// within the window (∪ the viewer's own), aging out older non-mine issues —
    /// for a workspace with very large teams. Backend-owned.
    #[serde(default)]
    pub linear_active_window_days: Option<u64>,
    /// Default Linear panel view applied on first open: "mine" | "state" |
    /// "project" | "assignee" | "cycle". `None`/unknown → "mine" (legacy
    /// behavior). Frontend-owned (round-trips through get_config/save_config).
    #[serde(default)]
    pub linear_default_view: Option<String>,
    /// Default ClickUp panel view applied on first open: "mine" | "status" |
    /// "list" | "assignee". `None`/unknown → "mine". Frontend-owned.
    #[serde(default)]
    pub clickup_default_view: Option<String>,
    /// User keymap overrides, keyed by `ShortcutAction.id` (e.g. "new-session")
    /// → keys string in the registry format (e.g. "ctrl+alt+n"). The frontend
    /// dispatcher resolves override-over-default; the command palette renders
    /// the effective keys. Locked shortcuts (command-palette / focus-terminal /
    /// session-1..9) are ignored even if present. This struct only persists the
    /// map verbatim — validation (collisions, reserved combos) lives frontend.
    #[serde(default)]
    pub keymap_overrides: HashMap<String, String>,
    /// Enable the MCP daemon that exposes the live session directory to agents
    /// over the dedicated socket. Default **off** until the user knowingly
    /// opts into the global-read-within-uid posture (design Decision 8). When
    /// off, the daemon still binds but `tools/call` returns `mcp_disabled`.
    #[serde(default)]
    pub mcp_server_enabled: bool,
    /// Opt-in AI session summaries (phase 6). Off by default; see
    /// [`SummaryConfig`]. The backend is a single enum, so the two active
    /// modes are structurally mutually exclusive — the Settings UI surfaces
    /// them as two switches but only one value is ever persisted.
    #[serde(default)]
    pub summary: SummaryConfig,
    /// Cross-session agent-to-agent messaging (cross-session-messaging). The
    /// `enabled` kill-switch gates ALL delivery (PTY wake + Stop emit) and is
    /// default off — a halt switch for a critical-tier autonomous PTY-injecting
    /// router. Backend-owned (the `enabled` toggle goes through
    /// `cross_session_set_enabled`; the tuning fields are config-file only).
    #[serde(default)]
    pub cross_session: CrossSessionConfig,
    /// Agent-spawned worktree sessions (agent-spawned-worktrees). An agent may
    /// REQUEST a worktree session; a mandatory, structurally un-bypassable human
    /// gate approves it. The `enabled` kill-switch is default off — the most
    /// resource-sensitive capability of the MCP set. Backend-owned (the toggle
    /// goes through `agent_worktrees_set_enabled`; tuning fields are file-only).
    #[serde(default)]
    pub agent_spawned_worktrees: AgentWorktreesConfig,
}

/// Tuning + kill-switch for agent-spawned worktree sessions. All fields have
/// explicit defaults; the request timeout is clamped at use-time to a 24h cap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWorktreesConfig {
    /// Master kill-switch. Off → `create_worktree_session` returns `disabled`
    /// and enqueues nothing. Default off.
    #[serde(default)]
    pub enabled: bool,
    /// How long a request waits at the gate before the sweeper atomically purges
    /// it as `timed_out`. Default ~1h; clamped to [60s, 24h] at use-time.
    #[serde(default = "default_worktree_request_timeout_secs")]
    pub request_timeout_secs: u64,
    /// Per-session cap on concurrent pending requests (keeps the gate usable).
    #[serde(default = "default_max_pending_per_session")]
    pub max_pending_per_session: u32,
    /// Soft cap on total worktrees: the gate UI WARNS when the count meets/
    /// exceeds it (never blocks). 0 → no warning.
    #[serde(default)]
    pub soft_worktree_cap: u32,
}

fn default_worktree_request_timeout_secs() -> u64 {
    3600
}
fn default_max_pending_per_session() -> u32 {
    3
}

impl Default for AgentWorktreesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            request_timeout_secs: default_worktree_request_timeout_secs(),
            max_pending_per_session: default_max_pending_per_session(),
            soft_worktree_cap: 0,
        }
    }
}

/// Tuning + kill-switch for cross-session messaging. Budget is a message-count
/// cap + a wall-clock deadline — NEVER tokens (nergal cannot measure agent-side
/// tokens). All fields have explicit defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSessionConfig {
    /// Master kill-switch. Off → no delivery of any kind. Default off.
    #[serde(default)]
    pub enabled: bool,
    /// Reach hop cap: how many NEW participants a thread may pull in (A→B→C→D).
    /// A reply between existing participants does not count against this.
    #[serde(default = "default_max_hops")]
    pub max_hops: u32,
    /// Conversation-length cap (message count) before a thread auto-closes.
    #[serde(default = "default_msg_budget")]
    pub msg_budget: u32,
    /// Wall-clock lifetime (seconds) before the deadline sweeper closes a thread.
    #[serde(default = "default_deadline_secs")]
    pub deadline_secs: u64,
}

fn default_max_hops() -> u32 {
    4
}
fn default_msg_budget() -> u32 {
    30
}
fn default_deadline_secs() -> u64 {
    1800
}

impl Default for CrossSessionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_hops: default_max_hops(),
            msg_budget: default_msg_budget(),
            deadline_secs: default_deadline_secs(),
        }
    }
}

/// Which inference backend produces session summaries. The variants are
/// mutually exclusive by construction (a single enum value), enforcing the
/// "never both at once" invariant without any cross-field validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SummaryBackend {
    /// No transcript read, no model invoked, no summary produced. A fully
    /// supported steady state, not a degraded one.
    #[default]
    Off,
    /// Headless agent CLI (`<cmd> -p <prompt>`) on the user's subscription —
    /// no API key. Summarization consumes the user's subscription quota.
    AgentCli,
    /// Provider-agnostic OpenAI-compatible HTTP endpoint. The key lives in the
    /// OS keyring, never in this struct.
    ApiKey,
}

/// Configuration for opt-in AI session summaries (phase 6). The API key is
/// deliberately absent — it is stored in the OS keyring, never serialized
/// here. Per-project opt-out lets a user enable summaries globally yet exclude
/// specific repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryConfig {
    #[serde(default)]
    pub backend: SummaryBackend,
    /// Agent-CLI mode: the headless command to invoke (default `claude`, run
    /// as `<cmd> -p <prompt>`). Lets a non-Claude CLI be substituted.
    #[serde(default)]
    pub agent_command: Option<String>,
    /// API-key mode: provider-agnostic OpenAI-compatible base URL.
    #[serde(default)]
    pub api_base_url: Option<String>,
    /// API-key mode: model id (e.g. `gpt-4o-mini`).
    #[serde(default)]
    pub api_model: Option<String>,
    /// Canonicalized project paths where summaries are disabled even when a
    /// backend is enabled globally (per-project override).
    #[serde(default)]
    pub disabled_projects: Vec<String>,
    /// Days a non-live session stays visible in the MCP directory and remains
    /// summarizable on demand (Revision 1). `0` means live-only. Backend-owned
    /// (inside `summary`, discarded from the frontend save payload).
    #[serde(default = "default_history_window_days")]
    pub history_window_days: u64,
}

fn default_history_window_days() -> u64 {
    7
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            backend: SummaryBackend::default(),
            agent_command: None,
            api_base_url: None,
            api_model: None,
            disabled_projects: Vec::new(),
            history_window_days: default_history_window_days(),
        }
    }
}

/// Custom theme — forked from a builtin via Settings → Appearance →
/// Customize. Inherits surface CSS tokens from `base_id` and overrides
/// the accent + fonts. See `src/lib/themes.ts` for the apply pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTheme {
    pub id: String,
    pub label: String,
    pub base_id: String,
    pub primary: String,
    pub fonts: CustomThemeFonts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemeFonts {
    pub interface: String,
    pub terminal: String,
    pub markdown: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().expect("home directory must exist");
        let claude_dir = home.join(".claude");
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        Self {
            claude_binary: "claude".into(),
            transcripts_directory: claude_dir.join("projects"),
            hook_socket_path: std::env::temp_dir().join("nergal.sock"),
            default_shell: shell,
            theme_mode: "v1-dark".into(),
            preferred_editor: String::new(),
            terminal_kitty_keyboard: true,
            sidebar_dot_grid: false,
            panel_focus_pulse: false,
            panel_glow: false,
            scratchpad_path: None,
            default_agent: None,
            agent_overrides: HashMap::new(),
            custom_themes: Vec::new(),
            clickup_poll_interval_secs: None,
            linear_poll_interval_secs: None,
            linear_active_window_days: None,
            linear_default_view: None,
            clickup_default_view: None,
            keymap_overrides: HashMap::new(),
            mcp_server_enabled: false,
            summary: SummaryConfig::default(),
            cross_session: CrossSessionConfig::default(),
            agent_spawned_worktrees: AgentWorktreesConfig::default(),
        }
    }
}

impl Config {
    /// Path to the config file.
    fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
        config_dir.join("nergal").join("config.json")
    }

    /// Load config from disk, falling back to defaults. The legacy
    /// `plans_directory` field (removed by `plan-panel-multi-agent`) is
    /// ignored on load via serde's default behavior for unknown fields and
    /// dropped on next save.
    pub fn load() -> Self {
        let path = Self::config_path();
        let mut cfg: Self = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        cfg.theme_mode = normalize_theme_mode(&cfg.theme_mode);
        cfg
    }

    /// Save config to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Canonicalize a project path so [`Self::agent_overrides`] is keyed
    /// consistently regardless of how the caller spells the path. Uses
    /// `dunce::canonicalize` (cross-platform; on Windows avoids extended
    /// `\\?\` prefixes). For non-existent paths, returns the lossy display
    /// string as a stable fallback so config writes don't fail when a path
    /// briefly disappears (e.g. mid-checkout).
    pub fn canonicalize_project_path(p: &Path) -> String {
        match dunce::canonicalize(p) {
            Ok(canonical) => canonical.display().to_string(),
            Err(_) => p.display().to_string(),
        }
    }

    /// Resolve which agent to use for a project. Implements the documented
    /// priority: `agent_overrides[project] > default_agent > registry default`.
    /// Returns `None` if neither config nor caller fallback resolved one.
    pub fn resolve_agent_for_project(&self, project_path: &Path) -> Option<String> {
        let key = Self::canonicalize_project_path(project_path);
        self.agent_overrides
            .get(&key)
            .cloned()
            .or_else(|| self.default_agent.clone())
    }

    /// Effective summary backend for a project, applying the per-project
    /// opt-out: returns [`SummaryBackend::Off`] when the global backend is off
    /// OR the project is in `summary.disabled_projects`.
    pub fn effective_summary_backend(&self, project_path: &Path) -> SummaryBackend {
        if self.summary.backend == SummaryBackend::Off {
            return SummaryBackend::Off;
        }
        let key = Self::canonicalize_project_path(project_path);
        if self.summary.disabled_projects.contains(&key) {
            return SummaryBackend::Off;
        }
        self.summary.backend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_no_agent_preferences() {
        let c = Config::default();
        assert!(c.default_agent.is_none());
        assert!(c.agent_overrides.is_empty());
    }

    #[test]
    fn resolve_returns_override_when_present() {
        let mut c = Config::default();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        let key = Config::canonicalize_project_path(path);
        c.agent_overrides.insert(key, "opencode".into());
        c.default_agent = Some("codex".into());
        assert_eq!(
            c.resolve_agent_for_project(path).as_deref(),
            Some("opencode")
        );
    }

    #[test]
    fn resolve_falls_back_to_default_agent() {
        let c = Config {
            default_agent: Some("codex".into()),
            ..Config::default()
        };
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            c.resolve_agent_for_project(dir.path()).as_deref(),
            Some("codex")
        );
    }

    #[test]
    fn resolve_returns_none_when_nothing_configured() {
        let c = Config::default();
        let dir = tempfile::tempdir().unwrap();
        assert!(c.resolve_agent_for_project(dir.path()).is_none());
    }

    #[test]
    fn canonicalize_handles_nonexistent_paths() {
        let phantom = Path::new("/definitely/does/not/exist/and/should/stay/string");
        let s = Config::canonicalize_project_path(phantom);
        assert!(!s.is_empty());
    }

    #[test]
    fn summary_off_by_default() {
        let c = Config::default();
        assert_eq!(c.summary.backend, SummaryBackend::Off);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(c.effective_summary_backend(dir.path()), SummaryBackend::Off);
    }

    #[test]
    fn summary_per_project_optout_disables() {
        let dir = tempfile::tempdir().unwrap();
        let key = Config::canonicalize_project_path(dir.path());
        let c = Config {
            summary: SummaryConfig {
                backend: SummaryBackend::AgentCli,
                disabled_projects: vec![key],
                ..SummaryConfig::default()
            },
            ..Config::default()
        };
        assert_eq!(c.effective_summary_backend(dir.path()), SummaryBackend::Off);
        let other = tempfile::tempdir().unwrap();
        assert_eq!(
            c.effective_summary_backend(other.path()),
            SummaryBackend::AgentCli
        );
    }

    #[test]
    fn summary_backend_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&SummaryBackend::AgentCli).unwrap(),
            "\"agent_cli\""
        );
        assert_eq!(
            serde_json::to_string(&SummaryBackend::ApiKey).unwrap(),
            "\"api_key\""
        );
        assert_eq!(
            serde_json::to_string(&SummaryBackend::Off).unwrap(),
            "\"off\""
        );
    }

    #[test]
    fn canonicalize_resolves_dot_segments() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        let with_dot = dir.path().join("./sub");
        let resolved_a = Config::canonicalize_project_path(&nested);
        let resolved_b = Config::canonicalize_project_path(&with_dot);
        assert_eq!(resolved_a, resolved_b);
    }
}
