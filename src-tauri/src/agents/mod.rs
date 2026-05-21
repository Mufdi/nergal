//! Agent-agnostic foundation.
//!
//! Cluihud was originally a Claude Code (CC) wrapper. This module introduces
//! the [`AgentAdapter`] trait and supporting types so that other agent CLIs
//! (OpenCode, Pi, Codex, …) can plug in without duplicating the integration
//! per-agent. CC remains first-class as the first adapter.
//!
//! Core concepts:
//! - [`AgentAdapter`] — trait each agent implements; declares its [`Transport`]
//!   as data, not as monomorphism (Decision 1 in `design.md`).
//! - [`AgentCapability`] — bitset of features the adapter exposes; UI gates by
//!   bit presence (Decision 2). No `Option<fn>` proliferation in the trait.
//! - [`AgentId`] — string newtype validated against `^[a-z][a-z0-9-]{0,31}$`
//!   so adapter IDs are filesystem-safe and DB-safe (Decision 3).
//! - [`SessionCostAggregator`] — owns running per-session token totals; agents
//!   only emit raw per-line cost via [`TranscriptEvent::Cost`] (Decision 6).

pub mod claude_code;
pub mod codex;
pub mod cost_aggregator;
pub mod opencode;
pub mod pi;
pub mod registry;
pub mod state;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Errors raised by adapter implementations and the registry.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("capability not supported by this adapter: {0:?}")]
    NotSupported(AgentCapability),
    #[error("session is not in a state that allows this operation")]
    SessionLocked,
    #[error("invalid agent id: {0}")]
    InvalidAgentId(String),
    #[error("agent id already registered: {0:?}")]
    DuplicateAgentId(AgentId),
    #[error("transport error: {0}")]
    Transport(#[from] anyhow::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Stable string identifier for an agent adapter.
///
/// Validated against `^[a-z][a-z0-9-]{0,31}$` (1-32 chars, must start with a
/// lowercase letter). Used in DB rows, env vars (`CLUIHUD_AGENT_ID`), FIFO
/// filenames and state-directory paths — the conservative charset closes the
/// injection surface for future plugin adapters.
#[derive(Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentId(String);

impl AgentId {
    /// Construct from arbitrary input. Validates against the agent-id regex.
    pub fn new(s: &str) -> Result<Self, AdapterError> {
        let re = regex::Regex::new(r"^[a-z][a-z0-9-]{0,31}$").unwrap();
        if !re.is_match(s) {
            return Err(AdapterError::InvalidAgentId(s.into()));
        }
        Ok(Self(s.into()))
    }

    pub fn claude_code() -> Self {
        Self("claude-code".into())
    }
    pub fn opencode() -> Self {
        Self("opencode".into())
    }
    pub fn pi() -> Self {
        Self("pi".into())
    }
    pub fn codex() -> Self {
        Self("codex".into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

bitflags::bitflags! {
    /// Feature set declared by an adapter. UI gates panels by checking flag
    /// presence; the trait does not declare `Option<fn>` per feature.
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    pub struct AgentCapability: u32 {
        const PLAN_REVIEW           = 1 << 0;
        const ASK_USER_BLOCKING     = 1 << 1;
        const TOOL_CALL_EVENTS      = 1 << 2;
        const STRUCTURED_TRANSCRIPT = 1 << 3;
        const RAW_COST_PER_MESSAGE  = 1 << 4;
        const TASK_LIST             = 1 << 5;
        const SESSION_RESUME        = 1 << 6;
        const ANNOTATIONS_INJECT    = 1 << 7;
        /// The CLI exposes a built-in interactive picker for "choose a past
        /// session". CC/Pi/Codex do; OpenCode doesn't. Distinct from
        /// SESSION_RESUME, which only says the agent can resume by id.
        const SESSION_PICKER        = 1 << 8;
        /// The adapter can write cluihud's active theme palette into the
        /// agent's native theme subsystem (pi hot-reload, opencode custom
        /// theme, codex tui.theme). See `AgentAdapter::apply_theme`.
        const THEME_SYNC            = 1 << 9;
    }
}

/// Custom Serialize emits `Vec<&'static str>` so the wire shape is portable
/// to TS (`string[]`). `#[derive(Serialize)]` would emit raw bits which the
/// frontend can't easily map to capability names.
impl serde::Serialize for AgentCapability {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut names: Vec<&'static str> = Vec::new();
        if self.contains(Self::PLAN_REVIEW) {
            names.push("PLAN_REVIEW");
        }
        if self.contains(Self::ASK_USER_BLOCKING) {
            names.push("ASK_USER_BLOCKING");
        }
        if self.contains(Self::TOOL_CALL_EVENTS) {
            names.push("TOOL_CALL_EVENTS");
        }
        if self.contains(Self::STRUCTURED_TRANSCRIPT) {
            names.push("STRUCTURED_TRANSCRIPT");
        }
        if self.contains(Self::RAW_COST_PER_MESSAGE) {
            names.push("RAW_COST_PER_MESSAGE");
        }
        if self.contains(Self::TASK_LIST) {
            names.push("TASK_LIST");
        }
        if self.contains(Self::SESSION_RESUME) {
            names.push("SESSION_RESUME");
        }
        if self.contains(Self::ANNOTATIONS_INJECT) {
            names.push("ANNOTATIONS_INJECT");
        }
        if self.contains(Self::SESSION_PICKER) {
            names.push("SESSION_PICKER");
        }
        if self.contains(Self::THEME_SYNC) {
            names.push("THEME_SYNC");
        }
        names.serialize(ser)
    }
}

/// Mirror of [`AgentCapability`]'s Serialize; parses `Vec<String>` back into
/// a bitset so persisted/transported capability lists round-trip.
impl<'de> serde::Deserialize<'de> for AgentCapability {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let names: Vec<String> = serde::Deserialize::deserialize(de)?;
        let mut flags = AgentCapability::empty();
        for name in names {
            match name.as_str() {
                "PLAN_REVIEW" => flags |= Self::PLAN_REVIEW,
                "ASK_USER_BLOCKING" => flags |= Self::ASK_USER_BLOCKING,
                "TOOL_CALL_EVENTS" => flags |= Self::TOOL_CALL_EVENTS,
                "STRUCTURED_TRANSCRIPT" => flags |= Self::STRUCTURED_TRANSCRIPT,
                "RAW_COST_PER_MESSAGE" => flags |= Self::RAW_COST_PER_MESSAGE,
                "TASK_LIST" => flags |= Self::TASK_LIST,
                "SESSION_RESUME" => flags |= Self::SESSION_RESUME,
                "ANNOTATIONS_INJECT" => flags |= Self::ANNOTATIONS_INJECT,
                "SESSION_PICKER" => flags |= Self::SESSION_PICKER,
                "THEME_SYNC" => flags |= Self::THEME_SYNC,
                other => {
                    return Err(serde::de::Error::custom(format!(
                        "unknown agent capability: {other}"
                    )));
                }
            }
        }
        Ok(flags)
    }
}

/// Bundle of bitflags + supported model identifiers, returned by
/// [`AgentAdapter::capabilities`]. The model list is informational; gating is
/// done by `flags`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AgentCapabilities {
    pub flags: AgentCapability,
    pub supported_models: Vec<String>,
}

/// Cluihud theme snapshot crossed to each [`AgentAdapter::apply_theme`] call.
///
/// Derived in the frontend from `getComputedStyle` on `<html>` after
/// `applyTheme` commits, so the source of truth stays in CSS and adapters
/// don't need to know cluihud's theme catalog.
///
/// `border` may carry an rgba string (e.g. `rgba(255,255,255,0.08)`); each
/// adapter converts to its own native color shape.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePalette {
    pub id: String,
    pub is_dark: bool,
    pub surface: String,
    pub foreground: String,
    pub card: String,
    pub secondary: String,
    pub muted_foreground: String,
    pub border: String,
    pub accent: String,
}

