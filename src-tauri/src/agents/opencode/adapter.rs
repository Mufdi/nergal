//! OpenCode adapter — runs the `opencode` TUI inside the nergal terminal
//! and listens to its embedded Hono server's SSE stream for events.
//!
//! The TUI binary serves the same HTTP API as `opencode serve`. We pin its
//! port via `--port <X>` so the SSE consumer can connect to a known address.
//! Events translated from `/event` flow through the runtime sink into the
//! same dispatcher CC's socket events use, so the Modified Files / Activity
//! panels light up identically — without restoring the old chat-panel UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use super::sse_client::{SessionIdMap, SseClient};
use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, ContextInjection,
    DetectionResult, EventSink, PlanCapability, SpawnContext, SpawnSpec, ThemePalette,
    TranscriptEvent, Transport, fold_prompt_preamble, write_atomic,
};
use crate::models::Session;

/// Port range reserved for OpenCode TUI sessions. Stays well above the
/// reserved range and out of common dev-server territory (3000s/8000s).
const PORT_BASE: u16 = 41000;
const PORT_SPAN: u16 = 1000;

pub struct OpenCodeAdapter {
    capabilities: AgentCapabilities,
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    config_paths: Vec<PathBuf>,
    /// Root of the opencode user config (`~/.config/opencode` in practice).
    /// Captured at construction so `apply_theme` writes into a hermetic
    /// location in tests without relying on a process-global HOME.
    config_root: PathBuf,
    /// `nergal_session_id` → bound port. Set in [`Self::spawn`] and read by
    /// [`Self::start_event_pump`] so the SSE consumer connects to the right
    /// address without re-deriving it.
    session_ports: Arc<DashMap<String, u16>>,
    sse_clients: Arc<DashMap<String, SseClient>>,
    /// `nergal_session_id` → OpenCode-side session id, harvested from the
    /// `session.created` SSE event. Surfaced via [`Self::agent_internal_session_id`]
    /// so the runtime can persist it and resume via `--session <id>`, which
    /// is more reliable than `--continue` (the OpenCode CLI's "last session"
    /// flag is global and doesn't reliably scope by cwd across restarts).
    session_internal_ids: SessionIdMap,
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        // opencode resolves its config/data dirs via the `xdg-basedir` package,
        // which keeps the Linux XDG layout on macOS too — `~/.config/opencode`
        // + `~/.local/share/opencode`, NOT `~/Library/Application Support`
        // (that path is opencode's org-level *managed* settings, not user
        // config). Verified against opencode docs + sst/opencode#8235. So these
        // paths are correct on macOS unchanged — do NOT "fix" them to
        // `dirs::config_dir()`, which would point at the wrong directory.
        let home = dirs::home_dir().unwrap_or_default();
        Self::with_config_root(home.join(".config/opencode"))
    }

    /// Constructor for tests that need a hermetic config root. Production
    /// code always goes through [`Self::new`].
    fn with_config_root(config_root: PathBuf) -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            capabilities: AgentCapabilities {
                // Now that the SSE consumer is wired, re-enable the
                // capabilities the chat-panel era exposed: tool-call events,
                // raw cost per message, and ASK_USER_BLOCKING flow back in
                // a follow-up. SESSION_RESUME stays — the TUI accepts
                // --continue / --session <id>.
                flags: AgentCapability::SESSION_RESUME
                    | AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT
                    | AgentCapability::THEME_SYNC,
                supported_models: vec![],
            },
            binary_path: parking_lot::RwLock::new(which::which("opencode").ok()),
            config_paths: vec![config_root.clone(), home.join(".local/share/opencode")],
            config_root,
            session_ports: Arc::new(DashMap::new()),
            sse_clients: Arc::new(DashMap::new()),
            session_internal_ids: Arc::new(DashMap::new()),
        }
    }

    fn cached_binary(&self) -> Option<PathBuf> {
        self.binary_path.read().clone()
    }

    /// Pick a port deterministically from the session id so a session that
    /// reconnects after a frontend reload hits the same port. Multiple
    /// sessions hashing to the same port is unlikely (1/PORT_SPAN) and
    /// recovery is cheap (kill+respawn the TUI).
    fn port_for(session_id: &str) -> u16 {
        let mut hash: u32 = 5381;
        for b in session_id.as_bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(*b as u32);
        }
        PORT_BASE + (hash % PORT_SPAN as u32) as u16
    }
}

