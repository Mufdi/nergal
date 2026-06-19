//! In-memory registry of recent writes for echo-dedup and conflict detection.
//!
//! After a successful write the command records `(issue_id, field,
//! written_value, pre_write_value, at)` here.  On the next poll the run loop
//! reads a registry snapshot and, for each entry, compares the
//! server-current field value (read from the post-reconcile mirror) to the
//! written value:
//!   - match → own echo → suppress notification, clear the entry
//!   - neither written_value nor pre_write_value → scalar conflict → emit
//!     `linear:write-conflict`
//!
//! TTL is `2 × DEFAULT_POLL_INTERVAL_SECS` (seconds) so the echo poll always
//! arrives before expiry.  A silently-failed write whose entry expires never
//! suppresses a real remote change for an unbounded window (see design Risks).
//!
//! The registry is purely in-memory daemon state.  A crash loses pending
//! entries; the next poll will treat the user's own edit as a remote change
//! and produce a one-shot spurious toast — benign and documented (design
//! Risk: "recent_writes crash-loss").
//!
//! ## Comment post-once model (Decision 4)
//!
//! `post_comment` / `verify_comment_landed` are NOT Tauri commands.  They are
//! wrapped behind the confirmation-token gate in `closure.rs` (Decision 5).
//! Comments are fundamentally different from field writes: append-only, no
//! optimistic insert, and ambiguous failures must never auto-retry.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use rusqlite::Connection;

use super::client::LinearClient;
use super::mirror;

use super::DEFAULT_POLL_INTERVAL_SECS;

/// TTL ≥ 2 × the poll interval so the echo cycle always lands before expiry.
pub const WRITE_TTL: Duration = Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS * 2);

/// Identifies which issue field was written.
///
/// Linear's in-scope write surface is all-scalar (no set fields in this
/// change): `State` and `Assignee` both carry a single value.  There is no
/// additive-merge branch (see design Decision 3 — the ClickUp additive path is
/// intentionally absent here as a scoping decision, not an oversight).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteField {
    State,
    Assignee,
}

/// A single recorded write.
#[derive(Debug, Clone)]
pub struct WriteEntry {
    pub issue_id: String,
    pub field: WriteField,
    /// The value we sent to the API.
    pub written_value: String,
    /// The value in the mirror immediately before we sent the write.
    pub pre_write_value: Option<String>,
    pub at: Instant,
}

/// Composite key for the registry map.
type Key = (String, WriteField);

pub struct WritebackRegistry {
    entries: Mutex<HashMap<Key, WriteEntry>>,
}

impl Default for WritebackRegistry {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl WritebackRegistry {
    /// Record a write.  Overwrites any prior entry for the same
    /// `(issue_id, field)` pair — the latest write is the one to echo-check.
    ///
    /// Call this BEFORE the API call (provisional record) to close the TOCTOU
    /// window where a concurrent poll lands between the write hitting Linear and
    /// the command resuming.  Clear on API failure via `clear_entry`.
    pub fn record(
        &self,
        issue_id: impl Into<String>,
        field: WriteField,
        written_value: impl Into<String>,
        pre_write_value: Option<impl Into<String>>,
    ) {
        let issue_id = issue_id.into();
        let written_value = written_value.into();
        let pre_write_value = pre_write_value.map(Into::into);
        let entry = WriteEntry {
            issue_id: issue_id.clone(),
            field: field.clone(),
            written_value,
            pre_write_value,
            at: Instant::now(),
        };
        if let Ok(mut guard) = self.entries.lock() {
            guard.insert((issue_id, field), entry);
        }
    }

    /// Return a snapshot of all non-expired entries for a given issue.
    pub fn entries_for_issue(&self, issue_id: &str) -> Vec<WriteEntry> {
        let now = Instant::now();
        let Ok(guard) = self.entries.lock() else {
            return Vec::new();
        };
        guard
            .values()
            .filter(|e| e.issue_id == issue_id && now.duration_since(e.at) < WRITE_TTL)
            .cloned()
            .collect()
    }

    /// Return all non-expired issue ids that have entries.
    pub fn tracked_issue_ids(&self) -> Vec<String> {
        let now = Instant::now();
        let Ok(guard) = self.entries.lock() else {
            return Vec::new();
        };
        let mut ids: Vec<String> = guard
            .values()
            .filter(|e| now.duration_since(e.at) < WRITE_TTL)
            .map(|e| e.issue_id.clone())
            .collect();
        ids.dedup();
        ids
    }

