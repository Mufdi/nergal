//! In-memory registry of recent writes for echo-dedup and conflict detection.
//!
//! After a successful write, the command records `(task_id, field,
//! written_value, pre_write_value, at)` here.  On the next poll the reconcile
//! reads a registry snapshot and, for tasks that changed, compares the
//! server-current field value to the written value:
//!   - match → own echo → reconcile silently, clear the entry
//!   - neither written_value nor pre_write_value → conflict → emit event
//!
//! TTL is `2 × DEFAULT_POLL_INTERVAL_SECS` (seconds) so the echo poll always
//! arrives before expiry.  A silently-failed write whose entry expires never
//! suppresses a real remote change for an unbounded window (Risk §8, design).
//!
//! The registry is purely in-memory daemon state.  A crash loses pending
//! entries; the next poll will treat the user's own edit as a remote change
//! and produce a one-shot spurious toast — benign and documented as such
//! (Risk §8, design.md).
//!
//! ## Comment post-once model (Decision 4)
//!
//! `post_comment` / `verify_comment_landed` are NOT Tauri commands.  Module 4
//! wraps them behind the confirmation-token gate (Decision 5).  They live here
//! because comments are structurally different from field writes: append-only,
//! no optimistic insert, and ambiguous failures must never auto-retry.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use rusqlite::Connection;

use super::client::ClickUpClient;
use super::{mirror, model};

use super::poller::DEFAULT_POLL_INTERVAL_SECS;

/// TTL ≥ 2 × the poll interval so the echo cycle always lands before expiry.
pub const WRITE_TTL: Duration = Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS * 2);

/// Identifies which task field was written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteField {
    Status,
    Description,
    DueDate,
    /// Assignees: additive field — merge semantics, no conflict warning.
    Assignees,
    /// Checklist item identified by `"{checklist_id}:{item_id}"`.
    ChecklistItem(String),
    /// Custom field identified by its `field_id`.
    CustomField(String),
}

/// Per-field class for conflict resolution (Decision 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldClass {
    /// status / due_date / description / single-select custom field.
    /// LWW + warn when remote supersedes.
    Scalar,
    /// Assignees / checklist / labels / multi-select.
    /// Merge to server state, no false superseded-warning.
    Additive,
}

impl WriteField {
    pub fn field_class(&self) -> FieldClass {
        match self {
            WriteField::Assignees | WriteField::ChecklistItem(_) => FieldClass::Additive,
            _ => FieldClass::Scalar,
        }
    }
}

/// A single recorded write.
#[derive(Debug, Clone)]
pub struct WriteEntry {
    pub task_id: String,
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
    /// Record a successful write.  Overwrites any prior entry for the same
    /// `(task_id, field)` pair — the latest write is the one we want to echo.
    pub fn record(
        &self,
        task_id: impl Into<String>,
        field: WriteField,
        written_value: impl Into<String>,
        pre_write_value: Option<impl Into<String>>,
    ) {
        let task_id = task_id.into();
        let written_value = written_value.into();
        let pre_write_value = pre_write_value.map(Into::into);
        let entry = WriteEntry {
            task_id: task_id.clone(),
            field: field.clone(),
            written_value,
            pre_write_value,
            at: Instant::now(),
        };
        if let Ok(mut guard) = self.entries.lock() {
            guard.insert((task_id, field), entry);
        }
    }

    /// Return a snapshot of all non-expired entries for a given task.
    ///
    /// Expired entries for OTHER tasks accumulate until the next reconcile
    /// purge; they are never returned.
    pub fn entries_for_task(&self, task_id: &str) -> Vec<WriteEntry> {
        let now = Instant::now();
        let Ok(guard) = self.entries.lock() else {
            return Vec::new();
        };
        guard
            .values()
            .filter(|e| e.task_id == task_id && now.duration_since(e.at) < WRITE_TTL)
            .cloned()
            .collect()
    }

    /// Clear a single `(task_id, field)` entry — called after a confirmed echo.
    pub fn clear_entry(&self, task_id: &str, field: &WriteField) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.remove(&(task_id.to_string(), field.clone()));
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

/// Emitted as `clickup:write-conflict` when the server's value for a scalar
/// field neither matches what we wrote nor what was there before our write.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WriteConflict {
    pub task_id: String,
    pub field: String,
    pub your_value: String,
    pub remote_value: String,
}

// ── Echo + conflict check (pure-ish, callable from tests without network/DB) ──

