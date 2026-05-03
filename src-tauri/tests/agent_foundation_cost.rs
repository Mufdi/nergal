//! Integration test: end-to-end cost extraction flow preserves the USD figure
//! the CC status bar showed pre-foundation.
//!
//! Pre-foundation: `parse_cost_from_transcript` walked the JSONL, accumulated
//! tokens, and applied Sonnet 4 pricing in one shot. Post-foundation:
//! `parse_cost_line` returns `RawCost` per line; the runtime feeds those into
//! a `SessionCostAggregator` and calls `legacy_usd_for_sonnet4` on the
//! totals. This test verifies both pipelines produce the same USD result.

use cluihud::agents::claude_code::cost::{parse_cost_from_transcript, parse_cost_line};
use cluihud::agents::cost_aggregator::{SessionCostAggregator, SessionCostTotals};

// Mirror of the private legacy_usd_for_sonnet4 in cost.rs. Kept inline in the
// test so the bridge stays `pub(crate)` (external callers should consume the
// generic pricing module once it lands; this test exists only to assert the
// streaming pipeline matches the legacy USD figure for CC).
fn legacy_usd_for_sonnet4(t: &SessionCostTotals) -> f64 {
    const INPUT: f64 = 3.0 / 1_000_000.0;
    const OUTPUT: f64 = 15.0 / 1_000_000.0;
    const CACHE_READ: f64 = 0.30 / 1_000_000.0;
    const CACHE_WRITE: f64 = 3.75 / 1_000_000.0;
    (t.input_tokens as f64) * INPUT
        + (t.output_tokens as f64) * OUTPUT
        + (t.cache_read_tokens as f64) * CACHE_READ
        + (t.cache_write_tokens as f64) * CACHE_WRITE
}

const FIXTURE: &str = r#"{"type":"user","message":{"role":"user","content":"hi"}}
{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-4","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}
{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-4","usage":{"input_tokens":300,"output_tokens":150,"cache_read_input_tokens":50,"cache_creation_input_tokens":0}}}
not even valid json — must be ignored
{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-4","usage":{"input_tokens":42,"output_tokens":7}}}
"#;

#[test]
fn legacy_and_streaming_pipelines_agree_on_usd() {
    // Write a fixture transcript to a temp file so we can call the legacy
    // `parse_cost_from_transcript` against an actual path.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cc_transcript.jsonl");
    std::fs::write(&path, FIXTURE).unwrap();

    let legacy = parse_cost_from_transcript(&path);

    // Streaming: feed each non-empty line through parse_cost_line + aggregator.
    let agg = SessionCostAggregator::new();
    for line in FIXTURE.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(raw) = parse_cost_line(line) {
            agg.add(&raw);
        }
    }
    let totals = agg.snapshot();
    let streaming_usd = legacy_usd_for_sonnet4(&totals);

    // Both paths should agree on token counts and USD.
    assert_eq!(totals.input_tokens, legacy.input_tokens);
    assert_eq!(totals.output_tokens, legacy.output_tokens);
    assert_eq!(totals.cache_read_tokens, legacy.cache_read_tokens);
    assert_eq!(totals.cache_write_tokens, legacy.cache_write_tokens);
    assert!(
        (streaming_usd - legacy.total_usd).abs() < 1e-9,
        "streaming USD {streaming_usd} vs legacy {} differ beyond float epsilon",
        legacy.total_usd
    );

    // Sanity: the fixture totals to 1342 input + 657 output + 250 cache_read +
    // 100 cache_write across three usage lines — the user line and the
    // garbage line drop out.
    assert_eq!(totals.input_tokens, 1342);
    assert_eq!(totals.output_tokens, 657);
    assert_eq!(totals.cache_read_tokens, 250);
    assert_eq!(totals.cache_write_tokens, 100);
    assert_eq!(totals.messages_counted, 3);
}