    /// Clear a single `(issue_id, field)` entry — called after a confirmed
    /// echo or on API failure.
    pub fn clear_entry(&self, issue_id: &str, field: &WriteField) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.remove(&(issue_id.to_string(), field.clone()));
        }
    }

    /// Remove all expired entries.  Called once per reconcile cycle to bound
    /// memory use on active workspaces.
    pub fn purge_expired(&self) {
        let now = Instant::now();
        if let Ok(mut guard) = self.entries.lock() {
            guard.retain(|_, e| now.duration_since(e.at) < WRITE_TTL);
        }
    }
}

// ── Conflict event payload ──

/// Emitted as `linear:write-conflict` when the server's value for a scalar
/// field neither matches what we wrote nor what was there before our write.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WriteConflict {
    pub issue_id: String,
    pub field: String,
    pub your_value: String,
    pub remote_value: String,
}

// ── Echo + conflict check (pure, callable from tests without network/DB) ──

/// Result of examining one `WriteEntry` against a fetched server value.
#[derive(Debug, PartialEq, Eq)]
pub enum EchoCheckResult {
    /// Server value matches what we wrote → own echo, suppress.
    OwnEcho,
    /// Server value matches neither written nor pre-write value.
    ScalarConflict(WriteConflict),
    /// Server value equals the pre-write value: our write hasn't landed yet.
    Unrelated,
}

/// Compare one `WriteEntry` against the server's current value for the field.
///
/// `server_value` is the server-current field value from the post-reconcile
/// mirror (canonical string, same encoding as `written_value`).
pub fn check_echo(entry: &WriteEntry, server_value: &str) -> EchoCheckResult {
    if server_value == entry.written_value {
        return EchoCheckResult::OwnEcho;
    }
    let pre = entry.pre_write_value.as_deref();
    if pre == Some(server_value) {
        // Server still matches what was there before our write: our write
        // hasn't propagated yet (or was a no-op from the server's view).
        EchoCheckResult::Unrelated
    } else {
        EchoCheckResult::ScalarConflict(WriteConflict {
            issue_id: entry.issue_id.clone(),
            field: format!("{:?}", entry.field),
            your_value: entry.written_value.clone(),
            remote_value: server_value.to_string(),
        })
    }
}

// ── Comment post-once model (Decision 4, tasks 4.1-4.3) ──

/// Timestamp tolerance for matching an uncertain comment against live-fetched
/// ones.  Linear assigns the server timestamp, which can differ slightly from
/// our local `now()`.  5 seconds covers typical network round-trips.
const COMMENT_TIMESTAMP_TOLERANCE_SECS: i64 = 5;

/// Structured outcome surfaced to the frontend (and the token gate).
///
/// `serde::Serialize` lets the closure command return this directly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommentOutcome {
    /// Comment half was not requested in the closure token; skipped.
    Skipped,
    /// API returned the created comment id; comment inserted into the mirror.
    Posted { id: String },
    /// Clear HTTP error (non-2xx response); comment was NOT sent.  Safe to
    /// surface the error and let the user retry.
    Failed { error: String },
    /// Network/timeout failure with no confirmation from the server.  The
    /// comment MAY have been received.  The caller MUST call
    /// `verify_comment_landed` before offering a retry to avoid duplicates.
    Uncertain { error: String },
}

/// Post a comment and, on confirmed success, insert it into the local mirror.
///
/// Error classification:
/// - Reqwest "connection reset / timed out / no response" (no HTTP status) →
///   `Uncertain` — the server may have received the request.
/// - Any other `anyhow` error → `Failed`.
/// - Success → `Posted`; mirror insert happens immediately so the detail
///   view shows the comment without waiting for the next poll.
///
/// No optimistic insert (Decision 4): the mirror is only updated after the API
/// confirms.  On `Uncertain` the mirror is NOT written; `verify_comment_landed`
/// must confirm before the caller may insert.
///
/// `author_id` is the viewer id string used to match uncertain comments.
pub async fn post_comment(
    client: &LinearClient,
    conn: &Connection,
    issue_id: &str,
    body: &str,
    author_id: Option<&str>,
) -> CommentOutcome {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    match client.comment_create(issue_id, body).await {
        Ok(comment_id) => {
            if let Err(e) =
                mirror::upsert_comment(conn, issue_id, &comment_id, author_id, body, now_secs)
            {
                // Mirror insert failure is non-fatal: the next poll will
                // reconcile the comment from the server.  Log and continue.
                tracing::warn!(
                    issue = %issue_id,
                    comment = %comment_id,
                    "linear comment mirror insert after post failed: {e:#}"
                );
            }
            CommentOutcome::Posted { id: comment_id }
        }
        Err(e) => classify_comment_error(e),
    }
}

