#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hook_event_name")]
pub enum HookEvent {
    #[serde(rename = "SessionStart")]
    SessionStart { session_id: String },
    #[serde(rename = "SessionEnd")]
    SessionEnd { session_id: String },
    #[serde(rename = "PreToolUse")]
    PreToolUse {
        session_id: String,
        tool_name: String,
        #[serde(default)]
        tool_input: serde_json::Value,
    },
    #[serde(rename = "PostToolUse")]
    PostToolUse {
        session_id: String,
        tool_name: String,
        #[serde(default)]
        tool_input: serde_json::Value,
        #[serde(default)]
        tool_result: Option<String>,
    },
    #[serde(rename = "Stop")]
    Stop {
        session_id: String,
        #[serde(default)]
        stop_reason: Option<String>,
        #[serde(default)]
        transcript_path: Option<String>,
        /// Background tasks still running at Stop (CC v2.1.150+). Pass-through
        /// JSON, tolerant of CC shape drift; surfaced in the MCP descriptor.
        /// `#[serde(default)]` keeps legacy payloads (without the field) valid.
        #[serde(default)]
        background_tasks: Vec<serde_json::Value>,
        /// Scheduled crons for the session (CC v2.1.150+). Same contract.
        #[serde(default)]
        session_crons: Vec<serde_json::Value>,
        /// Text of the last assistant message before stopping (CC Stop
        /// payload). A free, no-API session summary surfaced in the MCP
        /// descriptor. `#[serde(default)]` keeps legacy payloads valid.
        #[serde(default)]
        last_assistant_message: Option<String>,
    },
    #[serde(rename = "TaskCompleted")]
    TaskCompleted {
        session_id: String,
        #[serde(default)]
        task_id: Option<String>,
        #[serde(default)]
        task_subject: Option<String>,
    },
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit { session_id: String },
    #[serde(rename = "TaskCreated")]
    TaskCreated {
        session_id: String,
        #[serde(default)]
        task_id: Option<String>,
        #[serde(default)]
        task_subject: Option<String>,
        #[serde(default)]
        tool_input: serde_json::Value,
    },
    #[serde(rename = "PlanReview")]
    PlanReview {
        session_id: String,
        tool_name: String,
        #[serde(default)]
        tool_input: serde_json::Value,
        fifo_path: String,
    },
    #[serde(rename = "AskUser")]
    AskUser {
        session_id: String,
        #[serde(default)]
        tool_input: serde_json::Value,
        #[serde(default)]
        fifo_path: String,
    },
    /// Consumed only to light the session attention indicator —
    /// `notification_type` discriminates the subtype the matcher filtered on
    /// (e.g. `permission_prompt`).
    #[serde(rename = "Notification")]
    Notification {
        session_id: String,
        #[serde(default)]
        notification_type: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(rename = "CwdChanged")]
    CwdChanged {
        session_id: String,
        #[serde(default)]
        cwd: Option<String>,
    },
    #[serde(rename = "FileChanged")]
    FileChanged {
        session_id: String,
        #[serde(default)]
        file_path: Option<String>,
        #[serde(default)]
        event_type: Option<String>,
    },
    #[serde(rename = "PermissionDenied")]
    PermissionDenied {
        session_id: String,
        #[serde(default)]
        tool_name: Option<String>,
        #[serde(default)]
        tool_input: serde_json::Value,
        #[serde(default)]
        reason: Option<String>,
    },
    #[serde(rename = "StatusLine")]
    StatusLine {
        session_id: String,
        #[serde(default)]
        model_id: Option<String>,
        #[serde(default)]
        model_name: Option<String>,
        #[serde(default)]
        context_used_pct: Option<f64>,
        #[serde(default)]
        context_remaining_pct: Option<f64>,
        #[serde(default)]
        context_window_size: Option<u64>,
        #[serde(default)]
        rate_5h_pct: Option<f64>,
        #[serde(default)]
        rate_5h_resets_at: Option<u64>,
        #[serde(default)]
        rate_7d_pct: Option<f64>,
        #[serde(default)]
        rate_7d_resets_at: Option<u64>,
        #[serde(default)]
        duration_ms: Option<u64>,
        #[serde(default)]
        api_duration_ms: Option<u64>,
        #[serde(default)]
        lines_added: Option<u64>,
        #[serde(default)]
        lines_removed: Option<u64>,
    },
    /// Agent-agnostic status snapshot. Any adapter can emit it; the
    /// dispatcher forwards the payload to the frontend as
    /// `agent:status-update`. Fields are all optional so each adapter only
    /// populates what it knows: CC populates the full set, Pi/Codex populate
    /// `model_*`, OpenCode populates whatever its SSE surfaces.
    #[serde(rename = "AgentStatus")]
    AgentStatus {
        session_id: String,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        model_id: Option<String>,
        #[serde(default)]
        model_name: Option<String>,
        #[serde(default)]
        session_started_at: Option<u64>,
        #[serde(default)]
        context_used_pct: Option<f64>,
        #[serde(default)]
        context_window_size: Option<u64>,
        #[serde(default)]
        rate_5h_pct: Option<f64>,
        #[serde(default)]
        rate_5h_resets_at: Option<u64>,
        #[serde(default)]
        rate_7d_pct: Option<f64>,
        #[serde(default)]
        rate_7d_resets_at: Option<u64>,
        #[serde(default)]
        effort_level: Option<String>,
    },
}

