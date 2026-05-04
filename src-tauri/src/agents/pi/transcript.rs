//! Pi JSONL transcript parser. One entry per line; `type` discriminates.
//!
//! Documented entry types we care about:
//! - `session` — header (id, version, cwd). Consumed by
//!   [`super::session_resolver::extract_pi_session_uuid`] for resume.
//! - `agent` (role: assistant) — model message; carries `usage` when token
//!   counts are reported. Maps to either `TranscriptEvent::Cost` (when usage
//!   is present) or `TranscriptEvent::Message`.
//! - `tool_call` — agent invokes a tool. Maps to `TranscriptEvent::ToolUse`.
//! - `tool_result` — tool returns. Maps to `TranscriptEvent::ToolResult`.
//!
//! Any other type (`compaction`, `model_change`, `branch_summary`, future
//! variants) parses to `Unknown` and the dispatcher returns `None`.

use serde::Deserialize;

use crate::agents::{RawCost, TranscriptEvent};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum PiEntry {
    #[serde(rename = "session")]
    Session {
        // Captured for completeness; not consumed by the line-level parser.
        #[serde(default)]
        #[allow(dead_code)]
        id: Option<String>,
    },
    #[serde(rename = "agent")]
    Agent {
        role: String,
        #[serde(default)]
        content: serde_json::Value,
        #[serde(default)]
        usage: Option<PiUsage>,
        #[serde(default)]
        model: Option<String>,
    },
    #[serde(rename = "tool_call")]
    ToolCall {
        #[serde(default)]
        #[allow(dead_code)]
        id: Option<String>,
        name: String,
        #[serde(default)]
        arguments: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(default)]
        output: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
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
    // Pi may emit a `cost.usd` field; per Decision 6 we discard it (cost
    // pricing belongs to the cluihud-side pricing module, not the trait).
}

/// Parse a single Pi JSONL line. Returns `None` for headers, unknown types,
/// or malformed JSON — the tail watcher just skips and continues on `None`.
pub fn parse_transcript_line(line: &str) -> Option<TranscriptEvent> {
    let entry: PiEntry = serde_json::from_str(line).ok()?;
    match entry {
        PiEntry::Agent {
            role,
            content,
            usage,
            model,
        } if role == "assistant" => {
            if let Some(u) = usage {
                return Some(TranscriptEvent::Cost(RawCost {
                    model_id: model,
                    input_tokens: u.input,
                    output_tokens: u.output,
                    cache_read_tokens: u.cache_read,
                    cache_write_tokens: u.cache_write,
                }));
            }
            Some(TranscriptEvent::Message {
                role,
                // `content` for Pi is sometimes a string, sometimes an array
                // of parts. We pass the raw JSON string to the message
                // surface; a richer chat panel can interpret it later.
                content: content.to_string(),
                model,
            })
        }
        PiEntry::ToolCall {
            name, arguments, ..
        } => Some(TranscriptEvent::ToolUse {
            name,
            input: arguments,
        }),
        PiEntry::ToolResult {
            tool_call_id,
            output,
        } => Some(TranscriptEvent::ToolResult {
            tool_use_id: tool_call_id,
            output,
        }),
        // Headers, user agent messages, unknown types: nothing to emit.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_assistant_with_usage_emits_cost() {
        let line = r#"{"type":"agent","role":"assistant","content":"hi","usage":{"input":10,"output":5,"cacheRead":2,"cacheWrite":1},"model":"glm-4.6"}"#;
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
    fn parse_assistant_without_usage_emits_message() {
        let line = r#"{"type":"agent","role":"assistant","content":"hi"}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Message { role, .. } => assert_eq!(role, "assistant"),
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_call_emits_tool_use() {
        let line = r#"{"type":"tool_call","id":"tc_1","name":"bash","arguments":{"cmd":"ls"}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::ToolUse { name, input } => {
                assert_eq!(name, "bash");
                assert_eq!(input.get("cmd").and_then(|v| v.as_str()), Some("ls"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_result_emits_tool_result() {
        let line = r#"{"type":"tool_result","toolCallId":"tc_1","output":"file1\nfile2"}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::ToolResult {
                tool_use_id,
                output,
            } => {
                assert_eq!(tool_use_id, "tc_1");
                assert_eq!(output.as_str(), Some("file1\nfile2"));
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_session_header_returns_none() {
        let line = r#"{"type":"session","id":"abc-uuid","version":1,"cwd":"/tmp"}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_unknown_type_returns_none() {
        let line = r#"{"type":"compaction","summary":"…"}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_user_role_returns_none() {
        let line = r#"{"type":"agent","role":"user","content":"hi"}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        assert!(parse_transcript_line("not json").is_none());
    }
}
