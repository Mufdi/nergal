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
}
