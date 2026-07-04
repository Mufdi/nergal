//! Linear write-field surface (all-scalar), and the comment post-once model.
//!
//! The registry mechanics (`WritebackRegistry`, `WriteEntry`, `check_echo`,
//! `EchoCheckResult`) live in `tracker_shared::writeback_registry`, shared
//! with ClickUp.  `WriteConflict` stays here: it is the `Serialize`d event
//! payload emitted as `linear:write-conflict` with Linear's `issue_id` key
//! (ClickUp's wire payload keys on `task_id` instead, so it stays per-tracker
//! too — see `tracker_shared::writeback_registry` module doc).
//!
//! ## Comment post-once model (Decision 4)
//!
//! `post_comment` / `verify_comment_landed` are NOT Tauri commands.  They are
//! wrapped behind the confirmation-token gate in `closure.rs` (Decision 5).
//! Comments are fundamentally different from field writes: append-only, no
//! optimistic insert, and ambiguous failures must never auto-retry.

use std::time::Duration;

use anyhow::Result;
use rusqlite::Connection;

use super::client::LinearClient;
use super::mirror;

use super::DEFAULT_POLL_INTERVAL_SECS;
pub(crate) use crate::tracker_shared::writeback_registry::{EchoCheckResult, check_echo};
use crate::tracker_shared::writeback_registry::{
    FieldClass, WriteFieldClass, WritebackRegistry as GenericWritebackRegistry,
};

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
    Cycle,
}

impl WriteFieldClass for WriteField {
    fn field_class(&self) -> FieldClass {
        // All-scalar write surface (see enum doc) — never Additive.
        FieldClass::Scalar
    }
}

/// Linear's writeback registry, keyed by issue id.
pub type WritebackRegistry = GenericWritebackRegistry<WriteField>;

impl Default for WritebackRegistry {
    fn default() -> Self {
        Self::new(WRITE_TTL)
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
    use crate::tracker_shared::writeback_registry::WriteEntry;
    use std::time::{Duration, Instant};

    fn make_entry(
        issue: &str,
        field: WriteField,
        written: &str,
        pre: Option<&str>,
    ) -> WriteEntry<WriteField> {
        WriteEntry {
            id: issue.into(),
            field,
            written_value: written.into(),
            pre_write_value: pre.map(Into::into),
            at: Instant::now(),
        }
    }

    // Registry mechanics (record/entries_for/clear_entry/purge_expired/
    // tracked_ids) and the generic OwnEcho/ScalarConflict/Unrelated shape of
    // check_echo are consolidated in `tracker_shared::writeback_registry`'s
    // own test module — these tests cover only what's Linear-specific: the
    // WRITE_TTL derivation and the real Assignee-field regression.

    // 2.3 TTL ≥ 2 × poll interval
    #[test]
    fn ttl_is_at_least_two_poll_intervals() {
        let two_polls = Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS * 2);
        assert!(
            WRITE_TTL >= two_polls,
            "WRITE_TTL ({WRITE_TTL:?}) must be >= 2 * poll interval ({two_polls:?})"
        );
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
                assert_eq!(c.id, "i1");
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
}
