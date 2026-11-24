#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub const BACKGROUND_INPUT_WAITING_PREFIX: &str = "background agent";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptCacheStatus {
    #[serde(default)]
    pub warm: Option<bool>,
    #[serde(default)]
    pub caching_observed: Option<bool>,
    #[serde(default)]
    pub ttl: Option<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub requests: Option<u64>,
    #[serde(default)]
    pub misses: Option<u64>,
    #[serde(default)]
    pub hit_ratio: Option<f64>,
    #[serde(default)]
    pub miss_recache_tokens: Option<u64>,
    #[serde(default)]
    pub last_miss_at: Option<u64>,
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub last_miss_causes: Vec<String>,
}

/// One explicit `null` would otherwise fail the whole `AgentStatus` and drop
/// the status update silently.
fn null_as_empty_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

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
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        agent_type: Option<String>,
    },
    #[serde(rename = "PostToolUse")]
    PostToolUse {
        session_id: String,
        tool_name: String,
        #[serde(default)]
        tool_input: serde_json::Value,
        #[serde(default)]
        tool_result: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        agent_type: Option<String>,
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
        #[serde(default)]
        agent_id: Option<String>,
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
    #[serde(rename = "SubagentStart")]
    SubagentStart {
        session_id: String,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        agent_type: Option<String>,
    },
    #[serde(rename = "SubagentStop")]
    SubagentStop {
        session_id: String,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        agent_type: Option<String>,
        #[serde(default)]
        last_assistant_message: Option<String>,
        #[serde(default)]
        effort: serde_json::Value,
        /// The subagent's own transcript; the hook payload reports an empty message
        /// when the agent was stopped before it replied.
        #[serde(default)]
        agent_transcript_path: Option<String>,
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
        #[serde(default)]
        prompt_cache: Option<PromptCacheStatus>,
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
            Self::Notification {
                notification_type, ..
            } if notification_type.as_deref() == Some("agent_completed") => None,
            Self::PlanReview { .. }
            | Self::AskUser { .. }
            | Self::Notification { .. }
            | Self::PermissionDenied { .. } => Some("needs_attention"),
            Self::Stop { .. } | Self::SessionEnd { .. } => Some("idle"),
            Self::CwdChanged { .. }
            | Self::FileChanged { .. }
            | Self::SubagentStart { .. }
            | Self::SubagentStop { .. }
            | Self::StatusLine { .. }
            | Self::AgentStatus { .. } => None,
        }
    }

    pub fn effort_level(&self) -> Option<String> {
        match self {
            Self::SubagentStop { effort, .. } => effort
                .get("level")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            _ => None,
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
            } => match notification_type.as_deref() {
                Some("agent_completed") => None,
                Some("agent_needs_input") => Some(match message {
                    Some(m) => format!("{BACKGROUND_INPUT_WAITING_PREFIX}: {m}"),
                    None => BACKGROUND_INPUT_WAITING_PREFIX.to_string(),
                }),
                _ => message
                    .clone()
                    .or_else(|| notification_type.clone())
                    .or_else(|| Some("notification".to_string())),
            },
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
            | Self::SubagentStart { session_id, .. }
            | Self::SubagentStop { session_id, .. }
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
            agent_id: None,
            agent_type: None,
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
            agent_id: None,
        };
        assert_eq!(plan.waiting_for().as_deref(), Some("plan review"));

        let running = HookEvent::PreToolUse {
            session_id: "s".into(),
            tool_name: "Read".into(),
            tool_input: serde_json::Value::Null,
            agent_id: None,
            agent_type: None,
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

    #[test]
    fn subagent_start_deserializes_with_all_fields() {
        let json = r#"{"hook_event_name":"SubagentStart","session_id":"s1","agent_id":"a1","agent_type":"Explore"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::SubagentStart {
                session_id,
                agent_id,
                agent_type,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(agent_id.as_deref(), Some("a1"));
                assert_eq!(agent_type.as_deref(), Some("Explore"));
            }
            _ => panic!("expected SubagentStart"),
        }
    }

    #[test]
    fn subagent_start_without_optional_fields_still_deserializes() {
        let json = r#"{"hook_event_name":"SubagentStart","session_id":"s1"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::SubagentStart {
                agent_id,
                agent_type,
                ..
            } => {
                assert!(agent_id.is_none());
                assert!(agent_type.is_none());
            }
            _ => panic!("expected SubagentStart"),
        }
    }

    #[test]
    fn subagent_stop_deserializes_with_effort_and_last_message() {
        let json = r#"{"hook_event_name":"SubagentStop","session_id":"s1","agent_id":"a1","agent_type":"Plan","last_assistant_message":"plan ready","effort":{"level":"high"}}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev.effort_level().as_deref(), Some("high"));
        match ev {
            HookEvent::SubagentStop {
                agent_id,
                agent_type,
                last_assistant_message,
                ..
            } => {
                assert_eq!(agent_id.as_deref(), Some("a1"));
                assert_eq!(agent_type.as_deref(), Some("Plan"));
                assert_eq!(last_assistant_message.as_deref(), Some("plan ready"));
            }
            _ => panic!("expected SubagentStop"),
        }
    }

    #[test]
    fn subagent_stop_captures_agent_transcript_path() {
        let json = r#"{"hook_event_name":"SubagentStop","session_id":"s1","agent_transcript_path":"/t/agent-a1.jsonl"}"#;
        match serde_json::from_str::<HookEvent>(json).unwrap() {
            HookEvent::SubagentStop {
                agent_transcript_path,
                ..
            } => assert_eq!(agent_transcript_path.as_deref(), Some("/t/agent-a1.jsonl")),
            _ => panic!("expected SubagentStop"),
        }
    }

    #[test]
    fn subagent_stop_without_optional_fields_still_deserializes() {
        let json = r#"{"hook_event_name":"SubagentStop","session_id":"s1"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert!(ev.effort_level().is_none());
        match ev {
            HookEvent::SubagentStop {
                agent_id,
                agent_type,
                last_assistant_message,
                effort,
                ..
            } => {
                assert!(agent_id.is_none());
                assert!(agent_type.is_none());
                assert!(last_assistant_message.is_none());
                assert!(effort.is_null());
            }
            _ => panic!("expected SubagentStop"),
        }
    }

    #[test]
    fn effort_level_is_none_for_non_string_level_and_other_events() {
        let json = r#"{"hook_event_name":"SubagentStop","session_id":"s1","effort":{"level":3}}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert!(ev.effort_level().is_none());

        let stop = HookEvent::SessionEnd {
            session_id: "s".into(),
        };
        assert!(stop.effort_level().is_none());
    }

    #[test]
    fn subagent_session_id_is_exposed() {
        let start = HookEvent::SubagentStart {
            session_id: "s-start".into(),
            agent_id: None,
            agent_type: None,
        };
        assert_eq!(start.session_id(), "s-start");
        let stop = HookEvent::SubagentStop {
            session_id: "s-stop".into(),
            agent_id: None,
            agent_type: None,
            last_assistant_message: None,
            effort: serde_json::Value::Null,
            agent_transcript_path: None,
        };
        assert_eq!(stop.session_id(), "s-stop");
    }

    #[test]
    fn subagent_events_carry_no_mcp_mode_or_waiting_reason() {
        let start = HookEvent::SubagentStart {
            session_id: "s".into(),
            agent_id: Some("a1".into()),
            agent_type: Some("Explore".into()),
        };
        let stop = HookEvent::SubagentStop {
            session_id: "s".into(),
            agent_id: Some("a1".into()),
            agent_type: Some("Explore".into()),
            last_assistant_message: Some("done".into()),
            effort: serde_json::Value::Null,
            agent_transcript_path: None,
        };
        for ev in [start, stop] {
            assert_eq!(ev.mcp_mode(), None);
            assert!(ev.waiting_for().is_none());
        }
    }

    #[test]
    fn notification_mcp_mode_covers_attention_types_and_background_completion() {
        let notif = |t: Option<&str>| HookEvent::Notification {
            session_id: "s".into(),
            notification_type: t.map(str::to_string),
            message: None,
        };
        assert_eq!(
            notif(Some("agent_needs_input")).mcp_mode(),
            Some("needs_attention")
        );
        assert_eq!(
            notif(Some("permission_prompt")).mcp_mode(),
            Some("needs_attention")
        );
        assert_eq!(notif(Some("agent_completed")).mcp_mode(), None);
    }

    #[test]
    fn waiting_for_prefixes_background_agent_input_requests() {
        assert_eq!(BACKGROUND_INPUT_WAITING_PREFIX, "background agent");

        let with_message = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: Some("agent_needs_input".into()),
            message: Some("Pick a branch".into()),
        };
        assert_eq!(
            with_message.waiting_for().as_deref(),
            Some("background agent: Pick a branch")
        );

        let without_message = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: Some("agent_needs_input".into()),
            message: None,
        };
        assert_eq!(
            without_message.waiting_for().as_deref(),
            Some("background agent")
        );
    }

    #[test]
    fn waiting_for_is_none_for_agent_completed_and_unchanged_for_other_types() {
        let completed = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: Some("agent_completed".into()),
            message: Some("finished".into()),
        };
        assert!(completed.waiting_for().is_none());

        let by_type = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: Some("idle_prompt".into()),
            message: None,
        };
        assert_eq!(by_type.waiting_for().as_deref(), Some("idle_prompt"));

        let bare = HookEvent::Notification {
            session_id: "s".into(),
            notification_type: None,
            message: None,
        };
        assert_eq!(bare.waiting_for().as_deref(), Some("notification"));
    }

    #[test]
    fn agent_status_deserializes_full_prompt_cache() {
        let json = r#"{"hook_event_name":"AgentStatus","session_id":"s1","prompt_cache":{"warm":true,"caching_observed":true,"ttl":"1h","expires_at":1738429200,"requests":14,"misses":2,"hit_ratio":0.91,"miss_recache_tokens":310200,"last_miss_at":1738425230,"last_miss_causes":["tools_changed"]}}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::AgentStatus { prompt_cache, .. } => {
                assert_eq!(
                    prompt_cache,
                    Some(PromptCacheStatus {
                        warm: Some(true),
                        caching_observed: Some(true),
                        ttl: Some("1h".into()),
                        expires_at: Some(1738429200),
                        requests: Some(14),
                        misses: Some(2),
                        hit_ratio: Some(0.91),
                        miss_recache_tokens: Some(310200),
                        last_miss_at: Some(1738425230),
                        last_miss_causes: vec!["tools_changed".into()],
                    })
                );
            }
            _ => panic!("expected AgentStatus"),
        }
    }

    #[test]
    fn agent_status_without_prompt_cache_deserializes_to_none() {
        let json = r#"{"hook_event_name":"AgentStatus","session_id":"s1","model_id":"m"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::AgentStatus { prompt_cache, .. } => assert!(prompt_cache.is_none()),
            _ => panic!("expected AgentStatus"),
        }
    }

    #[test]
    fn agent_status_accepts_null_prompt_cache() {
        let json = r#"{"hook_event_name":"AgentStatus","session_id":"s1","prompt_cache":null}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::AgentStatus { prompt_cache, .. } => assert!(prompt_cache.is_none()),
            _ => panic!("expected AgentStatus"),
        }
    }

    #[test]
    fn prompt_cache_with_null_ratio_and_causes_deserializes_to_defaults() {
        let json = r#"{"hook_event_name":"AgentStatus","session_id":"s1","prompt_cache":{"warm":false,"hit_ratio":null}}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::AgentStatus { prompt_cache, .. } => {
                let pc = prompt_cache.unwrap();
                assert_eq!(pc.warm, Some(false));
                assert!(pc.hit_ratio.is_none());
                assert!(pc.last_miss_causes.is_empty());
            }
            _ => panic!("expected AgentStatus"),
        }
    }

    #[test]
    fn prompt_cache_tolerates_explicit_null_causes() {
        let json = r#"{"hook_event_name":"AgentStatus","session_id":"s","prompt_cache":{"warm":true,"last_miss_causes":null}}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        match ev {
            HookEvent::AgentStatus { prompt_cache, .. } => {
                let pc = prompt_cache.unwrap();
                assert_eq!(pc.warm, Some(true));
                assert!(pc.last_miss_causes.is_empty());
            }
            _ => panic!("expected AgentStatus"),
        }
    }
}
