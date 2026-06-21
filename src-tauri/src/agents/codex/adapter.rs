//! Codex adapter — implements [`AgentAdapter`] reusing the FileHooks
//! transport. Hook events flow through the same Unix socket dispatcher
//! as CC; the adapter primarily contributes detection, cost extraction,
//! and the hooks.json setup helper.

use std::collections::HashMap;

use async_trait::async_trait;

use std::path::{Path, PathBuf};

use super::transcript::parse_transcript_line;
use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, ContextInjection,
    DetectionResult, EventSink, PlanCapability, SpawnContext, SpawnSpec, ThemePalette,
    TranscriptEvent, Transport, fold_prompt_preamble, write_atomic,
};
use crate::models::Session;

pub struct CodexAdapter {
    capabilities: AgentCapabilities,
    /// Root of the codex user config (`~/.codex` in practice). Captured at
    /// construction so theme writes go to a hermetic location in tests.
    config_root: PathBuf,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self::with_config_root(home.join(".codex"))
    }

    /// Constructor for tests that need a hermetic config root. Production
    /// code always goes through [`Self::new`].
    fn with_config_root(config_root: PathBuf) -> Self {
        Self {
            capabilities: AgentCapabilities {
                // Codex has permission prompts via PermissionRequest hooks
                // (same shape as CC). It has no plan-mode equivalent and no
                // first-class task list; the rollout JSONL is structured but
                // not as rich as CC's transcript for annotations injection.
                flags: AgentCapability::ASK_USER_BLOCKING
                    | AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT
                    | AgentCapability::RAW_COST_PER_MESSAGE
                    | AgentCapability::SESSION_RESUME
                    | AgentCapability::SESSION_PICKER
                    | AgentCapability::THEME_SYNC,
                supported_models: vec![],
            },
            config_root,
        }
    }
}

#[async_trait]
impl AgentAdapter for CodexAdapter {
    fn id(&self) -> AgentId {
        AgentId::codex()
    }

    fn display_name(&self) -> &str {
        "Codex"
    }

    fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    fn headless_print_command(&self) -> Option<crate::agents::HeadlessPrintCommand> {
        // `codex exec` runs non-interactively; stdout carries a banner, so the
        // final message is read from the `--output-last-message` file instead.
        Some(crate::agents::HeadlessPrintCommand {
            binary: "codex".into(),
            args: vec!["exec".into()],
            output: crate::agents::HeadlessOutput::LastMessageFile {
                flag: "--output-last-message".into(),
            },
        })
    }