/// Atomically write `bytes` to `path` via stage-and-rename so readers (pi's
/// theme hot-reloader, opencode's config probe) never see a half-written
/// file. Parent directory is created on demand.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

/// Reconcile a JSON settings file's top-level `theme` key.
///
/// - Missing file (or empty) → create with `{ "theme": expected }`.
/// - `theme` absent → set it, preserve other keys.
/// - `theme` already equals `expected` → no-op (`Ok(true)`).
/// - `theme` set to anything else → leave untouched, `Ok(false)` so the
///   caller can log "user selection wins".
/// - Parse failure → leave file alone (`Ok(false)`), since clobbering a
///   user's hand-edited config is worse than missing a theme sync cycle.
pub async fn update_settings_theme_if_safe(path: &Path, expected: &str) -> std::io::Result<bool> {
    let raw = match tokio::fs::read_to_string(path).await {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let mut value: serde_json::Value = match raw.as_deref().map(str::trim) {
        None | Some("") => serde_json::Value::Object(serde_json::Map::new()),
        Some(s) => match serde_json::from_str(s) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        },
    };
    let obj = match value.as_object_mut() {
        Some(o) => o,
        None => return Ok(false),
    };
    match obj.get("theme") {
        Some(serde_json::Value::String(s)) if s == expected => return Ok(true),
        Some(_) => return Ok(false),
        None => {
            obj.insert(
                "theme".into(),
                serde_json::Value::String(expected.to_string()),
            );
        }
    }
    let pretty =
        serde_json::to_vec_pretty(&value).map_err(|e| std::io::Error::other(e.to_string()))?;
    write_atomic(path, &pretty).await?;
    Ok(true)
}

