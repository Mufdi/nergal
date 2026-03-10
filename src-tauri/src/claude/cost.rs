#![allow(dead_code)]
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Serialize;

/// Accumulated token usage from a Claude Code session.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CostSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
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

    /// Format as a short display string.
    pub fn display(&self) -> String {
        let cost = self.estimated_cost_usd();
        if cost < 0.01 {
            format!("~${cost:.4}")
        } else {
            format!("${cost:.2}")
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

    summary
}
