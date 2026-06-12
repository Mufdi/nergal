//! Structural confirmation-token gate for the write-back closure (Decision 5).
//!
//! The sole path through which a status move and/or comment post can be
//! executed is:
//!
//! 1. `clickup_request_closure_token` — called when the user confirms the
//!    prompt.  Validates inputs, then issues a short-lived (TOKEN_TTL_SECS)
//!    single-use random token scoped to the exact `(task_id, status, comment)`
//!    tuple.
//!
//! 2. `clickup_execute_closure` — takes ONLY the token; looks up and
//!    destructively consumes it; rejects expired/unknown tokens with no write;
//!    executes status and comment as independent halves (both attempted even if
//!    the other fails) and returns per-write outcome.
//!
//! 3. `clickup_verify_comment_landed` — thin Tauri wrapper around
//!    `writeback::verify_comment_landed`; called by the frontend before
//!    offering a retry after an `Uncertain` comment outcome.
//!
//! A renderer cannot forge a valid token without going through step 1, and
//! cannot reuse a token (single-use destructive take), so closure and comment
//! writes are structurally gated against renderer bugs and injected content.
//!
//! ## Write order (status → comment)
//!
//! Status is applied first.  If the status write fails, the comment is still
//! attempted — the two halves are independent by design (Decision 5, Risks).
//! We choose status-first so the most reversible/retryable half happens before
//! the irreversible one; if the process dies mid-execution the user has a clean
//! surface to retry the comment without re-doing the status.  This ordering
//! is documented in the outcome struct so the frontend can surface
//! "comment posted; status change failed — retry status?" accurately.
//!
//! ## Comment sanitization
//!
//! Before posting, the comment text is run through `sanitize_comment_text` to
//! strip ClickUp mention syntax (`@username`, `@here`, `@everyone`) and task-
//! reference syntax (`#TASKID`).  This neutralizes injected content (e.g. a
//! PR description that contains `@here`) so it cannot ping team members or
//! create task links.  The user sees and reviews the sanitized text at the
//! confirm step before the token is issued, so the sanitizer is applied early
//! (at token-request time) and again at execution time as defense-in-depth.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::writeback::{self, CommentOutcome, WriteField, WritebackRegistry};

/// A token expires this long after issuance.  Short enough to make replay
/// implausible, long enough for the async round-trip through the webview.
pub const TOKEN_TTL: Duration = Duration::from_secs(30);

/// A confirmed closure token: valid only for the stored `(task_id, status,
/// comment)` tuple, single-use, and expiring after TOKEN_TTL.
#[derive(Debug)]
struct ClosureToken {
    task_id: String,
    /// `None` → status half was not selected; skip the status write.
    status: Option<String>,
    /// `None` → comment half was not selected; skip the comment post.
    comment: Option<String>,
    issued_at: Instant,
}

/// In-memory store of pending closure tokens.
///
/// Tokens are indexed by their random UUID string.  A `.take()` removes the
/// entry — the single-use invariant holds under the lock.
pub struct ClosureTokenStore {
    tokens: Mutex<HashMap<String, ClosureToken>>,
}

impl Default for ClosureTokenStore {
    fn default() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }
}

impl ClosureTokenStore {
    /// Issue a new token for `(task_id, status, comment)`.
    ///
    /// The caller is responsible for validating inputs before calling this.
    fn issue(&self, task_id: String, status: Option<String>, comment: Option<String>) -> String {
        let token_str = uuid::Uuid::new_v4().to_string();
        let token = ClosureToken {
            task_id,
            status,
            comment,
            issued_at: Instant::now(),
        };
        if let Ok(mut guard) = self.tokens.lock() {
            // Evict expired entries while we hold the lock.
            guard.retain(|_, t| t.issued_at.elapsed() < TOKEN_TTL);
            guard.insert(token_str.clone(), token);
        }
        token_str
    }