    fn transport(&self) -> Transport {
        let settings_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".codex/hooks.json");
        Transport::FileHooks {
            settings_path,
            hook_event_names: vec![
                "SessionStart",
                "SessionEnd",
                "PreToolUse",
                "PostToolUse",
                "PermissionRequest",
                "Stop",
                "UserPromptSubmit",
            ],
        }
    }

    fn requires_cluihud_setup(&self) -> bool {
        true
    }

    async fn detect(&self) -> DetectionResult {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = home.join(".codex");
        let binary_path = which::which("codex").ok();
        let trusted_for_project = read_trust_for_cwd().await;
        // The binary on PATH is the authoritative install signal. Lingering
        // `~/.codex` from a previous install would otherwise mark the agent
        // as available even after the binary has been removed.
        DetectionResult {
            installed: binary_path.is_some(),
            binary_path,
            config_path: if config_dir.exists() {
                Some(config_dir)
            } else {
                None
            },
            version: None,
            trusted_for_project,
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let out = tokio::process::Command::new("codex")
            .arg("--version")
            .output()
            .await
            .ok()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() { None } else { Some(raw) }
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
        let binary = which::which("codex")
            .map_err(|e| AdapterError::Transport(anyhow::anyhow!("codex not on PATH: {e}")))?;
        let mut args: Vec<String> = Vec::new();
        // Sentinels shared with the rest of the adapters:
        //   - "continue"    → `codex resume --last`     (latest session)
        //   - "resume_pick" → `codex resume`            (Codex shows a picker)
        //   - "<any other>" → `codex resume <id>`       (specific Codex session)
        match ctx.resume_from {
            None => {}
            Some("continue") => {
                args.push("resume".into());
                args.push("--last".into());
            }
            Some("resume_pick") => args.push("resume".into()),
            Some(id) => {
                args.push("resume".into());
                args.push(id.to_string());
            }
        }
        // Permission preset → Codex approval flags (verified against codex-cli
        // 0.139.0). `--full-auto` was REMOVED; the modern equivalent of "run
        // without per-command approval, sandboxed to the workspace" is
        // `--ask-for-approval never --sandbox workspace-write`. Bypass drops the
        // sandbox too. Plan/Auto stay unmapped (Codex plan mode is TUI-only).
        if ctx.resume_from.is_none()
            && let Some(opts) = ctx.launch_options
        {
            match opts.permission_preset {
                crate::models::PermissionPreset::AcceptEdits => {
                    args.push("--ask-for-approval".into());
                    args.push("never".into());
                    args.push("--sandbox".into());
                    args.push("workspace-write".into());
                }
                crate::models::PermissionPreset::Bypass => {
                    args.push("--dangerously-bypass-approvals-and-sandbox".into());
                }
                _ => {}
            }
        }
        // Codex takes the prompt as a positional `[PROMPT]` arg, which the
        // `resume` subcommand reinterprets as a session id — so the preamble
        // only rides a fresh launch. Re-inject on resume falls to the next turn.
        if ctx.resume_from.is_none()
            && let Some(text) = fold_prompt_preamble(ctx.injected_context, ctx.initial_prompt)
        {
            args.push(text);
        }
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.to_string());
        Ok(SpawnSpec { binary, args, env })
    }

    fn permission_presets(&self) -> &'static [crate::models::PermissionPreset] {
        use crate::models::PermissionPreset as P;
        // No Plan/Auto: Codex plan mode is TUI-only, no CLI flag.
        &[P::Default, P::AcceptEdits, P::Bypass]
    }

    fn context_injection(&self) -> ContextInjection {
        ContextInjection::PromptPreamble
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        parse_transcript_line(line)
    }

    fn plan_capability(&self, _session: &Session, _cwd: &Path) -> PlanCapability {
        // SCOPE: Codex plan mode is TUI-only (no on-disk artifact). Re-evaluate
        // if a future change wires rollout-JSONL extraction.
        PlanCapability::NotApplicable
    }

    async fn start_event_pump(
        &self,
        _session_id: &str,
        _sink: EventSink,
    ) -> Result<(), AdapterError> {
        // Codex hook events arrive on the shared Unix socket (same as CC).
        // The rollout JSONL tail for cost extraction is wired through the
        // dispatcher when it gains adapter-aware routing — for now a no-op
        // keeps Codex's hook flow functional without spurious watchers.
        Ok(())
    }

    async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError> {
        let path = self.config_root.join("config.toml");
        let theme_name = derive_codex_theme(palette);
        upsert_codex_tui_theme(&path, theme_name).await?;
        Ok(())
    }
}

/// Map cluihud's light/dark variant to a codex `tui.theme` name. Codex
/// today exposes a single syntax theme name (`monochrome`) regardless of
/// variant — known limitation, documented in the spec delta. Returning a
/// constant lets the mapping evolve when codex CLI widens its theme keys.
fn derive_codex_theme(_palette: &ThemePalette) -> &'static str {
    "monochrome"
}