#[async_trait]
impl AgentAdapter for OpenCodeAdapter {
    fn id(&self) -> AgentId {
        AgentId::opencode()
    }

    fn display_name(&self) -> &str {
        "OpenCode"
    }

    fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    fn headless_print_command(&self) -> Option<crate::agents::HeadlessPrintCommand> {
        // `opencode run` prints a banner on stdout; `--format json` emits clean
        // JSONL the summarizer parses for the text parts (+ token totals).
        Some(crate::agents::HeadlessPrintCommand {
            binary: "opencode".into(),
            args: vec!["run".into(), "--format".into(), "json".into()],
            output: crate::agents::HeadlessOutput::OpencodeJsonl,
        })
    }

    fn transport(&self) -> Transport {
        Transport::HttpSse {
            base_url_template: "http://127.0.0.1::port".into(),
            sse_path: "/event",
            permission_endpoint: "/session/{sessionID}/permissions/{permissionID}",
            auth: Some(crate::agents::AuthScheme::None),
        }
    }

    fn requires_nergal_setup(&self) -> bool {
        false
    }

    async fn detect(&self) -> DetectionResult {
        let binary_path = self
            .cached_binary()
            .or_else(|| which::which("opencode").ok());
        let config_path = self.config_paths.iter().find(|p| p.exists()).cloned();
        DetectionResult {
            installed: binary_path.is_some(),
            binary_path,
            config_path,
            version: None,
            trusted_for_project: None,
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let binary = self.cached_binary()?;
        let mut cmd = tokio::process::Command::new(&binary);
        crate::platform_spawn::NoWindow::no_window(&mut cmd);
        let out = cmd.arg("--version").output().await.ok()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() { None } else { Some(raw) }
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
        let binary = self.cached_binary().ok_or_else(|| {
            AdapterError::Transport(anyhow::anyhow!("opencode binary not found on PATH"))
        })?;

        // Pin the TUI's embedded server to a known port so the SSE consumer
        // can attach. Stash for `start_event_pump`.
        let port = Self::port_for(ctx.session_id);
        self.session_ports.insert(ctx.session_id.to_string(), port);

        let mut args: Vec<String> = vec![
            "--port".into(),
            port.to_string(),
            "--hostname".into(),
            "127.0.0.1".into(),
        ];

        // Resume sentinels shared with the rest of the adapters:
        //   - "continue"    → `--continue`        (latest session)
        //   - "<any other>" → `--session <id>`    (specific session)
        //   - "resume_pick" → not supported (no picker UI); falls back to
        //                    `--continue`.
        match ctx.resume_from {
            None => {}
            Some("continue") | Some("resume_pick") => args.push("--continue".into()),
            Some(id) => {
                args.push("--session".into());
                args.push(id.to_string());
            }
        }
        // Permission preset: only Plan maps — `--agent plan` selects the
        // built-in read-only plan agent (verified in `opencode --help`
        // 2026-06-06). AcceptEdits/Bypass have no CLI flag (permissions are
        // opencode.json config) and are skipped.
        if let Some(opts) = ctx.launch_options
            && opts.permission_preset == crate::models::PermissionPreset::Plan
        {
            args.push("--agent".into());
            args.push("plan".into());
        }
        // `--prompt` rides only a FRESH launch: it lands as the agent's first
        // turn, so re-submitting it on resume injects a stray turn into the
        // existing conversation (observed to hang the TUI). On resume the
        // context already lives in history from the original spawn.
        if ctx.resume_from.is_none()
            && let Some(text) = fold_prompt_preamble(ctx.injected_context, ctx.initial_prompt)
        {
            args.push("--prompt".into());
            args.push(text);
        }
        let mut env = HashMap::new();
        env.insert("NERGAL_SESSION_ID".into(), ctx.session_id.to_string());
        Ok(SpawnSpec { binary, args, env })
    }

    fn permission_presets(&self) -> &'static [crate::models::PermissionPreset] {
        use crate::models::PermissionPreset as P;
        // AcceptEdits/Bypass have no CLI flag (permissions live in
        // opencode.json config); Plan maps to `--agent plan`.
        &[P::Default, P::Plan]
    }

    fn context_injection(&self) -> ContextInjection {
        ContextInjection::PromptPreamble
    }