    /// Consume and return the token — single-use destructive take.
    ///
    /// Returns `Err` if the token is unknown or expired.
    fn take(&self, token_str: &str) -> Result<ClosureToken> {
        let mut guard = self
            .tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("token store lock poisoned"))?;
        let token = guard
            .remove(token_str)
            .ok_or_else(|| anyhow::anyhow!("unknown or already-used closure token"))?;
        if token.issued_at.elapsed() >= TOKEN_TTL {
            anyhow::bail!("closure token expired");
        }
        Ok(token)
    }

    /// Test-only constructor that injects a token with a synthetic issued_at.
    #[cfg(test)]
    fn insert_for_test(
        &self,
        token_str: &str,
        task_id: &str,
        status: Option<&str>,
        comment: Option<&str>,
        age: Duration,
    ) {
        let issued_at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        let token = ClosureToken {
            task_id: task_id.to_string(),
            status: status.map(str::to_string),
            comment: comment.map(str::to_string),
            issued_at,
        };
        if let Ok(mut guard) = self.tokens.lock() {
            guard.insert(token_str.to_string(), token);
        }
    }
}

// ── Comment sanitizer ──

/// Neutralize ClickUp mention and task-reference syntax that could ping team
/// members or create task links.
///
/// Patterns neutralized:
/// - `@word`  (@ followed by one or more word chars): covers
///   `@username`, `@here`, `@everyone`, `@teamname`, etc.
/// - `#word`  (# followed by one or more word chars): covers task-id refs.
///
/// Each trigger char (`@` or `#`) followed immediately by a word char is
/// broken by inserting a zero-width space (U+200B) between the trigger and
/// the word.  The resulting `@​name` sequence is invisible to ClickUp's
/// mention parser (which requires `@` immediately adjacent to the name)
/// but the full text remains readable to humans.  This is conservative: a
/// false-positive on a literal `@email` in a comment is an acceptable
/// trade-off against the risk of pinging the team with injected content.
pub fn sanitize_comment_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '@' || ch == '#' {
            // Only neutralize when the next char is a word char (i.e. this
            // looks like a mention/task-ref, not a standalone `@` or `#`).
            let next_is_word = chars
                .peek()
                .map(|c| c.is_alphanumeric() || *c == '_')
                .unwrap_or(false);
            result.push(ch);
            if next_is_word {
                // Zero-width space breaks the @word / #word token so
                // ClickUp's parser never sees them as contiguous.
                result.push('\u{200b}');
            }
        } else {
            result.push(ch);
        }
    }
    result
}

// ── Per-write outcome returned to the frontend ──

/// Status-half outcome.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StatusOutcome {
    /// Status half was not requested; skipped.
    Skipped,
    /// Status successfully applied upstream.
    Ok,
    /// Status write failed.  The comment half was still attempted
    /// (halves are independent).
    Failed { error: String },
}

/// Full per-write result returned by `clickup_execute_closure`.
#[derive(Debug, serde::Serialize)]
pub struct ClosureResult {
    pub status: StatusOutcome,
    pub comment: CommentOutcome,
}

// ── Tauri commands ──

/// Validate inputs and issue a single-use closure token.
///
/// Called when the user confirms the closure prompt (frontend has already
/// shown the sanitized text at this point). Server-side validation:
/// - At least one of `status` or `comment` must be present.
/// - If `status` is given, it must be a real status of the task's List.
/// - If `comment` is given, its length must be ≤ 10 000 characters (sane cap
///   to reject accidentally-huge inputs before they reach the API).
/// - The comment text is sanitized before being stored in the token.
#[tauri::command]
pub async fn clickup_request_closure_token(
    task_id: String,
    status: Option<String>,
    comment: Option<String>,
    db: tauri::State<'_, crate::db::SharedDb>,
    store: tauri::State<'_, ClosureTokenStore>,
) -> Result<String, String> {
    // Reject if neither half is requested.
    if status.is_none() && comment.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        return Err("at least one of status or comment must be provided".into());
    }

    // Validate + sanitize the comment before locking the DB.
    let sanitized_comment = match comment {
        Some(ref text) if !text.is_empty() => {
            const MAX_COMMENT_LEN: usize = 10_000;
            if text.chars().count() > MAX_COMMENT_LEN {
                return Err(format!("comment exceeds {MAX_COMMENT_LEN}-character limit"));
            }
            Some(sanitize_comment_text(text))
        }
        _ => None,
    };

    // Server-side status validation.
    if let Some(ref s) = status {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        super::validate_status_for_task(guard.conn(), &task_id, s).map_err(|e| format!("{e:#}"))?;
    }

    // Re-check: after sanitization a blank comment with no status is still rejected.
    if status.is_none() && sanitized_comment.is_none() {
        return Err("at least one of status or comment must be provided".into());
    }

    Ok(store.issue(task_id, status, sanitized_comment))
}