/// Upsert `[tui] theme = "<value>"` in `path`, preserving every other
/// table, key, comment and whitespace. Creates the file when missing.
async fn upsert_codex_tui_theme(path: &Path, value: &str) -> Result<(), AdapterError> {
    let raw = match tokio::fs::read_to_string(path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(AdapterError::Io(e)),
    };
    let mut doc: toml_edit::DocumentMut = raw
        .parse()
        .map_err(|e: toml_edit::TomlError| AdapterError::Transport(anyhow::anyhow!(e)))?;
    let tui = doc
        .entry("tui")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let tui_table = tui
        .as_table_mut()
        .ok_or_else(|| AdapterError::Transport(anyhow::anyhow!("[tui] is not a TOML table")))?;
    tui_table["theme"] = toml_edit::value(value);
    write_atomic(path, doc.to_string().as_bytes()).await?;
    Ok(())
}

/// Best-effort read of Codex's per-project trust state. Codex stores trust
/// in `~/.codex/config.toml` or a project-specific file; the exact format
/// shifts between versions. Returns `None` when the bit is unknown so the
/// UI shows a neutral state rather than asserting one way or the other.
async fn read_trust_for_cwd() -> Option<bool> {
    let home = dirs::home_dir()?;
    let trust_file = home.join(".codex").join("trust.json");
    if !trust_file.exists() {
        return None;
    }
    let content = tokio::fs::read_to_string(&trust_file).await.ok()?;
    let trust: serde_json::Value = serde_json::from_str(&content).ok()?;
    let cwd = std::env::current_dir().ok()?.display().to_string();
    trust.get(&cwd).and_then(|v| v.as_bool()).or(Some(false))
}

