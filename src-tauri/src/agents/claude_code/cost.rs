#![allow(dead_code)]
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Serialize;

use crate::agents::RawCost;
use crate::agents::cost_aggregator::SessionCostTotals;

/// Accumulated token usage from a Claude Code session.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CostSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(rename = "cache_read")]
    pub cache_read_tokens: u64,
    #[serde(rename = "cache_write")]
    pub cache_write_tokens: u64,
    pub total_usd: f64,
}

impl CostSummary {
    /// Estimate USD cost using Sonnet 4 pricing.
    /// Input: $3/MTok, Output: $15/MTok, Cache read: $0.30/MTok, Cache write: $3.75/MTok.
    pub fn estimated_cost_usd(&self) -> f64 {
        let input = self.input_tokens as f64 * 3.0 / 1_000_000.0;
        let output = self.output_tokens as f64 * 15.0 / 1_000_000.0;
        let cache_read = self.cache_read_tokens as f64 * 0.30 / 1_000_000.0;
        let cache_write = self.cache_write_tokens as f64 * 3.75 / 1_000_000.0;
        input + output + cache_read + cache_write
    }

    /// Compute total_usd from token counts.
    pub fn with_cost(mut self) -> Self {
        self.total_usd = self.estimated_cost_usd();
        self
    }

    /// Format as a short display string.
    pub fn display(&self) -> String {
        if self.total_usd < 0.01 {
            format!("~${:.4}", self.total_usd)
        } else {
            format!("${:.2}", self.total_usd)
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptScan {
    pub cost: CostSummary,
    pub last_effort: Option<String>,
}

/// Parse token usage from all entries in a Claude Code transcript (.jsonl).
pub fn parse_cost_from_transcript(path: &Path) -> CostSummary {
    scan_transcript(path).cost
}

/// Top-level `effort` of an assistant record. Other record types can carry the
/// key with a different meaning, so they are excluded.
pub fn assistant_effort_in_line(line: &str) -> Option<String> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    assistant_effort(&entry)
}

fn assistant_effort(entry: &serde_json::Value) -> Option<String> {
    if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
        return None;
    }
    entry
        .get("effort")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Single pass so Stop reads the transcript once for both cost and effort.
pub fn scan_transcript(path: &Path) -> TranscriptScan {
    let mut summary = CostSummary::default();
    let mut last_effort = None;

    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return TranscriptScan {
                cost: summary,
                last_effort,
            };
        }
    };

    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if let Some(effort) = assistant_effort(&entry) {
            last_effort = Some(effort);
        }

        let Some(usage) = entry.get("message").and_then(|m| m.get("usage")) else {
            continue;
        };

        if let Some(v) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
            summary.input_tokens += v;
        }
        if let Some(v) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
            summary.output_tokens += v;
        }
        if let Some(v) = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
        {
            summary.cache_read_tokens += v;
        }
        if let Some(v) = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
        {
            summary.cache_write_tokens += v;
        }
    }

    TranscriptScan {
        cost: summary.with_cost(),
        last_effort,
    }
}

/// Parse a single transcript JSONL line into a [`RawCost`].
///
/// Returns `None` for lines without a `message.usage` payload (most lines —
/// only assistant messages with API usage carry tokens). The caller (typically
/// the runtime feeding a [`crate::agents::cost_aggregator::SessionCostAggregator`])
/// owns running totals.
pub fn parse_cost_line(line: &str) -> Option<RawCost> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let usage = entry.get("message").and_then(|m| m.get("usage"))?;
    Some(RawCost {
        model_id: entry
            .pointer("/message/model")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        input_tokens: usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        output_tokens: usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_read_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_write_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    })
}