impl HookEvent {
    /// The live MCP session mode this event implies, or `None` for events that
    /// carry no liveness signal (timer-driven telemetry, external file-watcher
    /// events). Mirrors the frontend `modeMapAtom` derivation
    /// (`src/stores/hooks.ts`) so the MCP descriptor and the in-app indicator
    /// agree on whether a session is working, idle, or waiting on the human.
    pub fn mcp_mode(&self) -> Option<&'static str> {
        match self {
            Self::SessionStart { .. }
            | Self::UserPromptSubmit { .. }
            | Self::PreToolUse { .. }
            | Self::PostToolUse { .. }
            | Self::TaskCreated { .. }
            | Self::TaskCompleted { .. } => Some("running"),
            Self::PlanReview { .. }
            | Self::AskUser { .. }
            | Self::Notification { .. }
            | Self::PermissionDenied { .. } => Some("needs_attention"),
            Self::Stop { .. } | Self::SessionEnd { .. } => Some("idle"),
            Self::CwdChanged { .. }
            | Self::FileChanged { .. }
            | Self::StatusLine { .. }
            | Self::AgentStatus { .. } => None,
        }
    }

    /// Human-readable reason this event puts the session into
    /// `needs_attention`, surfaced as the descriptor's `waiting_for`. `None`
    /// for events that don't block on the human (the descriptor then clears any
    /// stale reason).
    pub fn waiting_for(&self) -> Option<String> {
        match self {
            Self::Notification {
                notification_type,
                message,
                ..
            } => message
                .clone()
                .or_else(|| notification_type.clone())
                .or_else(|| Some("notification".to_string())),
            Self::AskUser { .. } => Some("answering a question".to_string()),
            Self::PlanReview { .. } => Some("plan review".to_string()),
            Self::PermissionDenied {
                tool_name, reason, ..
            } => Some(match (tool_name, reason) {
                (Some(t), _) => format!("permission for {t}"),
                (None, Some(r)) => format!("permission denied: {r}"),
                _ => "permission prompt".to_string(),
            }),
            _ => None,
        }
    }

    pub fn session_id(&self) -> &str {
        match self {
            Self::SessionStart { session_id }
            | Self::SessionEnd { session_id }
            | Self::PreToolUse { session_id, .. }
            | Self::PostToolUse { session_id, .. }
            | Self::Stop { session_id, .. }
            | Self::TaskCompleted { session_id, .. }
            | Self::TaskCreated { session_id, .. }
            | Self::UserPromptSubmit { session_id }
            | Self::PlanReview { session_id, .. }
            | Self::AskUser { session_id, .. }
            | Self::Notification { session_id, .. }
            | Self::CwdChanged { session_id, .. }
            | Self::FileChanged { session_id, .. }
            | Self::PermissionDenied { session_id, .. }
            | Self::StatusLine { session_id, .. }
            | Self::AgentStatus { session_id, .. } => session_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_stop_without_bg_fields_still_deserializes() {
        let json = r#"{"hook_event_name":"Stop","session_id":"s1","stop_reason":"end_turn"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::Stop {
                session_id,
                background_tasks,
                session_crons,
                ..
            } => {
                assert_eq!(session_id, "s1");
                assert!(background_tasks.is_empty());
                assert!(session_crons.is_empty());
            }
            _ => panic!("expected Stop"),
        }
    }

    #[test]
    fn mcp_mode_maps_events_to_canonical_modes() {
        let running = HookEvent::PreToolUse {
            session_id: "s".into(),
            tool_name: "Read".into(),
            tool_input: serde_json::Value::Null,
        };
        assert_eq!(running.mcp_mode(), Some("running"));

        let attention = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: None,
            message: None,
        };
        assert_eq!(attention.mcp_mode(), Some("needs_attention"));

        let idle = HookEvent::Stop {
            session_id: "s".into(),
            stop_reason: None,
            transcript_path: None,
            background_tasks: vec![],
            session_crons: vec![],
            last_assistant_message: None,
        };
        assert_eq!(idle.mcp_mode(), Some("idle"));

        let telemetry = HookEvent::CwdChanged {
            session_id: "s".into(),
            cwd: None,
        };
        assert_eq!(telemetry.mcp_mode(), None);
    }

    #[test]
    fn waiting_for_describes_attention_events_only() {
        let notif = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: Some("permission_prompt".into()),
            message: Some("Allow Bash?".into()),
        };
        assert_eq!(notif.waiting_for().as_deref(), Some("Allow Bash?"));

        let plan = HookEvent::PlanReview {
            session_id: "s".into(),
            tool_name: "ExitPlanMode".into(),
            tool_input: serde_json::Value::Null,
            fifo_path: "/tmp/x".into(),
        };
        assert_eq!(plan.waiting_for().as_deref(), Some("plan review"));

        let running = HookEvent::PreToolUse {
            session_id: "s".into(),
            tool_name: "Read".into(),
            tool_input: serde_json::Value::Null,
        };
        assert!(running.waiting_for().is_none());
    }

    #[test]
    fn stop_captures_background_tasks_and_crons() {
        let json = r#"{"hook_event_name":"Stop","session_id":"s1","background_tasks":[{"id":"bg1"}],"session_crons":[{"id":"c1"}]}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::Stop {
                background_tasks,
                session_crons,
                ..
            } => {
                assert_eq!(background_tasks.len(), 1);
                assert_eq!(session_crons.len(), 1);
                assert_eq!(background_tasks[0]["id"], "bg1");
            }
            _ => panic!("expected Stop"),
        }
    }

    #[test]
    fn stop_captures_last_assistant_message() {
        let json = r#"{"hook_event_name":"Stop","session_id":"s1","last_assistant_message":"done refactoring"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::Stop {
                last_assistant_message,
                ..
            } => assert_eq!(last_assistant_message.as_deref(), Some("done refactoring")),
            _ => panic!("expected Stop"),
        }
    }
}