    fn parse_transcript_line(&self, _line: &str) -> Option<TranscriptEvent> {
        None
    }

    fn plan_capability(&self, _session: &Session, _cwd: &Path) -> PlanCapability {
        // SCOPE: opencode plan mode is read-only and does not persist plans
        // to disk by default. The `.opencode/plans/*.md` write path is
        // documented but the model refuses to use it (issue
        // anomalyco/opencode#11078). Revisit if upstream adds reliable
        // automatic persistence.
        PlanCapability::NotApplicable
    }

    async fn start_event_pump(
        &self,
        session_id: &str,
        sink: EventSink,
    ) -> Result<(), AdapterError> {
        // Idempotent: a second call while a client is already running would
        // open a second `/event` subscription on the same port. Today the
        // duplication is mostly invisible (live-only stream, no catch-up)
        // but a long-lived second client still emits future events twice
        // until the first is replaced via DashMap insert. Mirrors the Pi
        // adapter's guard.
        if self.sse_clients.contains_key(session_id) {
            return Ok(());
        }

        // Read the port the spawn step picked — defensive lookup with a
        // fallback to the deterministic hash so a hot-reload of the
        // frontend (which may skip spawn) still finds the right address.
        let port = self
            .session_ports
            .get(session_id)
            .map(|r| *r)
            .unwrap_or_else(|| Self::port_for(session_id));

        let base_url = format!("http://127.0.0.1:{port}");
        let client = SseClient::spawn(
            base_url,
            session_id.to_string(),
            sink,
            self.session_internal_ids.clone(),
        );
        self.sse_clients.insert(session_id.to_string(), client);
        Ok(())
    }

    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> {
        if let Some((_, client)) = self.sse_clients.remove(session_id) {
            client.cancel().await;
        }
        self.session_ports.remove(session_id);
        // session_internal_ids stays — the runtime persists the id onto the
        // session row before teardown so resume continues to work.
        Ok(())
    }

    fn agent_internal_session_id(&self, nergal_session_id: &str) -> Option<String> {
        self.session_internal_ids
            .get(nergal_session_id)
            .map(|r| r.clone())
    }

    async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError> {
        let themes_dir = self.config_root.join("themes");
        let target = themes_dir.join("nergal-active.json");
        let body = serde_json::to_vec_pretty(&build_opencode_theme(palette))
            .map_err(|e| AdapterError::Transport(anyhow::anyhow!(e)))?;
        write_atomic(&target, &body).await?;
        let tui_json = self.config_root.join("tui.json");
        let pointed_at_us =
            crate::agents::update_settings_theme_if_safe(&tui_json, "nergal-active").await?;
        if !pointed_at_us {
            tracing::debug!(
                "opencode tui.json points at a user-selected theme; nergal-active.json written but inactive"
            );
            return Ok(());
        }
        for entry in self.session_ports.iter() {
            let port = *entry.value();
            let url = format!("http://127.0.0.1:{port}/tui/execute-command");
            let body = serde_json::json!({ "command": "theme nergal-active" });
            // Best-effort: the live-switch endpoint isn't documented as
            // accepting an argument. Failure (timeout, 4xx, connection
            // refused) falls back to next-spawn via tui.json.
            let _ = reqwest::Client::new()
                .post(&url)
                .json(&body)
                .timeout(std::time::Duration::from_millis(1500))
                .send()
                .await;
        }
        Ok(())
    }
}

