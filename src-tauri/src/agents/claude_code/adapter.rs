//! Claude Code adapter — first concrete implementation of [`AgentAdapter`].
//!
//! Wraps the existing CC-specific logic that lives in this module's siblings
//! (`transcript`, `plan`, `cost`, `tasks_parser`). The runtime previously
//! reached into those modules directly; subsequent commits migrate the call
//! sites to go through this trait so OpenCode/Pi/Codex can plug in without
//! special-casing.
//!
//! Two pieces of state live here, populated by the hook dispatcher when the
//! corresponding hook event fires for a CC session:
//! - `pending_plan_fifos`: maps `session_id` → FIFO path written by
//!   `nergal hook plan-review`. [`Self::submit_plan_decision`] reads this
//!   to know where to unblock the CLI.
//! - `pending_ask_fifos`: same, for `nergal hook ask-user`.
//!
//! Until the hook dispatcher is rewired (commit 4), these maps stay empty
//! and `submit_*` returns [`AdapterError::SessionLocked`] on miss. The
//! existing Tauri commands (`commands::submit_plan_decision`,
//! `commands::submit_ask_answer`) remain the production call path during
//! the transition; the trait methods are the future call path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, ContextInjection,
    DetectionResult, EventSink, PlanCapability, PlanDecision, SpawnContext, SpawnSpec,
    TranscriptEvent, Transport,
};
use crate::models::Session;

use super::cost;
use super::plans_path::resolve_cc_plans_directory;

