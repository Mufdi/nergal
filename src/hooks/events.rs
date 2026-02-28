use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
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
    },
    #[serde(rename = "TaskCompleted")]
    TaskCompleted { session_id: String },
    #[serde(rename = "UserPromptSubmit")]
    UserPromptSubmit { session_id: String },
}

impl HookEvent {
    pub fn session_id(&self) -> &str {
        match self {
            Self::SessionStart { session_id }
            | Self::SessionEnd { session_id }
            | Self::PreToolUse { session_id, .. }
            | Self::PostToolUse { session_id, .. }
            | Self::Stop { session_id, .. }
            | Self::TaskCompleted { session_id }
            | Self::UserPromptSubmit { session_id } => session_id,
        }
    }
}
