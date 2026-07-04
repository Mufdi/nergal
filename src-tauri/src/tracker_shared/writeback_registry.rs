//! Shared in-memory registry of recent writes for echo-dedup and conflict
//! detection, used by both the ClickUp and Linear writeback stacks.
//!
//! After a successful write the command records `(id, field, written_value,
//! pre_write_value, at)` here.  On the next poll the reconcile/run-loop reads
//! a registry snapshot and, for entries whose owning item changed, compares
//! the server-current field value to the written value:
//!   - match → own echo → suppress notification, clear the entry
//!   - neither written_value nor pre_write_value → conflict → the caller maps
//!     the neutral `EchoCheckResult::ScalarConflict` to its own tracker's
//!     `WriteConflict` (kept per-tracker: it is `Serialize`d onto the event
//!     wire with a tracker-specific id key, `task_id` vs `issue_id`).
//!
//! `WriteFieldClass` carries each tracker's own write-field enum and its
//! Scalar/Additive split (design Decision 3): ClickUp has a real additive
//! path (assignees, checklist items); Linear's write surface is all-scalar
//! and implements `field_class` returning `Scalar` unconditionally.
//!
//! TTL is a construction parameter (`WritebackRegistry::new(ttl)`), not a
//! shared constant — each tracker derives its own `2 × DEFAULT_POLL_INTERVAL_SECS`
//! from its own poller/run-loop constant, and this module must not import
//! either (would be an import cycle).
//!
//! The registry is purely in-memory daemon state.  A crash loses pending
//! entries; the next poll treats the user's own edit as a remote change and
//! produces a one-shot spurious toast — benign and documented (design Risk
//! §8 / "recent_writes crash-loss").

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per-field class for conflict resolution (design Decision 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldClass {
    /// status / due_date / description / single-select custom field / any
    /// plain scalar value.  LWW + warn when remote supersedes.
    Scalar,
    /// Assignees / checklist / labels / multi-select.  Merge to server
    /// state, no false superseded-warning.
    Additive,
}

/// Carries a tracker's own write-field enum plus its Scalar/Additive split.
pub trait WriteFieldClass: Clone + Eq + std::hash::Hash + std::fmt::Debug {
    fn field_class(&self) -> FieldClass;
}

/// A single recorded write.
#[derive(Debug, Clone)]
pub struct WriteEntry<F> {
    pub id: String,
    pub field: F,
    /// The value we sent to the API.
    pub written_value: String,
    /// The value in the mirror immediately before we sent the write.
    pub pre_write_value: Option<String>,
    pub at: Instant,
}

/// Composite key for the registry map.
type Key<F> = (String, F);

pub struct WritebackRegistry<F: WriteFieldClass> {
    entries: Mutex<HashMap<Key<F>, WriteEntry<F>>>,
    ttl: Duration,
}

impl<F: WriteFieldClass> WritebackRegistry<F> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Record a write.  Overwrites any prior entry for the same `(id, field)`
    /// pair — the latest write is the one to echo-check.
    ///
    /// Call this BEFORE the API call (provisional record) to close the
    /// TOCTOU window where a concurrent poll lands between the write hitting
    /// the tracker and the command resuming.  Clear on API failure via
    /// `clear_entry`.
    pub fn record(
        &self,
        id: impl Into<String>,
        field: F,
        written_value: impl Into<String>,
        pre_write_value: Option<impl Into<String>>,
    ) {
        let id = id.into();
        let written_value = written_value.into();
        let pre_write_value = pre_write_value.map(Into::into);
        let entry = WriteEntry {
            id: id.clone(),
            field: field.clone(),
            written_value,
            pre_write_value,
            at: Instant::now(),
        };
        if let Ok(mut guard) = self.entries.lock() {
            guard.insert((id, field), entry);
        }
    }

    /// Return a snapshot of all non-expired entries for a given id.
    pub fn entries_for(&self, id: &str) -> Vec<WriteEntry<F>> {
        let now = Instant::now();
        let Ok(guard) = self.entries.lock() else {
            return Vec::new();
        };
        guard
            .values()
            .filter(|e| e.id == id && now.duration_since(e.at) < self.ttl)
            .cloned()
            .collect()
    }

    /// Return all non-expired ids that have entries.
    pub fn tracked_ids(&self) -> Vec<String> {
        let now = Instant::now();
        let Ok(guard) = self.entries.lock() else {
            return Vec::new();
        };
        let mut ids: Vec<String> = guard
            .values()
            .filter(|e| now.duration_since(e.at) < self.ttl)
            .map(|e| e.id.clone())
            .collect();
        ids.dedup();
        ids
    }

    /// Clear a single `(id, field)` entry — called after a confirmed echo or
    /// on API failure.
    pub fn clear_entry(&self, id: &str, field: &F) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.remove(&(id.to_string(), field.clone()));
        }
    }

    /// Remove all expired entries.  Called once per reconcile cycle to bound
    /// memory use on active workspaces.
    pub fn purge_expired(&self) {
        let now = Instant::now();
        if let Ok(mut guard) = self.entries.lock() {
            guard.retain(|_, e| now.duration_since(e.at) < self.ttl);
        }
    }
}