/// Re-fetch the issue's comments from the API and check whether a comment with
/// the given `body` and `author_id` landed within `COMMENT_TIMESTAMP_TOLERANCE_SECS`
/// of `posted_at_secs`.
///
/// Returns `true` when a matching comment is found.  The caller can then
/// insert it into the mirror and treat the original uncertain outcome as
/// confirmed.
///
/// Returns `false` when no match is found — the comment was not received and
/// a retry is safe.
///
/// Returns `Err` when the live fetch itself fails (offline / rate-limited) —
/// the caller must NOT retry in that case since it cannot confirm either way.
pub async fn verify_comment_landed(
    client: &LinearClient,
    issue_id: &str,
    body: &str,
    author_id: Option<&str>,
    posted_at_secs: i64,
) -> Result<bool> {
    let detail = client.issue_detail(issue_id).await?;
    let found = detail.comments.iter().any(|c| {
        let body_match = c.body.as_deref() == Some(body);
        let author_match = match (author_id, c.user.as_ref().map(|u| u.id.as_str())) {
            (Some(a), Some(b)) => a == b,
            // No author info on either side: match on body + timestamp only.
            (None, _) | (_, None) => true,
        };
        let ts_match = c
            .created_at
            .as_deref()
            .and_then(super::model::iso8601_to_epoch)
            .map(|t| (t - posted_at_secs).abs() <= COMMENT_TIMESTAMP_TOLERANCE_SECS)
            .unwrap_or(false);
        body_match && author_match && ts_match
    });
    Ok(found)
}