/// Result of examining one `WriteEntry` against a fetched server value.
#[derive(Debug, PartialEq, Eq)]
pub enum EchoCheckResult {
    /// Server value matches what we wrote → own echo, suppress notification.
    OwnEcho,
    /// Scalar field: server value matches neither written nor pre-write value.
    ScalarConflict(WriteConflict),
    /// Additive field with diverged value → merge silently, no warning.
    AdditiveDivergence,
    /// No recent write for this field, or the server value equals pre-write
    /// (unchanged from our perspective).
    Unrelated,
}

/// Compare one `WriteEntry` against the server's current value for the field.
///
/// `server_value` is the server-current field value extracted from the fetched
/// task payload (as a canonical string, same encoding as `written_value`).
pub fn check_echo(entry: &WriteEntry, server_value: &str) -> EchoCheckResult {
    if server_value == entry.written_value {
        return EchoCheckResult::OwnEcho;
    }

    match entry.field.field_class() {
        FieldClass::Additive => EchoCheckResult::AdditiveDivergence,
        FieldClass::Scalar => {
            let pre = entry.pre_write_value.as_deref();
            if pre == Some(server_value) {
                // Server matches the pre-write value: our write hasn't landed
                // or was already overwritten by itself; treat as unrelated.
                EchoCheckResult::Unrelated
            } else {
                EchoCheckResult::ScalarConflict(WriteConflict {
                    task_id: entry.task_id.clone(),
                    field: format!("{:?}", entry.field),
                    your_value: entry.written_value.clone(),
                    remote_value: server_value.to_string(),
                })
            }
        }
    }
}

// ── Comment post-once model (Decision 4, tasks 4.1-4.3) ──

/// Timestamp tolerance for matching an uncertain comment against live fetched
/// ones: ClickUp assigns the server timestamp, which can differ slightly from
/// our local `now()`.  5 seconds covers typical network round-trip.
const COMMENT_TIMESTAMP_TOLERANCE_MS: i64 = 5_000;

/// Structured outcome surfaced to the frontend (and module 4's token gate).
///
/// `serde::Serialize` lets module 4 return this directly from a Tauri command
/// without an extra wrapper.
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
/// - Any other `anyhow` error (HTTP non-2xx, serialization) → `Failed`.
/// - HTTP 2xx → `Posted`; mirror insert happens immediately so the detail
///   view shows the comment without waiting for the next poll.
///
/// No optimistic insert (Decision 4): the mirror is only updated after the
/// API confirms.  On `Uncertain` the mirror is NOT written; `verify_comment_landed`
/// must confirm before the caller may insert.
///
/// `author_id` and `author_user` are the cached token-user values used to
/// fill the mirror row; `None` values produce a comment with no user metadata,
/// which is valid (the next poll will overwrite with full data).
pub async fn post_comment(
    client: &ClickUpClient,
    conn: &Connection,
    task_id: &str,
    text: &str,
    author_id: Option<i64>,
    author_user: Option<model::User>,
) -> CommentOutcome {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    match client.add_comment(task_id, text).await {
        Ok(comment_id) => {
            let comment = model::Comment {
                id: comment_id.clone(),
                comment_text: Some(text.to_string()),
                user: author_user.or_else(|| {
                    author_id.map(|id| model::User {
                        id: Some(id),
                        ..Default::default()
                    })
                }),
                resolved: Some(false),
                date: Some(now_ms),
                reply_count: Some(0),
            };
            if let Err(e) = mirror::upsert_comment(conn, task_id, &comment) {
                // Mirror insert failure is non-fatal: the next poll will
                // reconcile the comment from the server.  Log and continue.
                tracing::warn!(task = %task_id, comment = %comment_id, "mirror insert after post failed: {e:#}");
            }
            CommentOutcome::Posted { id: comment_id }
        }
        Err(e) => classify_comment_error(e),
    }
}

/// Re-fetch the task's comments from the API and check whether a comment with
/// the given `text` and `author_id` landed within `COMMENT_TIMESTAMP_TOLERANCE_MS`
/// of `posted_at_ms`.
///
/// Returns `true` when a matching comment is found.  The caller can then
/// insert it into the mirror (using `upsert_comment` directly) and treat
/// the original uncertain outcome as confirmed.
///
/// Returns `false` when no match is found — the comment was not received and
/// a retry is safe.
///
/// Returns `Err` when the live fetch itself fails (offline / rate-limited) —
/// the caller must NOT retry in that case since it cannot confirm either way.
pub async fn verify_comment_landed(
    client: &ClickUpClient,
    task_id: &str,
    text: &str,
    author_id: Option<i64>,
    posted_at_ms: i64,
) -> Result<bool> {
    let comments = client.get_task_comments(task_id).await?;
    let found = comments.iter().any(|c| {
        let text_match = c.comment_text.as_deref() == Some(text);
        let author_match = match (author_id, c.user.as_ref().and_then(|u| u.id)) {
            (Some(a), Some(b)) => a == b,
            // No author info on either side: match on text + timestamp only.
            (None, _) | (_, None) => true,
        };
        let timestamp_match = c
            .date
            .map(|d| (d - posted_at_ms).abs() <= COMMENT_TIMESTAMP_TOLERANCE_MS)
            .unwrap_or(false);
        text_match && author_match && timestamp_match
    });
    Ok(found)
}