// ── Echo + conflict check (pure, callable from tests without network/DB) ──

/// Neutral echo-conflict payload.  `WriteConflict` stays per-tracker (it is
/// `Serialize`d onto the event wire with a tracker-specific id key, `task_id`
/// vs `issue_id`) — callers map this at the emit site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EchoConflict {
    pub id: String,
    pub field: String,
    pub your_value: String,
    pub remote_value: String,
}

/// Result of examining one `WriteEntry` against a fetched server value.
#[derive(Debug, PartialEq, Eq)]
pub enum EchoCheckResult {
    /// Server value matches what we wrote → own echo, suppress notification.
    OwnEcho,
    /// Scalar field: server value matches neither written nor pre-write value.
    ScalarConflict(EchoConflict),
    /// Additive field with diverged value → merge silently, no warning.
    AdditiveDivergence,
    /// No recent write for this field, or the server value equals pre-write
    /// (unchanged from our perspective).
    Unrelated,
}

/// Compare one `WriteEntry` against the server's current value for the field.
///
/// `server_value` is the server-current field value extracted from the
/// fetched/reconciled payload (canonical string, same encoding as
/// `written_value`).
pub fn check_echo<F: WriteFieldClass>(
    entry: &WriteEntry<F>,
    server_value: &str,
) -> EchoCheckResult {
    if server_value == entry.written_value {
        return EchoCheckResult::OwnEcho;
    }

    match entry.field.field_class() {
        FieldClass::Additive => EchoCheckResult::AdditiveDivergence,
        FieldClass::Scalar => {
            let pre = entry.pre_write_value.as_deref();
            if pre == Some(server_value) {
                // Server matches the pre-write value: our write hasn't
                // landed or was already overwritten by itself; unrelated.
                EchoCheckResult::Unrelated
            } else {
                EchoCheckResult::ScalarConflict(EchoConflict {
                    id: entry.id.clone(),
                    field: format!("{:?}", entry.field),
                    your_value: entry.written_value.clone(),
                    remote_value: server_value.to_string(),
                })
            }
        }
    }
}