/// Translate a `ThemePalette` into the opencode theme schema
/// (`https://opencode.ai/theme.json`).
///
/// `defs` carries the nergal palette as named refs; each `theme.*` token
/// points at one of those refs. Required tokens per schema: primary,
/// secondary, accent, text, textMuted, background. We populate the full
/// UI/markdown/syntax set so opencode renders coherently against nergal's
/// chrome.
fn build_opencode_theme(palette: &ThemePalette) -> serde_json::Value {
    let mut defs = serde_json::Map::new();
    defs.insert("bg".into(), palette.surface.as_str().into());
    defs.insert("card".into(), palette.card.as_str().into());
    defs.insert("secondary".into(), palette.secondary.as_str().into());
    defs.insert("fg".into(), palette.foreground.as_str().into());
    defs.insert("muted".into(), palette.muted_foreground.as_str().into());
    // No "gray" def: opentui 1.17.7 ships a builtin "gray", and defining our own
    // (whose value can itself be the named color "gray") makes the resolver hit a
    // self-cycle (`muted -> gray -> gray`). The 1.16.2 "gray not found" abort that
    // originally justified this def no longer occurs now that gray is builtin.
    defs.insert("accent".into(), palette.accent.as_str().into());
    defs.insert("success".into(), "#22c55e".into());
    defs.insert("error".into(), "#ef4444".into());
    defs.insert("warning".into(), "#f59e0b".into());
    defs.insert("info".into(), palette.accent.as_str().into());

    let pairs: &[(&str, &str)] = &[
        ("primary", "accent"),
        ("secondary", "secondary"),
        ("accent", "accent"),
        ("error", "error"),
        ("warning", "warning"),
        ("success", "success"),
        ("info", "info"),
        ("text", "fg"),
        ("textMuted", "muted"),
        ("background", "bg"),
        ("backgroundPanel", "card"),
        ("backgroundElement", "secondary"),
        ("border", "muted"),
        ("borderActive", "accent"),
        ("borderSubtle", "muted"),
        ("diffAdded", "success"),
        ("diffRemoved", "error"),
        ("diffContext", "muted"),
        ("diffHunkHeader", "accent"),
        ("diffLineNumber", "muted"),
        ("markdownText", "fg"),
        ("markdownHeading", "accent"),
        ("markdownLink", "accent"),
        ("markdownLinkText", "accent"),
        ("markdownCode", "accent"),
        ("markdownBlockQuote", "muted"),
        ("markdownEmph", "accent"),
        ("markdownStrong", "fg"),
        ("markdownHorizontalRule", "muted"),
        ("markdownListItem", "fg"),
        ("markdownListEnumeration", "accent"),
        ("markdownImage", "accent"),
        ("markdownImageText", "muted"),
        ("markdownCodeBlock", "fg"),
        ("syntaxComment", "muted"),
        ("syntaxKeyword", "accent"),
        ("syntaxFunction", "accent"),
        ("syntaxVariable", "fg"),
        ("syntaxString", "success"),
        ("syntaxNumber", "fg"),
        ("syntaxType", "accent"),
        ("syntaxOperator", "muted"),
        ("syntaxPunctuation", "muted"),
    ];
    let mut theme = serde_json::Map::new();
    for (k, v) in pairs {
        theme.insert((*k).into(), serde_json::Value::String((*v).to_string()));
    }

    let mut root = serde_json::Map::new();
    root.insert("$schema".into(), "https://opencode.ai/theme.json".into());
    root.insert("defs".into(), serde_json::Value::Object(defs));
    root.insert("theme".into(), serde_json::Value::Object(theme));
    serde_json::Value::Object(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_injection_tier_is_prompt_preamble() {
        assert_eq!(
            OpenCodeAdapter::new().context_injection(),
            ContextInjection::PromptPreamble
        );
    }

    #[test]
    fn capabilities_match_terminal_plus_sse_flow() {
        let a = OpenCodeAdapter::new();
        let caps = a.capabilities().flags;
        assert!(caps.contains(AgentCapability::SESSION_RESUME));
        assert!(caps.contains(AgentCapability::TOOL_CALL_EVENTS));
        // No native picker on the OpenCode CLI.
        assert!(!caps.contains(AgentCapability::SESSION_PICKER));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = OpenCodeAdapter::new();
        assert_eq!(a.id().as_str(), "opencode");
        assert_eq!(a.display_name(), "OpenCode");
    }

    #[test]
    fn requires_nergal_setup_is_false_for_opencode() {
        let a = OpenCodeAdapter::new();
        assert!(!a.requires_nergal_setup());
    }

    #[test]
    fn parse_transcript_line_always_returns_none() {
        let a = OpenCodeAdapter::new();
        let line = r#"{"type":"session.idle","properties":{"sessionID":"ses_x"}}"#;
        assert!(a.parse_transcript_line(line).is_none());
    }

    #[test]
    fn port_for_is_deterministic_and_in_range() {
        let p1 = OpenCodeAdapter::port_for("ses-abc");
        let p2 = OpenCodeAdapter::port_for("ses-abc");
        assert_eq!(p1, p2);
        assert!((PORT_BASE..PORT_BASE + PORT_SPAN).contains(&p1));
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

    #[test]
    fn build_opencode_theme_includes_required_token_set() {
        let json = build_opencode_theme(&sample_palette());
        assert_eq!(json["$schema"], "https://opencode.ai/theme.json");
        let theme = json
            .get("theme")
            .and_then(|t| t.as_object())
            .expect("theme block");
        for required in [
            "primary",
            "secondary",
            "accent",
            "text",
            "textMuted",
            "background",
        ] {
            assert!(
                theme.contains_key(required),
                "missing required opencode token: {required}"
            );
        }
        let defs = json
            .get("defs")
            .and_then(|d| d.as_object())
            .expect("defs block");
        assert_eq!(defs["bg"], "#1a1b26");
        assert_eq!(defs["accent"], "#7aa2f7");
        assert_eq!(theme["background"], "bg");
        assert_eq!(theme["accent"], "accent");
    }

    #[tokio::test]
    async fn apply_theme_writes_expected_files() {
        let root = tempfile::tempdir().unwrap();
        let adapter = OpenCodeAdapter::with_config_root(root.path().to_path_buf());
        adapter.apply_theme(&sample_palette()).await.unwrap();
        let theme_path = root.path().join("themes/nergal-active.json");
        let raw = tokio::fs::read_to_string(&theme_path).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["$schema"], "https://opencode.ai/theme.json");
        assert_eq!(parsed["defs"]["accent"], "#7aa2f7");
        let tui_raw = tokio::fs::read_to_string(root.path().join("tui.json"))
            .await
            .unwrap();
        let tui: serde_json::Value = serde_json::from_str(&tui_raw).unwrap();
        assert_eq!(tui["theme"], "nergal-active");
    }

    #[tokio::test]
    async fn apply_theme_preserves_existing_tui_json_keys() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(root.path()).await.unwrap();
        tokio::fs::write(root.path().join("tui.json"), br#"{"layout":"split"}"#)
            .await
            .unwrap();
        let adapter = OpenCodeAdapter::with_config_root(root.path().to_path_buf());
        adapter.apply_theme(&sample_palette()).await.unwrap();
        let raw = tokio::fs::read_to_string(root.path().join("tui.json"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["theme"], "nergal-active");
        assert_eq!(parsed["layout"], "split");
    }

    #[tokio::test]
    async fn apply_theme_does_not_overwrite_user_theme_choice() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(root.path()).await.unwrap();
        tokio::fs::write(root.path().join("tui.json"), br#"{"theme":"tokyonight"}"#)
            .await
            .unwrap();
        let adapter = OpenCodeAdapter::with_config_root(root.path().to_path_buf());
        adapter.apply_theme(&sample_palette()).await.unwrap();
        let raw = tokio::fs::read_to_string(root.path().join("tui.json"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["theme"], "tokyonight");
        assert!(
            root.path().join("themes/nergal-active.json").exists(),
            "theme file written even when user opted out"
        );
    }

    #[test]
    fn spawn_emits_port_and_hostname_flags() {
        let a = OpenCodeAdapter::new();
        let cwd = std::path::Path::new("/tmp");
        let ctx = SpawnContext {
            session_id: "ses-spawn",
            cwd,
            resume_from: None,
            initial_prompt: None,
            injected_context: None,
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&ctx) {
            assert!(spec.args.iter().any(|a| a == "--port"));
            assert!(spec.args.iter().any(|a| a == "--hostname"));
            assert!(spec.args.iter().any(|a| a == "127.0.0.1"));
        }
    }

    #[test]
    fn spawn_resume_modes_map_to_correct_opencode_flags() {
        let a = OpenCodeAdapter::new();
        let cwd = std::path::Path::new("/tmp");
        let mk = |resume: Option<&'static str>| SpawnContext {
            session_id: "s",
            cwd,
            resume_from: resume,
            initial_prompt: None,
            injected_context: None,
            launch_options: None,
        };
        if let Ok(spec) = a.spawn(&mk(Some("continue"))) {
            assert!(spec.args.iter().any(|a| a == "--continue"));
        }
        if let Ok(spec) = a.spawn(&mk(Some("resume_pick"))) {
            assert!(spec.args.iter().any(|a| a == "--continue"));
        }
        if let Ok(spec) = a.spawn(&mk(Some("abc-uuid"))) {
            assert!(spec.args.iter().any(|a| a == "--session"));
            assert!(spec.args.iter().any(|a| a == "abc-uuid"));
        }
    }
}
