//! Per-session running token totals.
//!
//! Adapters emit [`super::RawCost`] per transcript line via
//! [`super::TranscriptEvent::Cost`]; the runtime owns one
//! [`SessionCostAggregator`] per active session and feeds raw events into it.
//! Status bar / cost views read [`SessionCostAggregator::snapshot`] for the
//! running total.
//!
//! Why a generic, non-adapter struct: per-line emission is the trait contract
//! (sync, hot path); session-scoped aggregation is runtime state that does not
//! belong to any specific agent. Decoupling lets adapters stay stateless on
//! cost without each one re-implementing accumulation.

use super::RawCost;

/// Running totals snapshot. `Clone` is cheap (six integers).
#[derive(Default, Clone, Debug, serde::Serialize)]
pub struct SessionCostTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub messages_counted: u64,
    /// Last `model_id` observed. Useful when the session uses a single model
    /// throughout (typical case). For multi-model sessions, the last value
    /// wins; richer reporting can iterate the raw events stream.
    pub last_model_id: Option<String>,
}

/// Lock-protected accumulator over [`SessionCostTotals`].
///
/// Uses `parking_lot::Mutex` (sync, sub-microsecond uncontended) instead of
/// `tokio::sync::Mutex` because `add()` is invoked from the transcript hot
/// path; an async lock would force `.await` per line and pollute every caller.
pub struct SessionCostAggregator {
    totals: parking_lot::Mutex<SessionCostTotals>,
}

impl Default for SessionCostAggregator {
    fn default() -> Self {
        Self {
            totals: parking_lot::Mutex::new(SessionCostTotals::default()),
        }
    }
}

impl SessionCostAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a single line's raw cost into the running totals.
    pub fn add(&self, raw: &RawCost) {
        let mut t = self.totals.lock();
        t.input_tokens = t.input_tokens.saturating_add(raw.input_tokens);
        t.output_tokens = t.output_tokens.saturating_add(raw.output_tokens);
        t.cache_read_tokens = t.cache_read_tokens.saturating_add(raw.cache_read_tokens);
        t.cache_write_tokens = t.cache_write_tokens.saturating_add(raw.cache_write_tokens);
        t.messages_counted = t.messages_counted.saturating_add(1);
        if raw.model_id.is_some() {
            t.last_model_id = raw.model_id.clone();
        }
    }

    /// Snapshot of the current running totals.
    pub fn snapshot(&self) -> SessionCostTotals {
        self.totals.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregator_starts_empty() {
        let agg = SessionCostAggregator::new();
        let snap = agg.snapshot();
        assert_eq!(snap.input_tokens, 0);
        assert_eq!(snap.output_tokens, 0);
        assert_eq!(snap.messages_counted, 0);
        assert!(snap.last_model_id.is_none());
    }

    #[test]
    fn aggregator_accumulates_across_messages() {
        let agg = SessionCostAggregator::new();
        agg.add(&RawCost {
            model_id: Some("claude-sonnet-4".into()),
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            cache_write_tokens: 5,
        });
        agg.add(&RawCost {
            model_id: Some("claude-sonnet-4".into()),
            input_tokens: 30,
            output_tokens: 70,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        });
        let snap = agg.snapshot();
        assert_eq!(snap.input_tokens, 130);
        assert_eq!(snap.output_tokens, 120);
        assert_eq!(snap.cache_read_tokens, 10);
        assert_eq!(snap.cache_write_tokens, 5);
        assert_eq!(snap.messages_counted, 2);
        assert_eq!(snap.last_model_id.as_deref(), Some("claude-sonnet-4"));
    }

    #[test]
    fn aggregator_preserves_last_model_id_when_new_event_has_none() {
        let agg = SessionCostAggregator::new();
        agg.add(&RawCost {
            model_id: Some("claude-sonnet-4".into()),
            ..Default::default()
        });
        agg.add(&RawCost {
            model_id: None,
            input_tokens: 5,
            ..Default::default()
        });
        let snap = agg.snapshot();
        // last_model_id stays sticky on a None event so a transient missing
        // field doesn't blank the model badge in the UI.
        assert_eq!(snap.last_model_id.as_deref(), Some("claude-sonnet-4"));
    }
}