// ── Tests (consolidated from clickup::writeback + linear::writeback) ──

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum TestField {
        Scalar,
        Additive,
    }

    impl WriteFieldClass for TestField {
        fn field_class(&self) -> FieldClass {
            match self {
                TestField::Scalar => FieldClass::Scalar,
                TestField::Additive => FieldClass::Additive,
            }
        }
    }

    const TEST_TTL: Duration = Duration::from_secs(90);

    fn make_entry(
        id: &str,
        field: TestField,
        written: &str,
        pre: Option<&str>,
    ) -> WriteEntry<TestField> {
        WriteEntry {
            id: id.into(),
            field,
            written_value: written.into(),
            pre_write_value: pre.map(Into::into),
            at: Instant::now(),
        }
    }

    // Expired entries are purged / not returned
    #[test]
    fn expired_entries_not_returned() {
        let reg: WritebackRegistry<TestField> = WritebackRegistry::new(TEST_TTL);
        // Manually inject an entry with an old timestamp by bypassing record().
        {
            let entry = WriteEntry {
                id: "t1".into(),
                field: TestField::Scalar,
                written_value: "done".into(),
                pre_write_value: Some("open".into()),
                at: Instant::now()
                    .checked_sub(TEST_TTL + Duration::from_secs(1))
                    .unwrap_or_else(Instant::now),
            };
            let mut guard = reg.entries.lock().unwrap();
            guard.insert(("t1".into(), TestField::Scalar), entry);
        }
        // expired → not returned by entries_for
        assert!(
            reg.entries_for("t1").is_empty(),
            "expired entry must not be returned"
        );

        // purge_expired also removes it
        reg.purge_expired();
        {
            let guard = reg.entries.lock().unwrap();
            assert!(guard.is_empty(), "expired entry must be removed by purge");
        }
    }

    // Fresh entries ARE returned and clearable
    #[test]
    fn fresh_entry_returned_and_clearable() {
        let reg: WritebackRegistry<TestField> = WritebackRegistry::new(TEST_TTL);
        reg.record("t2", TestField::Scalar, "in progress", Some("open"));
        let entries = reg.entries_for("t2");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].written_value, "in progress");

        reg.clear_entry("t2", &TestField::Scalar);
        assert!(reg.entries_for("t2").is_empty());
    }

    // API failure clears the provisional record (echo-ordering)
    #[test]
    fn api_failure_clears_provisional_record() {
        let reg: WritebackRegistry<TestField> = WritebackRegistry::new(TEST_TTL);
        // Record before the (simulated) API call.
        reg.record("t3", TestField::Scalar, "done", Some("open"));
        assert!(
            !reg.entries_for("t3").is_empty(),
            "provisional record must exist"
        );
        // Simulate API failure: clear the entry.
        reg.clear_entry("t3", &TestField::Scalar);
        assert!(reg.entries_for("t3").is_empty(), "cleared on failure");
    }

    // record without a failure (success path) leaves the entry present
    #[test]
    fn record_without_clear_leaves_entry_present() {
        let reg: WritebackRegistry<TestField> = WritebackRegistry::new(TEST_TTL);
        reg.record("t4", TestField::Scalar, "done", Some("open"));
        assert!(
            !reg.entries_for("t4").is_empty(),
            "entry must remain when no clear_entry is called"
        );
    }

    // tracked_ids returns ids with live entries
    #[test]
    fn tracked_ids_returns_live_entries() {
        let reg: WritebackRegistry<TestField> = WritebackRegistry::new(TEST_TTL);
        reg.record("t5", TestField::Scalar, "done", None::<String>);
        reg.record("t5", TestField::Additive, "x", None::<String>);
        reg.record("t6", TestField::Scalar, "backlog", None::<String>);
        let mut ids = reg.tracked_ids();
        ids.sort();
        assert!(ids.contains(&"t5".to_string()));
        assert!(ids.contains(&"t6".to_string()));
    }

    // TTL is a construction parameter, not a shared constant — two instances
    // with different TTLs expire independently.
    #[test]
    fn ttl_is_a_construction_parameter() {
        let short: WritebackRegistry<TestField> = WritebackRegistry::new(Duration::from_millis(1));
        short.record("t7", TestField::Scalar, "v", None::<String>);
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            short.entries_for("t7").is_empty(),
            "entry must expire per this instance's own TTL"
        );
    }

    // Own write value-match → OwnEcho
    #[test]
    fn own_echo_when_server_matches_written() {
        let entry = make_entry("t1", TestField::Scalar, "done", Some("open"));
        assert_eq!(check_echo(&entry, "done"), EchoCheckResult::OwnEcho);
    }

    // Scalar conflict: server value ≠ written AND ≠ pre-write
    #[test]
    fn scalar_conflict_when_remote_supersedes() {
        let entry = make_entry("t1", TestField::Scalar, "done", Some("open"));
        let result = check_echo(&entry, "in review");
        match result {
            EchoCheckResult::ScalarConflict(c) => {
                assert_eq!(c.id, "t1");
                assert_eq!(c.your_value, "done");
                assert_eq!(c.remote_value, "in review");
            }
            other => panic!("expected ScalarConflict, got {other:?}"),
        }
    }

    // Unrelated when server still equals pre-write (our write not landed yet)
    #[test]
    fn unrelated_when_server_matches_pre_write() {
        let entry = make_entry("t1", TestField::Scalar, "done", Some("open"));
        assert_eq!(check_echo(&entry, "open"), EchoCheckResult::Unrelated);
    }

    // Additive divergence → no conflict warning
    #[test]
    fn additive_divergence_never_scalar_conflict() {
        let entry = make_entry("t1", TestField::Additive, "[1,2]", Some("[1]"));
        // Server has a different value (e.g. two more added remotely).
        let result = check_echo(&entry, "[1,2,3,4]");
        assert_eq!(result, EchoCheckResult::AdditiveDivergence);
    }

    // Own additive write with matching server value → OwnEcho, not divergence.
    #[test]
    fn own_additive_write_is_own_echo_not_divergence() {
        let entry = make_entry("t1", TestField::Additive, "[1,2]", Some("[1]"));
        assert_eq!(check_echo(&entry, "[1,2]"), EchoCheckResult::OwnEcho);
    }
}