/// Classify a `client.comment_create` error into `Failed` vs `Uncertain`.
///
/// Reqwest errors that carry no HTTP response (timeout, connection reset,
/// DNS failure) are `Uncertain` — the server may have received the POST.
/// Everything else (HTTP non-2xx, body parsing, serialization) is `Failed`.
///
/// `pub` so `closure.rs` can reuse the classification when it performs the
/// comment_create call directly (the closure command cannot hold a `&Connection`
/// across the `.await` and must inline the post logic).
pub fn classify_comment_error(err: anyhow::Error) -> CommentOutcome {
    let msg = format!("{err:#}");
    // Reqwest surfaces network-layer errors without an HTTP status in the
    // error chain.  Our `bail!` calls always include "HTTP {status}", so the
    // absence of "HTTP " is the reliable network-error marker.
    let is_network_error = !msg.contains("HTTP ");
    if is_network_error {
        CommentOutcome::Uncertain { error: msg }
    } else {
        CommentOutcome::Failed { error: msg }
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_entry(issue: &str, field: WriteField, written: &str, pre: Option<&str>) -> WriteEntry {
        WriteEntry {
            issue_id: issue.into(),
            field,
            written_value: written.into(),
            pre_write_value: pre.map(Into::into),
            at: Instant::now(),
        }
    }

    // 2.3 TTL ≥ 2 × poll interval
    #[test]
    fn ttl_is_at_least_two_poll_intervals() {
        let two_polls = Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS * 2);
        assert!(
            WRITE_TTL >= two_polls,
            "WRITE_TTL ({WRITE_TTL:?}) must be >= 2 * poll interval ({two_polls:?})"
        );
    }

    // 2.3 Expired entries are not returned
    #[test]
    fn expired_entries_not_returned() {
        let reg = WritebackRegistry::default();
        {
            let entry = WriteEntry {
                issue_id: "i1".into(),
                field: WriteField::State,
                written_value: "done".into(),
                pre_write_value: Some("open".into()),
                at: Instant::now()
                    .checked_sub(WRITE_TTL + Duration::from_secs(1))
                    .unwrap_or_else(Instant::now),
            };
            let mut guard = reg.entries.lock().unwrap();
            guard.insert(("i1".into(), WriteField::State), entry);
        }
        assert!(
            reg.entries_for_issue("i1").is_empty(),
            "expired entry must not be returned"
        );
        reg.purge_expired();
        {
            let guard = reg.entries.lock().unwrap();
            assert!(guard.is_empty(), "expired entry must be removed by purge");
        }
    }

    // 2.3 Fresh entries ARE returned and clearable
    #[test]
    fn fresh_entry_returned_and_clearable() {
        let reg = WritebackRegistry::default();
        reg.record("i2", WriteField::State, "in_progress", Some("backlog"));
        let entries = reg.entries_for_issue("i2");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].written_value, "in_progress");

        reg.clear_entry("i2", &WriteField::State);
        assert!(reg.entries_for_issue("i2").is_empty());
    }

    // 3.3 Own write value-match → OwnEcho
    #[test]
    fn own_echo_when_server_matches_written() {
        let entry = make_entry("i1", WriteField::State, "done", Some("open"));
        assert_eq!(check_echo(&entry, "done"), EchoCheckResult::OwnEcho);
    }

    // 3.3 Scalar conflict: server value != written AND != pre-write
    #[test]
    fn scalar_conflict_when_remote_supersedes() {
        let entry = make_entry("i1", WriteField::State, "done", Some("open"));
        let result = check_echo(&entry, "in_review");
        match result {
            EchoCheckResult::ScalarConflict(c) => {
                assert_eq!(c.issue_id, "i1");
                assert_eq!(c.your_value, "done");
                assert_eq!(c.remote_value, "in_review");
            }
            other => panic!("expected ScalarConflict, got {other:?}"),
        }
    }

    // 3.3 Unrelated when server still equals pre-write (our write not yet landed)
    #[test]
    fn unrelated_when_server_matches_pre_write() {
        let entry = make_entry("i1", WriteField::State, "done", Some("open"));
        assert_eq!(check_echo(&entry, "open"), EchoCheckResult::Unrelated);
    }

    // 3.3 Assignee write: OwnEcho when server matches what we wrote
    #[test]
    fn own_assignee_echo_when_server_matches() {
        let entry = make_entry("i1", WriteField::Assignee, "user-abc", Some("user-xyz"));
        assert_eq!(check_echo(&entry, "user-abc"), EchoCheckResult::OwnEcho);
    }

    // 3.3 REGRESSION: own assignment-write is filtered from newly_assigned.
    // The filter runs in the run loop; this test verifies that the check_echo
    // function returns OwnEcho for the exact value we wrote, which is the
    // signal the run loop uses to suppress notify_assignments.
    #[test]
    fn own_assignment_write_returns_own_echo_not_conflict() {
        let entry = make_entry("i2", WriteField::Assignee, "viewer-id", Some(""));
        assert_eq!(
            check_echo(&entry, "viewer-id"),
            EchoCheckResult::OwnEcho,
            "own assign-to-me must be OwnEcho to be filtered from newly_assigned"
        );
    }

    // 3.3 Divergent remote assignee → ScalarConflict (no additive merge branch)
    #[test]
    fn assignee_conflict_when_remote_supersedes() {
        let entry = make_entry("i1", WriteField::Assignee, "user-abc", Some("user-xyz"));
        let result = check_echo(&entry, "user-new");
        match result {
            EchoCheckResult::ScalarConflict(c) => {
                assert_eq!(c.field, "Assignee");
                assert_eq!(c.your_value, "user-abc");
                assert_eq!(c.remote_value, "user-new");
            }
            other => panic!("expected ScalarConflict, got {other:?}"),
        }
    }

    // ── classify_comment_error ──

    #[test]
    fn network_error_classified_as_uncertain() {
        let err = anyhow::anyhow!("linear POST commentCreate: connection reset by peer");
        assert!(matches!(
            classify_comment_error(err),
            CommentOutcome::Uncertain { .. }
        ));
    }

    #[test]
    fn http_error_classified_as_failed() {
        let err = anyhow::anyhow!("linear graphql error: HTTP 400");
        assert!(matches!(
            classify_comment_error(err),
            CommentOutcome::Failed { .. }
        ));
    }

    #[test]
    fn http_401_is_failed_not_uncertain() {
        let err = anyhow::anyhow!("linear POST commentCreate: HTTP 401");
        assert!(matches!(
            classify_comment_error(err),
            CommentOutcome::Failed { .. }
        ));
    }

    // 2.3 API failure clears the provisional record
    #[test]
    fn api_failure_clears_provisional_record() {
        let reg = WritebackRegistry::default();
        // Record before the (simulated) API call.
        reg.record("i3", WriteField::State, "done", Some("open"));
        assert!(
            !reg.entries_for_issue("i3").is_empty(),
            "provisional record must exist"
        );
        // Simulate API failure: clear the entry.
        reg.clear_entry("i3", &WriteField::State);
        assert!(reg.entries_for_issue("i3").is_empty(), "cleared on failure");
    }

    // tracked_issue_ids returns ids with live entries
    #[test]
    fn tracked_issue_ids_returns_live_entries() {
        let reg = WritebackRegistry::default();
        reg.record("i4", WriteField::State, "done", None::<String>);
        reg.record("i4", WriteField::Assignee, "u1", None::<String>);
        reg.record("i5", WriteField::State, "backlog", None::<String>);
        let mut ids = reg.tracked_issue_ids();
        ids.sort();
        assert!(ids.contains(&"i4".to_string()));
        assert!(ids.contains(&"i5".to_string()));
    }
}