/// Outcome of a lightweight [`AgentAdapter::detect`] call. Must NOT spawn
/// child processes; `version` is populated by [`AgentAdapter::refresh_version`]
/// in a background task post-startup.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DetectionResult {
    pub installed: bool,
    pub binary_path: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub version: Option<String>,
    /// Codex-specific trust gate state. `None` for adapters without a trust
    /// concept (CC, OpenCode, Pi).
    pub trusted_for_project: Option<bool>,
}

/// Context passed to [`AgentAdapter::spawn`] when starting a session.
pub struct SpawnContext<'a> {
    pub session_id: &'a str,
    pub cwd: &'a Path,
    /// Agent-internal session token for resume — Pi/Codex UUID, CC `--continue`
    /// marker, etc. The adapter interprets it.
    pub resume_from: Option<&'a str>,
    pub initial_prompt: Option<&'a str>,
}

/// Spawn descriptor returned by an adapter; the runtime hands it to the PTY
/// layer. `env` MUST include `CLUIHUD_SESSION_ID` so hooks can route events.
#[derive(Clone, Debug)]
pub struct SpawnSpec {
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// HTTP+SSE auth scheme. Marked `#[non_exhaustive]` so future variants
/// (`OAuth2 { ... }`, etc.) don't break consumers (Decision 11).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum AuthScheme {
    None,
    Bearer(String),
    Header { name: String, value: String },
}

/// Transport modeled as data. Each variant carries enough detail for the
/// runtime dispatcher to subscribe correctly without needing to introspect
/// the adapter.
#[derive(Clone, Debug)]
pub enum Transport {
    FileHooks {
        settings_path: PathBuf,
        hook_event_names: Vec<&'static str>,
    },
    HttpSse {
        /// URL template with `:port` placeholder, e.g. `http://127.0.0.1::port`.
        base_url_template: String,
        sse_path: &'static str,
        permission_endpoint: &'static str,
        auth: Option<AuthScheme>,
    },
    JsonlTail {
        sessions_dir: PathBuf,
    },
    RpcStdio {
        binary: String,
        args: Vec<String>,
    },
}

/// Per-line transcript event emitted by [`AgentAdapter::parse_transcript_line`].
/// Adapters with structured transcript (CC, Codex, Pi) emit specific variants;
/// adapters without (OpenCode TUI mode) return `None` from the parser.
#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    Message {
        role: String,
        content: String,
        model: Option<String>,
    },
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        output: serde_json::Value,
    },
    Cost(RawCost),
    Other(serde_json::Value),
}

/// Raw token counts extracted per message. The aggregator owns running totals
/// (see [`cost_aggregator::SessionCostAggregator`]); this struct is the wire
/// payload of a single line's cost.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RawCost {
    pub model_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// User decision on a plan (CC's ExitPlanMode). Adapters without
/// `PLAN_REVIEW` capability return `NotSupported` from
/// [`AgentAdapter::submit_plan_decision`].
#[derive(Clone, Debug)]
pub struct PlanDecision {
    pub approved: bool,
    /// On deny: surfaced back to the agent (e.g. embedded in a tool deny
    /// message). Ignored on approve.
    pub message: Option<String>,
}

/// Sink that adapters call to forward translated events into the cluihud
/// event bus. The runtime owns the receiver; adapters only emit.
pub type EventSink = tokio::sync::mpsc::UnboundedSender<crate::hooks::events::HookEvent>;

