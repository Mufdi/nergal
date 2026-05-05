//! Pi JSONL transcript parser.
//!
//! Real Pi shape (verified against `~/.pi/agent/sessions/.../*.jsonl`):
//! ```json
//! {"type":"session","id":"<uuid>","cwd":"...","timestamp":"..."}
//! {"type":"message","message":{"role":"assistant","content":[
//!   {"type":"thinking","thinking":"..."},
//!   {"type":"text","text":"..."},
//!   {"type":"toolCall","id":"...","name":"edit","arguments":{"path":"...","edits":[...]}}
//! ],"usage":{"input":...,"output":...,"cacheRead":...,"cacheWrite":...}}}
//! {"type":"message","message":{"role":"toolResult","toolCallId":"...","toolName":"edit","content":[...]}}
//! {"type":"message","message":{"role":"user","content":[...]}}
//! ```
//!
//! Each line wraps `{"type":"message","message":{...}}` with the role inside.
//! Tool calls live as items in `message.content[]` with `type:"toolCall"`
//! (camelCase). Tool results use `role:"toolResult"` with top-level `toolCallId`.
//!
//! Parser priority when an assistant message carries both a tool call and
//! usage: emit the first `toolCall` as `ToolUse` (drives the diff panel).
//! Cost is dropped silently in this case — the foundation `wrap()` discards
//! `Cost` events anyway, so no information is lost downstream. When usage
//! arrives without a tool call, emit `Cost`.

use serde::Deserialize;

use crate::agents::{RawCost, TranscriptEvent};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum PiLine {
    #[serde(rename = "session")]
    Session,
    #[serde(rename = "message")]
    Message {
        message: PiMessage,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct PiMessage {
    role: String,
    #[serde(default)]
    content: serde_json::Value,
    #[serde(default)]
    usage: Option<PiUsage>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "toolCallId", default)]
    tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PiUsage {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(rename = "cacheRead", default)]
    cache_read: u64,
    #[serde(rename = "cacheWrite", default)]
    cache_write: u64,
}

/// Parse a single Pi JSONL line. Returns `None` for headers, malformed JSON,
/// user messages, and unknown shapes.
pub fn parse_transcript_line(line: &str) -> Option<TranscriptEvent> {
    let parsed: PiLine = serde_json::from_str(line).ok()?;
    let PiLine::Message { message } = parsed else {
        return None;
    };

    match message.role.as_str() {
        "assistant" => parse_assistant(message),
        "toolResult" => parse_tool_result(message),
        // user messages and any unknown role surface nothing.
        _ => None,
    }
}

fn parse_assistant(message: PiMessage) -> Option<TranscriptEvent> {
    if let Some(tool) = first_tool_call(&message.content) {
        return Some(tool);
    }
    if let Some(u) = message.usage {
        return Some(TranscriptEvent::Cost(RawCost {
            model_id: message.model,
            input_tokens: u.input,
            output_tokens: u.output,
            cache_read_tokens: u.cache_read,
            cache_write_tokens: u.cache_write,
        }));
    }
    Some(TranscriptEvent::Message {
        role: message.role,
        content: message.content.to_string(),
        model: message.model,
    })
}

fn parse_tool_result(message: PiMessage) -> Option<TranscriptEvent> {
    let tool_use_id = message.tool_call_id?;
    Some(TranscriptEvent::ToolResult {
        tool_use_id,
        output: message.content,
    })
}

/// Walk `message.content[]` and return the first `toolCall` part as a
/// `ToolUse`. Returns `None` when content isn't an array, is empty, or has
/// no tool-call parts.
fn first_tool_call(content: &serde_json::Value) -> Option<TranscriptEvent> {
    let parts = content.as_array()?;
    for part in parts {
        if part.get("type").and_then(|v| v.as_str()) == Some("toolCall") {
            let name = part.get("name").and_then(|v| v.as_str())?.to_string();
            let input = part
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            return Some(TranscriptEvent::ToolUse { name, input });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_assistant_with_tool_call_emits_tool_use_with_path() {
        let line = r#"{"type":"message","message":{"role":"assistant","content":[
            {"type":"text","text":"editing..."},
            {"type":"toolCall","id":"call_1","name":"edit","arguments":{"path":"./foo.js","edits":[{"oldText":"a","newText":"b"}]}}
        ],"usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0}}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::ToolUse { name, input } => {
                assert_eq!(name, "edit");
                assert_eq!(input.get("path").and_then(|v| v.as_str()), Some("./foo.js"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_without_tool_call_but_with_usage_emits_cost() {
        let line = r#"{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"hi"}],"usage":{"input":10,"output":5,"cacheRead":2,"cacheWrite":1},"model":"glm-4.6"}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Cost(raw) => {
                assert_eq!(raw.input_tokens, 10);
                assert_eq!(raw.output_tokens, 5);
                assert_eq!(raw.cache_read_tokens, 2);
                assert_eq!(raw.cache_write_tokens, 1);
                assert_eq!(raw.model_id.as_deref(), Some("glm-4.6"));
            }
            other => panic!("expected Cost, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_plain_message_emits_message() {
        let line = r#"{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Message { role, .. } => assert_eq!(role, "assistant"),
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_result_emits_tool_result() {
        let line = r#"{"type":"message","message":{"role":"toolResult","toolCallId":"call_1","toolName":"read","content":[{"type":"text","text":"output"}]}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::ToolResult {
                tool_use_id,
                output,
            } => {
                assert_eq!(tool_use_id, "call_1");
                assert!(output.is_array());
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_session_header_returns_none() {
        let line = r#"{"type":"session","id":"abc-uuid","version":3,"cwd":"/tmp","timestamp":"2026-05-04T23:11:51.631Z"}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_user_role_returns_none() {
        let line = r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_unknown_top_level_type_returns_none() {
        let line = r#"{"type":"compaction","summary":"…"}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        assert!(parse_transcript_line("not json").is_none());
    }

    #[test]
    fn parse_assistant_with_only_thinking_part_emits_message() {
        let line = r#"{"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"…"}]}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Message { .. } => {}
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_result_without_id_returns_none() {
        let line = r#"{"type":"message","message":{"role":"toolResult","content":[]}}"#;
        assert!(parse_transcript_line(line).is_none());
    }
}
