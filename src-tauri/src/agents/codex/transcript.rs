//! Codex rollout JSONL parser.
//!
//! Codex writes `~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-<uuid>.jsonl`
//! while a session runs. Each line is a JSON record carrying message
//! content, tool calls/results, and (on assistant messages) OpenAI-naming
//! token usage: `prompt_tokens`, `completion_tokens`, with optional
//! `prompt_cache_read_tokens` and `prompt_cache_write_tokens` extensions.
//!
//! The parser only extracts what cluihud needs for the foundation:
//! - cost (per-message tokens → [`RawCost`])
//! - tool calls + results (→ [`TranscriptEvent::ToolUse`] / `ToolResult`)
//! - assistant messages (→ [`TranscriptEvent::Message`])
//!
//! Anything else parses to [`None`] and the dispatcher skips it.

use crate::agents::{RawCost, TranscriptEvent};

/// Parse a single Codex rollout JSONL line. Returns `None` for
/// unrecognised types and malformed JSON.
pub fn parse_transcript_line(line: &str) -> Option<TranscriptEvent> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let entry_type = entry.get("type").and_then(|v| v.as_str())?;

    match entry_type {
        // Codex emits `message` records with `role: assistant|user|tool`.
        "message" => {
            let role = entry.get("role").and_then(|v| v.as_str())?;
            if role == "assistant" {
                if let Some(usage) = entry.get("usage") {
                    return Some(TranscriptEvent::Cost(extract_raw_cost(&entry, usage)));
                }
                let content = entry.get("content").cloned().unwrap_or_default();
                return Some(TranscriptEvent::Message {
                    role: role.to_string(),
                    content: content.to_string(),
                    model: entry
                        .get("model")
                        .and_then(|v| v.as_str().map(str::to_string)),
                });
            }
            None
        }
        // Tool invocation. Codex emits `function_call` (OpenAI-style) and
        // some sources emit `tool_call`. Accept both.
        "function_call" | "tool_call" => {
            let name = entry.get("name").and_then(|v| v.as_str())?.to_string();
            let arguments = entry
                .get("arguments")
                .or_else(|| entry.get("input"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Some(TranscriptEvent::ToolUse {
                name,
                input: arguments,
            })
        }
        "function_call_output" | "tool_result" => {
            let tool_use_id = entry
                .get("call_id")
                .or_else(|| entry.get("tool_call_id"))
                .or_else(|| entry.get("toolCallId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let output = entry
                .get("output")
                .or_else(|| entry.get("result"))
                .cloned()
                .unwrap_or_default();
            Some(TranscriptEvent::ToolResult {
                tool_use_id,
                output,
            })
        }
        _ => None,
    }
}

fn extract_raw_cost(entry: &serde_json::Value, usage: &serde_json::Value) -> RawCost {
    // OpenAI rollout fields: prompt_tokens, completion_tokens. Codex appears
    // to also emit Anthropic-flavoured names in some configurations; fall
    // through both to be defensive.
    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read_tokens = usage
        .get("prompt_cache_read_tokens")
        .or_else(|| usage.get("cache_read_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write_tokens = usage
        .get("prompt_cache_write_tokens")
        .or_else(|| usage.get("cache_creation_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    RawCost {
        model_id: entry
            .get("model")
            .and_then(|v| v.as_str().map(str::to_string)),
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_assistant_with_openai_usage_emits_cost() {
        let line = r#"{"type":"message","role":"assistant","model":"gpt-5","usage":{"prompt_tokens":120,"completion_tokens":42}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Cost(raw) => {
                assert_eq!(raw.input_tokens, 120);
                assert_eq!(raw.output_tokens, 42);
                assert_eq!(raw.model_id.as_deref(), Some("gpt-5"));
            }
            other => panic!("expected Cost, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_with_anthropic_naming_falls_back() {
        let line = r#"{"type":"message","role":"assistant","model":"claude","usage":{"input_tokens":50,"output_tokens":10,"cache_read_input_tokens":2}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Cost(raw) => {
                assert_eq!(raw.input_tokens, 50);
                assert_eq!(raw.output_tokens, 10);
                assert_eq!(raw.cache_read_tokens, 2);
            }
            other => panic!("expected Cost, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_call_emits_tool_use() {
        let line = r#"{"type":"function_call","name":"shell","arguments":{"cmd":"ls"}}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::ToolUse { name, input } => {
                assert_eq!(name, "shell");
                assert_eq!(input.get("cmd").and_then(|v| v.as_str()), Some("ls"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_call_output_emits_tool_result() {
        let line = r#"{"type":"function_call_output","call_id":"call_1","output":"hi"}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::ToolResult {
                tool_use_id,
                output,
            } => {
                assert_eq!(tool_use_id, "call_1");
                assert_eq!(output.as_str(), Some("hi"));
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_user_message_returns_none() {
        let line = r#"{"type":"message","role":"user","content":"hi"}"#;
        assert!(parse_transcript_line(line).is_none());
    }

    #[test]
    fn parse_assistant_without_usage_emits_message() {
        let line = r#"{"type":"message","role":"assistant","content":"hi","model":"gpt-5"}"#;
        match parse_transcript_line(line).unwrap() {
            TranscriptEvent::Message { role, model, .. } => {
                assert_eq!(role, "assistant");
                assert_eq!(model.as_deref(), Some("gpt-5"));
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_type_returns_none() {
        assert!(parse_transcript_line(r#"{"type":"system"}"#).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        assert!(parse_transcript_line("garbage").is_none());
    }
}