/// Adapter trait. Every supported agent CLI implements this.
///
/// Lifetime: an adapter is a long-lived `Arc<dyn AgentAdapter>` owned by the
/// [`registry::AgentRegistry`]. Per-session state (running event pumps,
/// watcher handles) is keyed by `session_id` inside the adapter's own
/// internal storage.
#[async_trait::async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> AgentId;
    fn display_name(&self) -> &str;
    fn capabilities(&self) -> &AgentCapabilities;
    fn transport(&self) -> Transport;

    /// Whether `cluihud setup` should write filesystem config for this
    /// adapter (e.g. `~/.claude/settings.json`). CC + Codex: true.
    /// OpenCode + Pi: false (no file-config hooks).
    fn requires_cluihud_setup(&self) -> bool;

    /// Lightweight detection. MUST NOT spawn child processes — only filesystem
    /// checks (config dir exists, binary on PATH, etc.). Use
    /// [`refresh_version`](Self::refresh_version) for the slower version probe.
    async fn detect(&self) -> DetectionResult;

    /// Optional async version refresh — runs in a background task post-startup
    /// and updates the agent metadata. Default returns `None` (no spawn).
    async fn refresh_version(&self) -> Option<String> {
        None
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError>;

    /// Per-line transcript parsing. SHALL emit [`TranscriptEvent::Cost`] when
    /// the line carries usage; the runtime's [`SessionCostAggregator`] owns the
    /// running totals (raw_cost is per-line, totals are per-session).
    /// Stays sync — this is a hot path called per JSONL line.
    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent>;

    /// Boots adapter-specific I/O for a session: CC starts transcript + plan
    /// watchers (Decision 12); OpenCode starts an SSE subscription; Pi starts
    /// a JSONL tail. The shared Unix socket for hook events runs outside the
    /// adapter (it belongs to the runtime).
    async fn start_event_pump(&self, session_id: &str, sink: EventSink)
    -> Result<(), AdapterError>;

    /// Stops any background tasks started by `start_event_pump`. Idempotent.
    /// Default impl is a no-op for adapters that don't start anything.
    async fn stop_event_pump(&self, _session_id: &str) -> Result<(), AdapterError> {
        Ok(())
    }

    /// For adapters that declare [`AgentCapability::PLAN_REVIEW`]. Default
    /// returns `NotSupported`.
    async fn submit_plan_decision(
        &self,
        _session_id: &str,
        _decision: PlanDecision,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::NotSupported(AgentCapability::PLAN_REVIEW))
    }

    /// Agent-internal session id harvested by the adapter (e.g. Pi's session
    /// UUID extracted from the JSONL header). Returned for the runtime to
    /// persist on the session row so resume can pass it back via
    /// `--session <id>` after a cluihud restart. Default returns `None` for
    /// adapters that don't surface a separate id.
    fn agent_internal_session_id(&self, _cluihud_session_id: &str) -> Option<String> {
        None
    }

    /// For adapters that declare [`AgentCapability::ASK_USER_BLOCKING`].
    /// Default returns `NotSupported`.
    async fn submit_ask_answer(
        &self,
        _session_id: &str,
        _answers: serde_json::Value,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::NotSupported(
            AgentCapability::ASK_USER_BLOCKING,
        ))
    }

    /// Apply cluihud's active theme to the agent's native theming subsystem.
    ///
    /// Best-effort by contract: implementations write namespaced files
    /// (`cluihud-active.*`) and only flip the agent's pointer when it is
    /// absent or already ours, so a user-authored theme never gets clobbered.
    /// Returned errors are logged at the registry dispatcher and never
    /// surfaced as user-facing notifications.
    ///
    /// Adapters that declare [`AgentCapability::THEME_SYNC`] MUST override
    /// this; the default returns `NotSupported`.
    async fn apply_theme(&self, _palette: &ThemePalette) -> Result<(), AdapterError> {
        Err(AdapterError::NotSupported(AgentCapability::THEME_SYNC))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_accepts_valid_inputs() {
        for s in ["a", "claude-code", "opencode", "pi", "codex", "x1-2-3"] {
            assert!(AgentId::new(s).is_ok(), "{s}");
        }
    }

    #[test]
    fn agent_id_rejects_invalid_inputs() {
        for s in [
            "",
            "1abc",    // starts with digit
            "Abc",     // uppercase
            "abc_def", // underscore
            "abc.def", // dot
            "abc/def", // slash
            "../../etc/passwd",
            &"a".repeat(33), // length 33 > 32
        ] {
            assert!(AgentId::new(s).is_err(), "expected error for {s:?}");
        }
    }

    #[test]
    fn known_agent_ids_round_trip_through_validator() {
        // Protect against a future rename of the known constructors that
        // would silently produce a string the regex no longer accepts.
        for id in [
            AgentId::claude_code(),
            AgentId::opencode(),
            AgentId::pi(),
            AgentId::codex(),
        ] {
            assert!(
                AgentId::new(id.as_str()).is_ok(),
                "{} fails the AgentId regex",
                id.as_str()
            );
        }
    }

    #[test]
    fn capability_serializes_as_string_list() {
        let caps = AgentCapability::PLAN_REVIEW | AgentCapability::TOOL_CALL_EVENTS;
        let json = serde_json::to_string(&caps).unwrap();
        assert_eq!(json, r#"["PLAN_REVIEW","TOOL_CALL_EVENTS"]"#);
    }

    #[test]
    fn capability_round_trips_via_string_list() {
        let original = AgentCapability::PLAN_REVIEW
            | AgentCapability::ASK_USER_BLOCKING
            | AgentCapability::SESSION_RESUME;
        let json = serde_json::to_string(&original).unwrap();
        let parsed: AgentCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn capability_deserialize_rejects_unknown_name() {
        let json = r#"["PLAN_REVIEW","NOT_A_REAL_CAP"]"#;
        let parsed: Result<AgentCapability, _> = serde_json::from_str(json);
        assert!(parsed.is_err());
    }

    #[test]
    fn theme_sync_capability_round_trips() {
        let caps = AgentCapability::THEME_SYNC | AgentCapability::TOOL_CALL_EVENTS;
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("THEME_SYNC"));
        let parsed: AgentCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, caps);
    }

    #[test]
    fn theme_palette_serializes_camel_case() {
        let p = ThemePalette {
            id: "v1-dark".into(),
            is_dark: true,
            surface: "#000".into(),
            foreground: "#fff".into(),
            card: "#0a0a0b".into(),
            secondary: "#1c1c1e".into(),
            muted_foreground: "#5c5c5f".into(),
            border: "rgba(255,255,255,0.08)".into(),
            accent: "#f97316".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"isDark\":true"));
        assert!(json.contains("\"mutedForeground\":"));
        let parsed: ThemePalette = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "v1-dark");
        assert_eq!(parsed.muted_foreground, "#5c5c5f");
        assert!(parsed.is_dark);
    }

    #[tokio::test]
    async fn write_atomic_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested/sub/out.json");
        write_atomic(&target, b"{\"k\":1}").await.unwrap();
        let content = tokio::fs::read_to_string(&target).await.unwrap();
        assert_eq!(content, "{\"k\":1}");
    }

    #[tokio::test]
    async fn update_settings_theme_creates_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let touched = update_settings_theme_if_safe(&path, "cluihud-active")
            .await
            .unwrap();
        assert!(touched);
        let v: serde_json::Value =
            serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(v["theme"], "cluihud-active");
    }

    #[tokio::test]
    async fn update_settings_theme_preserves_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        tokio::fs::write(&path, br#"{"defaultModel":"sonnet","other":42}"#)
            .await
            .unwrap();
        let touched = update_settings_theme_if_safe(&path, "cluihud-active")
            .await
            .unwrap();
        assert!(touched);
        let v: serde_json::Value =
            serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(v["theme"], "cluihud-active");
        assert_eq!(v["defaultModel"], "sonnet");
        assert_eq!(v["other"], 42);
    }

    #[tokio::test]
    async fn update_settings_theme_respects_user_choice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        tokio::fs::write(&path, br#"{"theme":"tokyonight"}"#)
            .await
            .unwrap();
        let touched = update_settings_theme_if_safe(&path, "cluihud-active")
            .await
            .unwrap();
        assert!(!touched);
        let v: serde_json::Value =
            serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(v["theme"], "tokyonight");
    }

    #[tokio::test]
    async fn update_settings_theme_idempotent_when_already_ours() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        tokio::fs::write(&path, br#"{"theme":"cluihud-active","x":1}"#)
            .await
            .unwrap();
        let touched = update_settings_theme_if_safe(&path, "cluihud-active")
            .await
            .unwrap();
        assert!(touched);
        let raw = tokio::fs::read_to_string(&path).await.unwrap();
        // No-op path: file is left untouched (we asserted equality, didn't
        // rewrite), so the original compact form survives.
        assert_eq!(raw, r#"{"theme":"cluihud-active","x":1}"#);
    }

    #[tokio::test]
    async fn update_settings_theme_skips_corrupted_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        tokio::fs::write(&path, b"not valid json").await.unwrap();
        let touched = update_settings_theme_if_safe(&path, "cluihud-active")
            .await
            .unwrap();
        assert!(!touched);
        let raw = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(raw, "not valid json");
    }
}