/// Re-export of [`super::setup::run_codex_setup`] for use by the
/// `cluihud setup` flow when the Codex adapter is selected.
pub use super::setup::run_codex_setup;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_injection_tier_is_prompt_preamble() {
        assert_eq!(
            CodexAdapter::new().context_injection(),
            ContextInjection::PromptPreamble
        );
    }

    #[test]
    fn capabilities_match_codex_design() {
        let a = CodexAdapter::new();
        let caps = a.capabilities().flags;
        assert!(!caps.contains(AgentCapability::PLAN_REVIEW));
        assert!(caps.contains(AgentCapability::ASK_USER_BLOCKING));
        assert!(!caps.contains(AgentCapability::TASK_LIST));
        assert!(!caps.contains(AgentCapability::ANNOTATIONS_INJECT));
        assert!(caps.contains(AgentCapability::SESSION_RESUME));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = CodexAdapter::new();
        assert_eq!(a.id().as_str(), "codex");
        assert_eq!(a.display_name(), "Codex");
    }

    #[test]
    fn transport_targets_codex_hooks_json() {
        let a = CodexAdapter::new();
        match a.transport() {
            Transport::FileHooks {
                settings_path,
                hook_event_names,
            } => {
                assert!(settings_path.ends_with(".codex/hooks.json"));
                assert!(hook_event_names.contains(&"PreToolUse"));
                assert!(hook_event_names.contains(&"PermissionRequest"));
            }
            other => panic!("expected FileHooks, got {other:?}"),
        }
    }

    #[test]
    fn requires_cluihud_setup_is_true_for_codex() {
        let a = CodexAdapter::new();
        assert!(a.requires_cluihud_setup());
    }

    fn sample_palette() -> ThemePalette {
        ThemePalette {
            id: "v11-tokyo-night".into(),
            is_dark: true,
            surface: "#1a1b26".into(),
            foreground: "#c0caf5".into(),
            card: "#16161e".into(),
            secondary: "#24283b".into(),
            muted_foreground: "#7a88cf".into(),
            border: "rgba(255,255,255,0.08)".into(),
            accent: "#7aa2f7".into(),
        }
    }

    #[tokio::test]
    async fn apply_theme_writes_tui_theme_key() {
        let root = tempfile::tempdir().unwrap();
        let adapter = CodexAdapter::with_config_root(root.path().to_path_buf());
        adapter.apply_theme(&sample_palette()).await.unwrap();
        let raw = tokio::fs::read_to_string(root.path().join("config.toml"))
            .await
            .unwrap();
        let parsed: toml_edit::DocumentMut = raw.parse().unwrap();
        assert_eq!(parsed["tui"]["theme"].as_str(), Some("monochrome"));
    }

    #[tokio::test]
    async fn apply_theme_preserves_other_config_keys() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(root.path()).await.unwrap();
        tokio::fs::write(
            root.path().join("config.toml"),
            br#"# user-managed
model = "gpt-5-codex"

[mcp_servers.foo]
command = "/usr/bin/foo"
"#,
        )
        .await
        .unwrap();
        let adapter = CodexAdapter::with_config_root(root.path().to_path_buf());
        adapter.apply_theme(&sample_palette()).await.unwrap();
        let raw = tokio::fs::read_to_string(root.path().join("config.toml"))
            .await
            .unwrap();
        let parsed: toml_edit::DocumentMut = raw.parse().unwrap();
        assert_eq!(parsed["model"].as_str(), Some("gpt-5-codex"));
        assert_eq!(
            parsed["mcp_servers"]["foo"]["command"].as_str(),
            Some("/usr/bin/foo")
        );
        assert_eq!(parsed["tui"]["theme"].as_str(), Some("monochrome"));
        assert!(raw.contains("# user-managed"), "leading comment preserved");
    }

    #[tokio::test]
    async fn apply_theme_creates_missing_config_file() {
        let root = tempfile::tempdir().unwrap();
        let adapter = CodexAdapter::with_config_root(root.path().to_path_buf());
        let path = root.path().join("config.toml");
        assert!(!path.exists());
        adapter.apply_theme(&sample_palette()).await.unwrap();
        let raw = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(raw.contains("[tui]"));
        assert!(raw.contains("theme = \"monochrome\""));
    }

    #[test]
    fn spawn_resume_modes_map_to_correct_codex_flags() {
        let a = CodexAdapter::new();
        let cwd = std::path::Path::new("/tmp");
        let mk = |resume: Option<&'static str>| SpawnContext {
            session_id: "s",
            cwd,
            resume_from: resume,
            initial_prompt: None,
            injected_context: None,
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&mk(None)) {
            assert!(spec.args.is_empty());
        }
        if let Ok(spec) = a.spawn(&mk(Some("continue"))) {
            assert_eq!(spec.args, vec!["resume".to_string(), "--last".to_string()]);
        }
        if let Ok(spec) = a.spawn(&mk(Some("resume_pick"))) {
            assert_eq!(spec.args, vec!["resume".to_string()]);
        }
        if let Ok(spec) = a.spawn(&mk(Some("abc-uuid"))) {
            assert_eq!(
                spec.args,
                vec!["resume".to_string(), "abc-uuid".to_string()]
            );
        }
    }

    #[test]
    fn permission_presets_map_to_real_codex_0139_flags() {
        // Regression guard: `--full-auto` was removed in codex-cli 0.139.0.
        use crate::models::{LaunchOptions, PermissionPreset};
        let a = CodexAdapter::new();
        let cwd = std::path::Path::new("/tmp");
        let mk = |preset: PermissionPreset| {
            let opts = LaunchOptions {
                permission_preset: preset,
                ..Default::default()
            };
            // Leak so the &-borrow in SpawnContext lives for the call.
            let opts: &'static LaunchOptions = Box::leak(Box::new(opts));
            SpawnContext {
                session_id: "s",
                cwd,
                resume_from: None,
                initial_prompt: None,
                injected_context: None,
                launch_options: Some(opts),
            }
        };
        let edits = a.spawn(&mk(PermissionPreset::AcceptEdits)).unwrap();
        assert_eq!(
            edits.args,
            vec![
                "--ask-for-approval".to_string(),
                "never".to_string(),
                "--sandbox".to_string(),
                "workspace-write".to_string(),
            ]
        );
        assert!(!edits.args.iter().any(|a| a == "--full-auto"));
        let bypass = a.spawn(&mk(PermissionPreset::Bypass)).unwrap();
        assert_eq!(
            bypass.args,
            vec!["--dangerously-bypass-approvals-and-sandbox".to_string()]
        );
    }
}