/// Wrapper around CC's hooks/transcript/plan/tasks behavior.
pub struct ClaudeCodeAdapter {
    capabilities: AgentCapabilities,
    pending_plan_fifos: Arc<DashMap<String, PathBuf>>,
    pending_ask_fifos: Arc<DashMap<String, PathBuf>>,
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self {
            capabilities: AgentCapabilities {
                flags: AgentCapability::PLAN_REVIEW
                    | AgentCapability::ASK_USER_BLOCKING
                    | AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT
                    | AgentCapability::RAW_COST_PER_MESSAGE
                    | AgentCapability::TASK_LIST
                    | AgentCapability::SESSION_RESUME
                    | AgentCapability::SESSION_PICKER
                    | AgentCapability::ANNOTATIONS_INJECT,
                supported_models: vec![],
            },
            pending_plan_fifos: Arc::new(DashMap::new()),
            pending_ask_fifos: Arc::new(DashMap::new()),
        }
    }

    /// Record the FIFO path for a session's pending plan-review. Called by
    /// the hook dispatcher when a `PlanReview` hook event arrives.
    pub fn register_pending_plan_fifo(&self, session_id: &str, path: PathBuf) {
        self.pending_plan_fifos.insert(session_id.to_string(), path);
    }

    /// Record the FIFO path for a session's pending ask-user. Called by the
    /// hook dispatcher when an `AskUser` hook event arrives.
    pub fn register_pending_ask_fifo(&self, session_id: &str, path: PathBuf) {
        self.pending_ask_fifos.insert(session_id.to_string(), path);
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> AgentId {
        AgentId::claude_code()
    }

    fn display_name(&self) -> &str {
        "Claude Code"
    }

    fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    fn headless_print_command(&self) -> Option<crate::agents::HeadlessPrintCommand> {
        // `claude -p "<prompt>"` authenticates via the user's subscription with
        // no API key (verified). stdout is the clean answer.
        Some(crate::agents::HeadlessPrintCommand {
            binary: "claude".into(),
            args: vec!["-p".into()],
            output: crate::agents::HeadlessOutput::Stdout,
        })
    }

    fn transport(&self) -> Transport {
        let settings_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".claude/settings.json");
        Transport::FileHooks {
            settings_path,
            hook_event_names: vec![
                "SessionStart",
                "SessionEnd",
                "PreToolUse",
                "PostToolUse",
                "TaskCreated",
                "TaskCompleted",
                "Stop",
                "UserPromptSubmit",
                "PermissionRequest",
                "CwdChanged",
                "FileChanged",
                "PermissionDenied",
            ],
        }
    }

    fn requires_nergal_setup(&self) -> bool {
        true
    }

    async fn detect(&self) -> DetectionResult {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = home.join(".claude");
        let binary_path = which::which("claude").ok();
        // PATH lookup is authoritative — config dir alone (a leftover from a
        // prior install) shouldn't mark the agent as installed.
        let installed = binary_path.is_some();
        DetectionResult {
            installed,
            binary_path,
            config_path: if config_dir.exists() {
                Some(config_dir)
            } else {
                None
            },
            version: None,
            trusted_for_project: None,
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let mut cmd = tokio::process::Command::new("claude");
        crate::platform_spawn::NoWindow::no_window(&mut cmd);
        let out = cmd.arg("--version").output().await.ok()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() { None } else { Some(raw) }
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
        let binary = which::which("claude")
            .map_err(|e| AdapterError::Transport(anyhow::anyhow!("claude not on PATH: {e}")))?;
        let mut args: Vec<String> = Vec::new();
        // Three resume modes — the caller chooses which by what it puts in
        // `resume_from`:
        //   - "continue"    → `claude --continue`     (latest session)
        //   - "resume_pick" → `claude --resume`       (Claude prompts to pick)
        //   - "<any other>" → `claude --resume <id>`  (specific CC session id)
        match ctx.resume_from {
            None => {}
            Some("continue") => args.push("--continue".into()),
            Some("resume_pick") => args.push("--resume".into()),
            Some(id) => {
                args.push("--resume".into());
                args.push(id.to_string());
            }
        }
        // Permission preset → native CC flags. One mode per session:
        // `--dangerously-skip-permissions` is the documented equivalent of
        // `--permission-mode bypassPermissions`, so it is NOT combinable
        // with plan/acceptEdits (bypass silently wins — verified 2026-06-06
        // against the CLI reference). Valid alongside --resume/--continue;
        // must precede the positional prompt below.
        if let Some(opts) = ctx.launch_options {
            match opts.permission_preset {
                crate::models::PermissionPreset::Default => {}
                crate::models::PermissionPreset::Plan => {
                    args.push("--permission-mode".into());
                    args.push("plan".into());
                }
                crate::models::PermissionPreset::AcceptEdits => {
                    args.push("--permission-mode".into());
                    args.push("acceptEdits".into());
                }
                crate::models::PermissionPreset::Auto => {
                    args.push("--permission-mode".into());
                    args.push("auto".into());
                }
                crate::models::PermissionPreset::Bypass => {
                    args.push("--dangerously-skip-permissions".into());
                }
            }
            // Adds bypass to the Shift+Tab cycle without starting in it —
            // redundant (and skipped) when already starting in bypass.
            if opts.allow_skip_in_cycle
                && opts.permission_preset != crate::models::PermissionPreset::Bypass
            {
                args.push("--allow-dangerously-skip-permissions".into());
            }
        }
        // Pinned-note context rides as an ephemeral system prompt, kept
        // distinct from the user's first turn and isolated per session. Flag
        // must precede the positional prompt below.
        if let Some(context) = ctx.injected_context.filter(|c| !c.is_empty()) {
            let path = crate::agents::spawn_context_file(ctx.session_id);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(AdapterError::Io)?;
            }
            std::fs::write(&path, context).map_err(AdapterError::Io)?;
            args.push("--append-system-prompt-file".into());
            args.push(path.display().to_string());
        }
        // `claude [prompt]` starts an interactive session with the prompt
        // submitted — so a deep-link `session/new` prompt rides the launch
        // command instead of being typed in after the REPL is up. Resume modes
        // are mutually exclusive with a fresh prompt in practice (session/new
        // never resumes), so appending after the resume args is safe.
        if let Some(prompt) = ctx.initial_prompt.filter(|p| !p.is_empty()) {
            args.push(prompt.to_string());
        }
        let mut env = HashMap::new();
        env.insert("NERGAL_SESSION_ID".into(), ctx.session_id.to_string());
        Ok(SpawnSpec { binary, args, env })
    }

    fn permission_presets(&self) -> &'static [crate::models::PermissionPreset] {
        use crate::models::PermissionPreset as P;
        &[P::Default, P::Plan, P::AcceptEdits, P::Auto, P::Bypass]
    }

    fn supports_allow_skip_cycle(&self) -> bool {
        true
    }

    fn context_injection(&self) -> ContextInjection {
        ContextInjection::AppendSystemPromptFile
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        cost::parse_cost_line(line).map(TranscriptEvent::Cost)
    }

    async fn start_event_pump(
        &self,
        _session_id: &str,
        _sink: EventSink,
    ) -> Result<(), AdapterError> {
        // CC's transcript and plan watchers run app-globally today (set up in
        // lib.rs at startup). The session-scoped lifecycle envisioned in
        // Decision 12 is migration work that lands once the hook dispatcher
        // routes through the adapter — at that point this method takes over
        // ownership. For zero-regression today, keeping it a no-op preserves
        // current behavior.
        Ok(())
    }

    fn plan_capability(&self, _session: &Session, cwd: &Path) -> PlanCapability {
        PlanCapability::FileBased {
            dir: resolve_cc_plans_directory(cwd),
            label: "Claude Code".to_string(),
        }
    }

    async fn submit_plan_decision(
        &self,
        session_id: &str,
        decision: PlanDecision,
    ) -> Result<(), AdapterError> {
        let Some((_, fifo_path)) = self.pending_plan_fifos.remove(session_id) else {
            return Err(AdapterError::SessionLocked);
        };
        let body = if decision.approved {
            serde_json::json!({ "approved": true })
        } else {
            let msg = decision
                .message
                .unwrap_or_else(|| "Plan changes requested".to_string());
            serde_json::json!({ "approved": false, "message": msg })
        };
        tokio::fs::write(&fifo_path, body.to_string())
            .await
            .map_err(AdapterError::Io)?;
        Ok(())
    }

    async fn submit_ask_answer(
        &self,
        session_id: &str,
        answers: serde_json::Value,
    ) -> Result<(), AdapterError> {
        let Some((_, fifo_path)) = self.pending_ask_fifos.remove(session_id) else {
            return Err(AdapterError::SessionLocked);
        };
        let body = serde_json::json!({ "answers": answers });
        tokio::fs::write(&fifo_path, body.to_string())
            .await
            .map_err(AdapterError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn capabilities_include_all_known_flags() {
        let a = ClaudeCodeAdapter::new();
        let caps = a.capabilities().flags;
        assert!(caps.contains(AgentCapability::PLAN_REVIEW));
        assert!(caps.contains(AgentCapability::ASK_USER_BLOCKING));
        assert!(caps.contains(AgentCapability::TASK_LIST));
        assert!(caps.contains(AgentCapability::ANNOTATIONS_INJECT));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = ClaudeCodeAdapter::new();
        assert_eq!(a.id().as_str(), "claude-code");
        assert_eq!(a.display_name(), "Claude Code");
    }

    #[test]
    fn transport_is_file_hooks_with_settings_json() {
        let a = ClaudeCodeAdapter::new();
        match a.transport() {
            Transport::FileHooks {
                settings_path,
                hook_event_names,
            } => {
                assert!(settings_path.ends_with(".claude/settings.json"));
                assert!(hook_event_names.contains(&"SessionStart"));
                assert!(hook_event_names.contains(&"PreToolUse"));
            }
            other => panic!("expected FileHooks, got {other:?}"),
        }
    }

    #[test]
    fn parse_transcript_line_emits_cost_for_usage_lines() {
        let a = ClaudeCodeAdapter::new();
        let line = r#"{"message":{"model":"claude-sonnet-4","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;
        let ev = a.parse_transcript_line(line).expect("expected Some");
        match ev {
            TranscriptEvent::Cost(raw) => {
                assert_eq!(raw.input_tokens, 10);
                assert_eq!(raw.output_tokens, 20);
            }
            other => panic!("expected Cost, got {other:?}"),
        }
    }

    #[test]
    fn parse_transcript_line_returns_none_for_user_message() {
        let a = ClaudeCodeAdapter::new();
        let line = r#"{"message":{"role":"user","content":"hi"}}"#;
        assert!(a.parse_transcript_line(line).is_none());
    }

    #[test]
    fn spawn_includes_nergal_session_id_env() {
        let a = ClaudeCodeAdapter::new();
        let cwd = Path::new("/tmp");
        let ctx = SpawnContext {
            session_id: "abc-123",
            cwd,
            resume_from: None,
            initial_prompt: None,
            injected_context: None,
            launch_options: None,
        };
        // `claude` may not exist on this machine; if it doesn't, the spawn
        // returns Transport(NotFound). Either way, the session_id env wiring
        // is a contract we can assert on the success path only.
        if let Ok(spec) = a.spawn(&ctx) {
            assert_eq!(
                spec.env.get("NERGAL_SESSION_ID").map(String::as_str),
                Some("abc-123")
            );
        }
    }

    #[test]
    fn spawn_resume_modes_map_to_correct_cc_flags() {
        let a = ClaudeCodeAdapter::new();
        let cwd = Path::new("/tmp");
        let mk = |resume: Option<&'static str>| SpawnContext {
            session_id: "s",
            cwd,
            resume_from: resume,
            initial_prompt: None,
            injected_context: None,
            launch_options: None,
        };
        // None → no resume flag
        if let Ok(spec) = a.spawn(&mk(None)) {
            assert!(spec.args.is_empty());
        }
        // "continue" → --continue
        if let Ok(spec) = a.spawn(&mk(Some("continue"))) {
            assert_eq!(spec.args, vec!["--continue".to_string()]);
        }
        // "resume_pick" → bare --resume (CC then prompts)
        if let Ok(spec) = a.spawn(&mk(Some("resume_pick"))) {
            assert_eq!(spec.args, vec!["--resume".to_string()]);
        }
        // arbitrary id → --resume <id>
        if let Ok(spec) = a.spawn(&mk(Some("abc-123"))) {
            assert_eq!(
                spec.args,
                vec!["--resume".to_string(), "abc-123".to_string()]
            );
        }
    }

    #[test]
    fn spawn_maps_permission_presets_to_cc_flags() {
        use crate::models::{LaunchOptions, PermissionPreset};
        let a = ClaudeCodeAdapter::new();
        let cwd = Path::new("/tmp");
        let mk = |preset: PermissionPreset| LaunchOptions {
            permission_preset: preset,
            allow_skip_in_cycle: false,
            startup_command: None,
        };
        let spawn_with = |opts: &LaunchOptions| {
            a.spawn(&SpawnContext {
                session_id: "s",
                cwd,
                resume_from: None,
                initial_prompt: None,
                injected_context: None,
                launch_options: Some(opts),
            })
        };
        if let Ok(spec) = spawn_with(&mk(PermissionPreset::Default)) {
            assert!(spec.args.is_empty());
        }
        if let Ok(spec) = spawn_with(&mk(PermissionPreset::Plan)) {
            assert_eq!(spec.args, vec!["--permission-mode", "plan"]);
        }
        if let Ok(spec) = spawn_with(&mk(PermissionPreset::AcceptEdits)) {
            assert_eq!(spec.args, vec!["--permission-mode", "acceptEdits"]);
        }
        if let Ok(spec) = spawn_with(&mk(PermissionPreset::Auto)) {
            assert_eq!(spec.args, vec!["--permission-mode", "auto"]);
        }
        if let Ok(spec) = spawn_with(&mk(PermissionPreset::Bypass)) {
            assert_eq!(spec.args, vec!["--dangerously-skip-permissions"]);
        }
        // allow_skip_in_cycle composes with a non-bypass preset…
        let plan_plus_allow = LaunchOptions {
            permission_preset: PermissionPreset::Plan,
            allow_skip_in_cycle: true,
            startup_command: None,
        };
        if let Ok(spec) = spawn_with(&plan_plus_allow) {
            assert_eq!(
                spec.args,
                vec![
                    "--permission-mode",
                    "plan",
                    "--allow-dangerously-skip-permissions"
                ]
            );
        }
        // …and is dropped as redundant when already starting in bypass.
        let bypass_plus_allow = LaunchOptions {
            permission_preset: PermissionPreset::Bypass,
            allow_skip_in_cycle: true,
            startup_command: None,
        };
        if let Ok(spec) = spawn_with(&bypass_plus_allow) {
            assert_eq!(spec.args, vec!["--dangerously-skip-permissions"]);
        }
    }

    #[test]
    fn spawn_appends_initial_prompt_as_positional_arg() {
        let a = ClaudeCodeAdapter::new();
        let cwd = Path::new("/tmp");
        let ctx = SpawnContext {
            session_id: "s",
            cwd,
            resume_from: None,
            initial_prompt: Some("implement auth"),
            injected_context: None,
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&ctx) {
            assert_eq!(spec.args, vec!["implement auth".to_string()]);
        }
        // Empty prompt is not appended.
        let empty = SpawnContext {
            session_id: "s",
            cwd,
            resume_from: None,
            initial_prompt: Some(""),
            injected_context: None,
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&empty) {
            assert!(spec.args.is_empty());
        }
    }

    #[test]
    fn spawn_injects_context_as_append_system_prompt_file() {
        let a = ClaudeCodeAdapter::new();
        let cwd = Path::new("/tmp");
        let ctx = SpawnContext {
            session_id: "cc-inject-test",
            cwd,
            resume_from: None,
            initial_prompt: None,
            injected_context: Some("# Pinned vault context\n\nhello"),
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&ctx) {
            let path = crate::agents::spawn_context_file("cc-inject-test");
            let path_str = path.display().to_string();
            assert!(spec.args.iter().any(|x| x == "--append-system-prompt-file"));
            assert!(spec.args.iter().any(|x| x == &path_str));
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "# Pinned vault context\n\nhello"
            );
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn context_injection_tier_is_append_system_prompt_file() {
        assert_eq!(
            ClaudeCodeAdapter::new().context_injection(),
            ContextInjection::AppendSystemPromptFile
        );
    }

    #[test]
    fn spawn_without_context_omits_append_flag() {
        let a = ClaudeCodeAdapter::new();
        let cwd = Path::new("/tmp");
        let ctx = SpawnContext {
            session_id: "cc-no-inject",
            cwd,
            resume_from: None,
            initial_prompt: None,
            injected_context: None,
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&ctx) {
            assert!(!spec.args.iter().any(|x| x == "--append-system-prompt-file"));
        }
    }

    #[tokio::test]
    async fn submit_plan_decision_without_registered_fifo_returns_session_locked() {
        let a = ClaudeCodeAdapter::new();
        let err = a
            .submit_plan_decision(
                "no-such-session",
                PlanDecision {
                    approved: true,
                    message: None,
                },
            )
            .await
            .unwrap_err();
        match err {
            AdapterError::SessionLocked => {}
            other => panic!("expected SessionLocked, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn submit_plan_decision_writes_to_registered_fifo() {
        // Use a regular file (not a FIFO) — tokio::fs::write doesn't care; the
        // production code path uses an actual FIFO created by `mkfifo`.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan-decision");
        let a = ClaudeCodeAdapter::new();
        a.register_pending_plan_fifo("s1", path.clone());
        a.submit_plan_decision(
            "s1",
            PlanDecision {
                approved: false,
                message: Some("revise step 3".into()),
            },
        )
        .await
        .unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v.get("approved").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            v.get("message").and_then(|v| v.as_str()),
            Some("revise step 3")
        );
    }
}
