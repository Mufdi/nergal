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

/// Parse token usage from all entries in a Claude Code transcript (.jsonl).
pub fn parse_cost_from_transcript(path: &Path) -> CostSummary {
    let mut summary = CostSummary::default();

    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return summary,
    };

    let reader = BufReader::new(file);

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

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

    summary.with_cost()
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
}