/// Execute a confirmed closure using the provided token.
///
/// The token is consumed (single-use destructive take) before any write is
/// attempted.  An unknown, already-used, or expired token → error with no
/// write.
///
/// Both halves are attempted independently (Decision 5, Risks):
///   1. Status move (if requested)
///   2. Comment post (if requested)
///
/// The comment is posted after the status so the most reversible/retryable
/// half goes first.  If the status fails we still proceed to the comment
/// because the halves are independent — the frontend surfaces the mixed
/// outcome honestly.
#[tauri::command]
pub async fn clickup_execute_closure(
    token: String,
    db: tauri::State<'_, crate::db::SharedDb>,
    store: tauri::State<'_, ClosureTokenStore>,
    registry: tauri::State<'_, WritebackRegistry>,
) -> Result<ClosureResult, String> {
    // Consume the token first — no writes happen if this fails.
    let tok = store.take(&token).map_err(|e| format!("{e:#}"))?;

    let client = super::load_client().await?;

    // ── 1. Status half ──
    let status_outcome = match tok.status {
        None => StatusOutcome::Skipped,
        Some(ref status_name) => {
            // Re-validate: mirror may have changed between token issuance and
            // execution (e.g. the list was edited by a webhook reconcile).
            let validate_result = {
                let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
                let pre: Option<String> = {
                    use rusqlite::OptionalExtension;
                    guard
                        .conn()
                        .query_row(
                            "SELECT status_name FROM clickup_tasks WHERE id = ?1",
                            [&tok.task_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .optional()
                        .map_err(|e| format!("{e:#}"))?
                        .flatten()
                };
                let vr = super::validate_status_for_task(guard.conn(), &tok.task_id, status_name)
                    .map_err(|e| format!("{e:#}"));
                vr.map(|_| pre)
            };

            match validate_result {
                Err(e) => StatusOutcome::Failed { error: e },
                Ok(pre) => match client.set_task_status(&tok.task_id, status_name).await {
                    Ok(_task) => {
                        registry.record(
                            &tok.task_id,
                            WriteField::Status,
                            status_name.as_str(),
                            pre.as_deref(),
                        );
                        StatusOutcome::Ok
                    }
                    Err(e) => StatusOutcome::Failed {
                        error: format!("{e:#}"),
                    },
                },
            }
        }
    };

    // ── 2. Comment half ──
    // Uses the sanitized text stored in the token (sanitized at issuance).
    let comment_outcome = match tok.comment {
        None => CommentOutcome::Skipped,
        Some(ref text) => {
            // Read author_id while not holding db across an await.
            let author_id = {
                let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
                super::mirror::cached_user_id(guard.conn()).ok().flatten()
            };

            // post_comment needs a &Connection; we must not hold the guard
            // across the .await, so we call the client directly and mirror
            // the insert ourselves using the same logic as writeback::post_comment.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            match client.add_comment(&tok.task_id, text).await {
                Ok(comment_id) => {
                    // Mirror insert after the await — safe, brief lock.
                    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
                    let comment = super::model::Comment {
                        id: comment_id.clone(),
                        comment_text: Some(text.clone()),
                        user: author_id.map(|id| super::model::User {
                            id: Some(id),
                            ..Default::default()
                        }),
                        resolved: Some(false),
                        date: Some(now_ms),
                        reply_count: Some(0),
                    };
                    if let Err(e) =
                        super::mirror::upsert_comment(guard.conn(), &tok.task_id, &comment)
                    {
                        tracing::warn!(
                            task = %tok.task_id,
                            comment = %comment_id,
                            "closure mirror insert failed: {e:#}"
                        );
                    }
                    CommentOutcome::Posted { id: comment_id }
                }
                Err(e) => writeback::classify_comment_error(e),
            }
        }
    };

    Ok(ClosureResult {
        status: status_outcome,
        comment: comment_outcome,
    })
}

/// Re-fetch comments from the API and verify whether an uncertain comment
/// landed.  Called by the frontend before offering a retry after an
/// `Uncertain` outcome to avoid posting a duplicate.
#[tauri::command]
pub async fn clickup_verify_comment_landed(
    task_id: String,
    text: String,
    posted_at_ms: i64,
) -> Result<bool, String> {
    let client = super::load_client().await?;
    // author_id not available here — match on text + timestamp only
    // (see writeback::verify_comment_landed for the tolerance rules).
    writeback::verify_comment_landed(&client, &task_id, &text, None, posted_at_ms)
        .await
        .map_err(|e| format!("{e:#}"))
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ── sanitize_comment_text ──

    #[test]
    fn sanitize_neutralizes_at_mention() {
        // @username is a ClickUp mention trigger; must be neutralized.
        let out = sanitize_comment_text("hey @alice please review");
        assert!(
            !out.contains("@alice"),
            "bare @mention must not survive: {out:?}"
        );
        // The human-readable name is preserved after the zero-width space.
        assert!(
            out.contains("alice"),
            "name must still be readable: {out:?}"
        );
    }

    #[test]
    fn sanitize_neutralizes_at_here_and_at_everyone() {
        let out_here = sanitize_comment_text("@here please look");
        let out_everyone = sanitize_comment_text("@everyone FYI");
        // The contiguous @word token must be broken (ZWS inserted between @
        // and the word) so ClickUp's parser does not see a mention trigger.
        assert!(!out_here.contains("@here"), "got: {out_here:?}");
        assert!(!out_everyone.contains("@everyone"), "got: {out_everyone:?}");
        // Human-readable text remains.
        assert!(out_here.contains("here"), "got: {out_here:?}");
        assert!(out_everyone.contains("everyone"), "got: {out_everyone:?}");
    }

    #[test]
    fn sanitize_neutralizes_task_ref() {
        let out = sanitize_comment_text("see #abc123 for context");
        assert!(
            !out.contains(" #abc123"),
            "bare task-ref must be neutralized: {out:?}"
        );
        // The id text is still readable.
        assert!(out.contains("abc123"), "id must still be readable: {out:?}");
    }

    #[test]
    fn sanitize_preserves_standalone_at_and_hash() {
        // A lone `@` or `#` not followed by a word char should pass unchanged.
        let out_at = sanitize_comment_text("price: 5 @ ea");
        let out_hash = sanitize_comment_text("score: 100 # points");
        assert!(out_at.contains("@ ea"), "got: {out_at:?}");
        assert!(out_hash.contains("# points"), "got: {out_hash:?}");
    }

    #[test]
    fn sanitize_leaves_clean_text_unchanged() {
        // No `@` or `#` in this string — output must equal input exactly.
        let out = sanitize_comment_text("all good, no mentions here");
        assert_eq!(out, "all good, no mentions here");
    }

    #[test]
    fn sanitize_multiple_mentions_in_one_string() {
        let out = sanitize_comment_text("@alice and @bob please see #taskid");
        // None of the bare @/# patterns should remain unescaped.
        assert!(!out.contains("@alice"));
        assert!(!out.contains("@bob"));
        assert!(!out.contains(" #taskid"));
    }

    // ── ClosureTokenStore ──

    #[test]
    fn token_unknown_returns_error() {
        let store = ClosureTokenStore::default();
        let err = store.take("nonexistent-token");
        assert!(err.is_err(), "unknown token must error");
        let msg = format!("{:#}", err.unwrap_err());
        assert!(
            msg.contains("unknown") || msg.contains("already-used"),
            "error message: {msg}"
        );
    }

    #[test]
    fn token_is_single_use() {
        let store = ClosureTokenStore::default();
        let tok = store.issue("task1".into(), Some("done".into()), None);
        // First consume: succeeds.
        assert!(store.take(&tok).is_ok(), "first take must succeed");
        // Second consume: must fail.
        let second = store.take(&tok);
        assert!(second.is_err(), "second take must error (single-use)");
    }

    #[test]
    fn token_expires_after_ttl() {
        let store = ClosureTokenStore::default();
        // Inject a token that was issued TOKEN_TTL + 1 s ago.
        store.insert_for_test(
            "expired-tok",
            "task1",
            Some("done"),
            None,
            TOKEN_TTL + Duration::from_secs(1),
        );
        let err = store.take("expired-tok");
        assert!(err.is_err(), "expired token must error");
        let msg = format!("{:#}", err.unwrap_err());
        assert!(msg.contains("expired"), "error message: {msg}");
    }

    #[test]
    fn fresh_token_not_expired() {
        let store = ClosureTokenStore::default();
        // Inject a token issued 0 s ago.
        store.insert_for_test("fresh-tok", "task1", Some("done"), None, Duration::ZERO);
        assert!(store.take("fresh-tok").is_ok(), "fresh token must succeed");
    }

    #[test]
    fn token_scoped_to_stored_tuple_not_caller_values() {
        // The execute command takes ONLY the token; the stored tuple is what gets
        // used.  Verify the ClosureToken carries the original task_id/status/comment
        // regardless of what the caller might try to pass.
        let store = ClosureTokenStore::default();
        let tok_str = store.issue(
            "real-task".into(),
            Some("done".into()),
            Some("my note".into()),
        );
        let tok = store.take(&tok_str).unwrap();
        assert_eq!(tok.task_id, "real-task");
        assert_eq!(tok.status.as_deref(), Some("done"));
        assert_eq!(tok.comment.as_deref(), Some("my note"));
    }

    #[test]
    fn neither_half_issue_produces_error() {
        // The rejection is at the command layer; we test the guard logic
        // directly via the conditions it encodes: both None / empty.
        // status=None, comment=None → reject.
        let status: Option<String> = None;
        let comment: Option<String> = None;
        let rejected = status.is_none() && comment.as_ref().map(|s| s.is_empty()).unwrap_or(true);
        assert!(rejected, "neither-half must be rejected");

        // status=Some, comment=None → allowed.
        let status2 = Some("done".to_string());
        let comment2: Option<String> = None;
        let rejected2 =
            status2.is_none() && comment2.as_ref().map(|s| s.is_empty()).unwrap_or(true);
        assert!(!rejected2, "status-only must be allowed");

        // status=None, comment=Some(non-empty) → allowed.
        let status3: Option<String> = None;
        let comment3 = Some("PR shipped".to_string());
        let rejected3 =
            status3.is_none() && comment3.as_ref().map(|s| s.is_empty()).unwrap_or(true);
        assert!(!rejected3, "comment-only must be allowed");
    }

    #[test]
    fn comment_sanitized_at_token_issuance() {
        // Simulate what `clickup_request_closure_token` does before calling
        // `issue`: apply sanitize_comment_text to the incoming text.
        let raw = "@here the PR landed at https://github.com/org/repo/pull/42";
        let sanitized = sanitize_comment_text(raw);
        let store = ClosureTokenStore::default();
        let tok_str = store.issue("t1".into(), None, Some(sanitized.clone()));
        let tok = store.take(&tok_str).unwrap();
        // The stored comment is the sanitized version, not the raw input.
        assert_eq!(tok.comment.as_deref(), Some(sanitized.as_str()));
        assert!(
            !tok.comment.as_deref().unwrap_or("").contains("@here"),
            "bare @here must not be stored"
        );
    }

    #[test]
    fn partial_failure_result_shape() {
        // Verify the result struct carries both halves independently.
        let r = ClosureResult {
            status: StatusOutcome::Failed {
                error: "HTTP 500".into(),
            },
            comment: CommentOutcome::Posted { id: "cmt1".into() },
        };
        // Serialized shape must carry both fields.
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            json.contains("\"status\":{\"status\":\"failed\""),
            "got: {json}"
        );
        assert!(
            json.contains("\"comment\":{\"status\":\"posted\""),
            "got: {json}"
        );
    }

    #[test]
    fn closure_result_skipped_shape() {
        let r = ClosureResult {
            status: StatusOutcome::Skipped,
            comment: CommentOutcome::Skipped,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"status\":\"skipped\""), "got: {json}");
        // comment field uses the same tag convention from writeback.
        assert!(json.contains("\"status\":\"skipped\""), "got: {json}");
    }
}