/// Classify a `client.add_comment` error into `Failed` vs `Uncertain`.
///
/// Reqwest errors that carry no HTTP response (timeout, connection reset,
/// DNS failure) are `Uncertain` — the server may have received the POST.
/// Everything else (HTTP non-2xx, body parsing, serialization) is `Failed`.
///
/// `pub` so `closure.rs` can reuse the classification when it performs the
/// add_comment call directly (module 5 cannot hold a `&Connection` across
/// the `.await` and must inline the post logic).
pub fn classify_comment_error(err: anyhow::Error) -> CommentOutcome {
    let msg = format!("{err:#}");
    // `reqwest` surfaces network-layer errors without an HTTP status in the
    // error chain.  The canonical pattern: the message contains neither
    // "HTTP" nor a status code prefix — but the most reliable check is the
    // absence of "HTTP " in the anyhow chain, since our `bail!` calls always
    // include "HTTP {status}".
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

    fn make_entry(task: &str, field: WriteField, written: &str, pre: Option<&str>) -> WriteEntry {
        WriteEntry {
            task_id: task.into(),
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
            "WRITE_TTL ({WRITE_TTL:?}) must be ≥ 2 × poll interval ({two_polls:?})"
        );
    }

    // 2.3 Expired entries are purged / not returned
    #[test]
    fn expired_entries_not_returned() {
        let reg = WritebackRegistry::default();
        // Manually inject an entry with an old timestamp by bypassing record().
        {
            let entry = WriteEntry {
                task_id: "t1".into(),
                field: WriteField::Status,
                written_value: "done".into(),
                pre_write_value: Some("open".into()),
                at: Instant::now()
                    .checked_sub(WRITE_TTL + Duration::from_secs(1))
                    .unwrap_or_else(Instant::now),
            };
            let mut guard = reg.entries.lock().unwrap();
            guard.insert(("t1".into(), WriteField::Status), entry);
        }
        // expired → not returned by entries_for_task
        assert!(
            reg.entries_for_task("t1").is_empty(),
            "expired entry must not be returned"
        );

        // purge_expired also removes it
        reg.purge_expired();
        {
            let guard = reg.entries.lock().unwrap();
            assert!(guard.is_empty(), "expired entry must be removed by purge");
        }
    }

    // 2.3 Fresh entries ARE returned and cleared
    #[test]
    fn fresh_entry_returned_and_clearable() {
        let reg = WritebackRegistry::default();
        reg.record("t2", WriteField::Status, "in progress", Some("open"));
        let entries = reg.entries_for_task("t2");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].written_value, "in progress");

        reg.clear_entry("t2", &WriteField::Status);
        assert!(reg.entries_for_task("t2").is_empty());
    }

    // 3.3 Own write value-match → OwnEcho
    #[test]
    fn own_echo_when_server_matches_written() {
        let entry = make_entry("t1", WriteField::Status, "done", Some("open"));
        assert_eq!(check_echo(&entry, "done"), EchoCheckResult::OwnEcho);
    }

    // 3.3 Scalar conflict: server value ≠ written AND ≠ pre-write
    #[test]
    fn scalar_conflict_when_remote_supersedes() {
        let entry = make_entry("t1", WriteField::Status, "done", Some("open"));
        let result = check_echo(&entry, "in review");
        match result {
            EchoCheckResult::ScalarConflict(c) => {
                assert_eq!(c.task_id, "t1");
                assert_eq!(c.your_value, "done");
                assert_eq!(c.remote_value, "in review");
            }
            other => panic!("expected ScalarConflict, got {other:?}"),
        }
    }

    // 3.3 Unrelated when server still equals pre-write (our write not landed yet)
    #[test]
    fn unrelated_when_server_matches_pre_write() {
        let entry = make_entry("t1", WriteField::Status, "done", Some("open"));
        assert_eq!(check_echo(&entry, "open"), EchoCheckResult::Unrelated);
    }

    // 3.3 Additive divergence → no conflict warning
    #[test]
    fn additive_divergence_never_scalar_conflict() {
        let entry = make_entry("t1", WriteField::Assignees, "[1,2]", Some("[1]"));
        // Server has different value (e.g. two more added remotely)
        let result = check_echo(&entry, "[1,2,3,4]");
        assert_eq!(result, EchoCheckResult::AdditiveDivergence);
    }

    // 3.3 Checklist item is additive
    #[test]
    fn checklist_item_is_additive_class() {
        let f = WriteField::ChecklistItem("cl1:item1".into());
        assert_eq!(f.field_class(), FieldClass::Additive);
    }

    // 3.3 REGRESSION: own assignment-write (assignees field) with matching server value → OwnEcho
    #[test]
    fn own_assignment_write_is_own_echo_not_conflict() {
        let entry = make_entry("t1", WriteField::Assignees, "[1,2]", Some("[1]"));
        // Server reflects exactly what we wrote.
        assert_eq!(check_echo(&entry, "[1,2]"), EchoCheckResult::OwnEcho);
    }

    // ── classify_comment_error ──

    #[test]
    fn network_error_classified_as_uncertain() {
        // Errors without "HTTP " in the message are network-layer failures.
        let err = anyhow::anyhow!("clickup POST /task/t1/comment: connection reset by peer");
        assert!(matches!(
            classify_comment_error(err),
            CommentOutcome::Uncertain { .. }
        ));
    }

    #[test]
    fn http_error_classified_as_failed() {
        let err = anyhow::anyhow!("clickup POST /task/t1/comment: HTTP 400");
        assert!(matches!(
            classify_comment_error(err),
            CommentOutcome::Failed { .. }
        ));
    }

    #[test]
    fn http_401_is_failed_not_uncertain() {
        let err = anyhow::anyhow!("clickup POST /task/t1/comment: HTTP 401");
        assert!(matches!(
            classify_comment_error(err),
            CommentOutcome::Failed { .. }
        ));
    }

    // ── post_comment integration: mock server + in-memory mirror ──

    fn mirror_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/015_clickup_mirror.sql"))
            .unwrap();
        // Minimal hierarchy so the task FK is satisfied.
        conn.execute_batch(
            "INSERT INTO clickup_spaces(id, name, synced_at) VALUES ('sp1','S',0);
             INSERT INTO clickup_folders(id, space_id, name) VALUES ('f1','sp1','F');
             INSERT INTO clickup_lists(id, folder_id, space_id, name) VALUES ('list1','f1','sp1','L');
             INSERT INTO clickup_tasks(id, list_id, name) VALUES ('task1','list1','T');",
        )
        .unwrap();
        conn
    }

    // Minimal one-request-per-connection HTTP mock (mirrors client.rs pattern).
    async fn spawn_mock(
        responses: Vec<(u16, String)>,
    ) -> (String, std::sync::Arc<tokio::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: std::sync::Arc<tokio::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        tokio::spawn(async move {
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 16384];
                let mut req = String::new();
                loop {
                    let Ok(n) = stream.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        break;
                    }
                    req.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if req.contains("\r\n\r\n") {
                        break;
                    }
                }
                seen_writer.lock().await.push(req);
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    // 4.4 post→success inserts comment into mirror exactly once;
    //     re-upsert of the same id (echo) is idempotent — no duplicate.
    #[tokio::test]
    async fn post_success_inserts_once_and_echo_is_idempotent() {
        let comment_resp = r#"{"id":"cmt1","comment_text":"hello","user":{},"date":1717000000000}"#;
        let (base, _seen) = spawn_mock(vec![(200, comment_resp.into())]).await;
        let client = ClickUpClient::with_base_url("pk_test", &base).unwrap();
        let conn = mirror_conn();

        let outcome = post_comment(&client, &conn, "task1", "hello", Some(1), None).await;
        assert_eq!(outcome, CommentOutcome::Posted { id: "cmt1".into() });

        // Comment is now in the mirror.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_comments WHERE id = 'cmt1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "comment must be in mirror after post");

        // Simulate the echo: the poll calls upsert_comment with the same id.
        // The row count must remain 1 (ON CONFLICT DO UPDATE is idempotent).
        let echo_comment = model::Comment {
            id: "cmt1".into(),
            comment_text: Some("hello".into()),
            user: None,
            resolved: Some(false),
            date: Some(1_717_000_000_000),
            reply_count: Some(0),
        };
        mirror::upsert_comment(&conn, "task1", &echo_comment).unwrap();
        let count_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_comments WHERE id = 'cmt1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count_after, 1, "echo upsert must not duplicate the row");
    }

    // 4.4 clear HTTP failure → Failed outcome, mirror NOT written.
    #[tokio::test]
    async fn post_http_failure_returns_failed_no_mirror_insert() {
        let (base, _seen) = spawn_mock(vec![(400, r#"{"err":"bad request"}"#.into())]).await;
        let client = ClickUpClient::with_base_url("pk_test", &base).unwrap();
        let conn = mirror_conn();

        let outcome = post_comment(&client, &conn, "task1", "hello", None, None).await;
        assert!(
            matches!(outcome, CommentOutcome::Failed { .. }),
            "HTTP 400 must return Failed, got: {outcome:?}"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_comments WHERE task_id = 'task1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "mirror must be empty after a failed post");
    }

    // 4.4 ambiguous failure (verify finds it landed) → idempotent upsert,
    //     no duplicate when the poll subsequently echoes the same id.
    #[tokio::test]
    async fn uncertain_verify_finds_landed_no_duplicate_on_echo() {
        // First request: simulate an uncertain outcome by using a 500 (HTTP
        // error is technically "Failed", but we test the verify path with a
        // known-good comment already on the server).  We use the verify path
        // directly here rather than producing a genuine network timeout (which
        // would require dropping the connection mid-flight and complicates the
        // test harness).  The contract: verify returns true → caller may call
        // upsert_comment → subsequent echo upsert stays at count=1.
        let now_ms: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let comments_resp = format!(
            r#"{{"comments":[{{"id":"cmt2","comment_text":"verify me","user":{{"id":42}},"date":{now_ms}}}]}}"#
        );
        let (base, _seen) = spawn_mock(vec![(200, comments_resp)]).await;
        let client = ClickUpClient::with_base_url("pk_test", &base).unwrap();

        let found = verify_comment_landed(&client, "task1", "verify me", Some(42), now_ms)
            .await
            .unwrap();
        assert!(found, "verify must find the landed comment");

        // Caller inserts after verify confirms.
        let conn = mirror_conn();
        let comment = model::Comment {
            id: "cmt2".into(),
            comment_text: Some("verify me".into()),
            user: Some(model::User {
                id: Some(42),
                ..Default::default()
            }),
            resolved: Some(false),
            date: Some(now_ms),
            reply_count: Some(0),
        };
        mirror::upsert_comment(&conn, "task1", &comment).unwrap();

        // Echo poll upserts the same comment — must remain at 1.
        mirror::upsert_comment(&conn, "task1", &comment).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_comments WHERE id = 'cmt2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "verify+upsert+echo must not duplicate");
    }

    // 4.4 verify returns false when no matching comment is found.
    #[tokio::test]
    async fn verify_returns_false_when_comment_not_found() {
        let comments_resp = r#"{"comments":[]}"#;
        let (base, _) = spawn_mock(vec![(200, comments_resp.into())]).await;
        let client = ClickUpClient::with_base_url("pk_test", &base).unwrap();
        let now_ms = 1_717_000_000_000i64;
        let found = verify_comment_landed(&client, "task1", "missing", Some(1), now_ms)
            .await
            .unwrap();
        assert!(!found);
    }

    // 4.3 verify timestamp tolerance: comments within ±5s still match.
    #[tokio::test]
    async fn verify_matches_within_timestamp_tolerance() {
        let base_ms = 1_717_000_000_000i64;
        let server_ms = base_ms + 3_000; // 3s later — within tolerance
        let comments_resp = format!(
            r#"{{"comments":[{{"id":"cmt3","comment_text":"tolerated","user":{{"id":7}},"date":{server_ms}}}]}}"#
        );
        let (base, _) = spawn_mock(vec![(200, comments_resp)]).await;
        let client = ClickUpClient::with_base_url("pk_test", &base).unwrap();
        let found = verify_comment_landed(&client, "task1", "tolerated", Some(7), base_ms)
            .await
            .unwrap();
        assert!(found, "comment within 5s tolerance must match");
    }

    // 4.3 verify rejects when outside tolerance.
    #[tokio::test]
    async fn verify_rejects_outside_timestamp_tolerance() {
        let base_ms = 1_717_000_000_000i64;
        let server_ms = base_ms + 10_000; // 10s later — outside tolerance
        let comments_resp = format!(
            r#"{{"comments":[{{"id":"cmt4","comment_text":"stale","user":{{"id":7}},"date":{server_ms}}}]}}"#
        );
        let (base, _) = spawn_mock(vec![(200, comments_resp)]).await;
        let client = ClickUpClient::with_base_url("pk_test", &base).unwrap();
        let found = verify_comment_landed(&client, "task1", "stale", Some(7), base_ms)
            .await
            .unwrap();
        assert!(!found, "comment outside 5s tolerance must not match");
    }
}