/// Sonnet 4 USD pricing bridge — kept private to the CC adapter so the
/// status bar's USD figure does not regress while the agent-agnostic
/// `pricing` module is in flight. Mirrors [`CostSummary::estimated_cost_usd`]
/// over the running totals shape so the call site in the status bar can
/// stay simple.
pub(crate) fn legacy_usd_for_sonnet4(t: &SessionCostTotals) -> f64 {
    const INPUT: f64 = 3.0 / 1_000_000.0;
    const OUTPUT: f64 = 15.0 / 1_000_000.0;
    const CACHE_READ: f64 = 0.30 / 1_000_000.0;
    const CACHE_WRITE: f64 = 3.75 / 1_000_000.0;
    (t.input_tokens as f64) * INPUT
        + (t.output_tokens as f64) * OUTPUT
        + (t.cache_read_tokens as f64) * CACHE_READ
        + (t.cache_write_tokens as f64) * CACHE_WRITE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cost_line_extracts_usage_and_model() {
        let line = r#"{"message":{"model":"claude-sonnet-4","usage":{"input_tokens":120,"output_tokens":42,"cache_read_input_tokens":15,"cache_creation_input_tokens":8}}}"#;
        let raw = parse_cost_line(line).unwrap();
        assert_eq!(raw.model_id.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(raw.input_tokens, 120);
        assert_eq!(raw.output_tokens, 42);
        assert_eq!(raw.cache_read_tokens, 15);
        assert_eq!(raw.cache_write_tokens, 8);
    }

    #[test]
    fn parse_cost_line_returns_none_when_no_usage() {
        let line = r#"{"message":{"role":"user","content":"hello"}}"#;
        assert!(parse_cost_line(line).is_none());
    }

    #[test]
    fn parse_cost_line_returns_none_for_invalid_json() {
        assert!(parse_cost_line("not json").is_none());
    }

    #[test]
    fn legacy_usd_for_sonnet4_matches_existing_pricing() {
        let totals = SessionCostTotals {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
            cache_write_tokens: 1_000_000,
            messages_counted: 0,
            last_model_id: None,
        };
        let usd = legacy_usd_for_sonnet4(&totals);
        // 3 + 15 + 0.30 + 3.75 = 22.05 USD per million across all four buckets
        assert!((usd - 22.05).abs() < 1e-9);
    }

    #[test]
    fn assistant_effort_in_line_reads_top_level_effort() {
        let line = r#"{"type":"assistant","effort":"high","message":{"role":"assistant"}}"#;
        assert_eq!(assistant_effort_in_line(line).as_deref(), Some("high"));
    }

    #[test]
    fn assistant_effort_in_line_ignores_user_records() {
        let line = r#"{"type":"user","effort":"high","message":{"role":"user"}}"#;
        assert_eq!(assistant_effort_in_line(line), None);
    }

    #[test]
    fn assistant_effort_in_line_none_without_effort() {
        let line = r#"{"type":"assistant","message":{"role":"assistant"}}"#;
        assert_eq!(assistant_effort_in_line(line), None);
    }

    #[test]
    fn assistant_effort_in_line_none_for_non_string_effort() {
        let line = r#"{"type":"assistant","effort":{"level":"high"}}"#;
        assert_eq!(assistant_effort_in_line(line), None);
        let line = r#"{"type":"assistant","effort":3}"#;
        assert_eq!(assistant_effort_in_line(line), None);
    }

    #[test]
    fn assistant_effort_in_line_none_for_invalid_json() {
        assert_eq!(assistant_effort_in_line("not json"), None);
    }

    #[test]
    fn assistant_effort_in_line_ignores_effort_nested_in_message() {
        let line = r#"{"type":"assistant","message":{"effort":"high"}}"#;
        assert_eq!(assistant_effort_in_line(line), None);
    }

    fn write_transcript(lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        (dir, path)
    }

    const USAGE_LINE: &str = r#"{"type":"assistant","effort":"high","message":{"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":5}}}"#;

    #[test]
    fn scan_transcript_last_assistant_effort_wins() {
        let (_dir, path) = write_transcript(&[
            r#"{"type":"assistant","effort":"high"}"#,
            r#"{"type":"assistant","effort":"medium"}"#,
            r#"{"type":"user","message":{"role":"user"}}"#,
        ]);
        assert_eq!(
            scan_transcript(&path).last_effort.as_deref(),
            Some("medium")
        );
    }

    #[test]
    fn scan_transcript_none_for_transcript_without_effort() {
        let (_dir, path) = write_transcript(&[
            r#"{"type":"assistant","message":{"role":"assistant"}}"#,
            r#"{"type":"user","message":{"role":"user"}}"#,
        ]);
        assert_eq!(scan_transcript(&path).last_effort, None);
    }

    #[test]
    fn scan_transcript_skips_malformed_trailing_line() {
        let (_dir, path) =
            write_transcript(&[r#"{"type":"assistant","effort":"low"}"#, "{not valid json"]);
        assert_eq!(scan_transcript(&path).last_effort.as_deref(), Some("low"));
    }

    #[test]
    fn scan_transcript_ignores_effort_on_user_records() {
        let (_dir, path) = write_transcript(&[
            r#"{"type":"assistant","effort":"low"}"#,
            r#"{"type":"user","effort":"xhigh"}"#,
        ]);
        assert_eq!(scan_transcript(&path).last_effort.as_deref(), Some("low"));
    }

    #[test]
    fn scan_transcript_cost_equals_parse_cost_from_transcript() {
        let (_dir, path) = write_transcript(&[
            USAGE_LINE,
            "garbage",
            r#"{"type":"assistant","effort":"low","message":{"usage":{"input_tokens":7,"output_tokens":3}}}"#,
        ]);
        let scan = scan_transcript(&path);
        let expected = parse_cost_from_transcript(&path);
        assert_eq!(scan.cost.input_tokens, expected.input_tokens);
        assert_eq!(scan.cost.output_tokens, expected.output_tokens);
        assert_eq!(scan.cost.cache_read_tokens, expected.cache_read_tokens);
        assert_eq!(scan.cost.cache_write_tokens, expected.cache_write_tokens);
        assert!((scan.cost.total_usd - expected.total_usd).abs() < 1e-12);
        assert_eq!(scan.cost.input_tokens, 107);
        assert_eq!(scan.last_effort.as_deref(), Some("low"));
    }

    #[test]
    fn scan_transcript_missing_file_yields_default_cost_and_no_effort() {
        let dir = tempfile::tempdir().unwrap();
        let scan = scan_transcript(&dir.path().join("absent.jsonl"));
        assert_eq!(scan.cost.input_tokens, 0);
        assert_eq!(scan.cost.total_usd, 0.0);
        assert_eq!(scan.last_effort, None);
    }
}
