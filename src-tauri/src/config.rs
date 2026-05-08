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
    pub plans_directory: PathBuf,
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
    /// Path where scratchpad notes (`.md` files) live. Defaults to
    /// `~/.config/cluihud/scratchpad/`. Can be changed at runtime via
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
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().expect("home directory must exist");
        let claude_dir = home.join(".claude");
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

        Self {
            claude_binary: "claude".into(),
            plans_directory: claude_dir.join("plans"),
            transcripts_directory: claude_dir.join("projects"),
            hook_socket_path: std::env::temp_dir().join("cluihud.sock"),
            default_shell: shell,
            theme_mode: "v1-dark".into(),
            preferred_editor: String::new(),
            terminal_kitty_keyboard: true,
            sidebar_dot_grid: false,
            panel_focus_pulse: false,
            scratchpad_path: None,
            default_agent: None,
            agent_overrides: HashMap::new(),
        }
    }
}

impl Config {
    /// Path to the config file.
    fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
        config_dir.join("cluihud").join("config.json")
    }

    /// Load config from disk, falling back to defaults.
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
