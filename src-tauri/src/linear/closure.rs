//! Structural confirmation-token gate for Linear write-back closure
//! (Decision 5 — linear-writeback change #3).
//!
//! The sole paths through which a state move and/or comment post can be
//! executed are:
//!
//! 1. `linear_request_comment_token` — issued by the detail composer when the
//!    user wants to post a comment WITHOUT closing out the issue (`close_out =
//!    false`).  Validates the comment; issues a token scoped to the
//!    `(issue_id, comment)` tuple.
//!
//! 2. `linear_request_closure_token` — issued by the closure prompt when the
//!    user confirms a closure (`close_out = true`).  Validates the optional
//!    state against the issue's non-synthetic team states; sanitizes the
//!    optional comment at issuance; issues a token scoped to the
//!    `(issue_id, state?, comment?, close_out=true)` tuple.
//!
//! 3. `linear_execute_gated_write` — takes ONLY the token; looks it up and
//!    destructively consumes it; rejects expired/unknown tokens with no write;
//!    executes the two halves (state → comment, independent) and, when
//!    `close_out=true`, the local closure (unbind + durable marker) regardless
//!    of the Linear outcomes.  Returns `GatedWriteResult`.
//!
//! 4. `linear_verify_comment_landed` — thin Tauri wrapper around
//!    `writeback::verify_comment_landed`; called by the frontend before
//!    offering a retry after an `Uncertain` comment outcome.
//!
//! A renderer cannot forge a valid token without going through step 1 or 2,
//! and cannot reuse a token (single-use destructive `take`), so closure and
//! comment writes are structurally gated against renderer bugs.
//!
//! ## Write order (Decision 6)
//!
//! State is applied first (reversible/retryable half).  Comment is posted
//! second (irreversible).  Local closure (unbind + durable marker) runs third
//! and ONLY when `close_out=true`, applied regardless of the Linear outcomes.
//!
//! A partial-failure retry surface ("state failed — retry state?") carries the
//! `issue_id` explicitly because the binding is already unbound by the time
//! the comment posted (and for a ship-triggered closure the session keeps
//! running — the retry cannot re-derive the issue from the binding).
//!
//! ## Token discriminant `close_out: bool` (review N1)
//!
//! A plain comment post (`close_out=false`) and a full closure (`close_out=true`)
//! are intentionally TWO request commands that produce tokens with different
//! `close_out` values.  Collapsing them would make "post a quick comment"
//! silently unbind the session and stamp it worked-and-closed.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::writeback::{self, CommentOutcome, WriteField, WritebackRegistry};

/// A token expires this long after issuance.  Short enough to make replay
/// implausible; long enough for the async round-trip through the webview.
pub const TOKEN_TTL: Duration = Duration::from_secs(30);

/// A confirmed gated-write token.  Scoped to `(issue_id, state_id?, comment?,
/// close_out)`.  Single-use (destructive `take`) and short-lived (TOKEN_TTL).
#[derive(Debug)]
struct GatedToken {
    issue_id: String,
    /// `None` → state half not selected; skip the state write.
    state_id: Option<String>,
    /// `None` → comment half not selected; skip the comment post.
    comment: Option<String>,
    /// When `true`, perform the local close-out (unbind + durable marker)
    /// regardless of the Linear write outcomes.
    close_out: bool,
    issued_at: Instant,
}

/// In-memory store of pending gated-write tokens.
///
/// Tokens are indexed by their random UUID string.  A `.take()` removes the
/// entry — the single-use invariant holds under the lock.
pub struct GatedTokenStore {
    tokens: Mutex<HashMap<String, GatedToken>>,
}

impl Default for GatedTokenStore {
    fn default() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }
}

impl GatedTokenStore {
    /// Issue a new token.  The caller is responsible for validating inputs.
    fn issue(
        &self,
        issue_id: String,
        state_id: Option<String>,
        comment: Option<String>,
        close_out: bool,
    ) -> String {
        let token_str = uuid::Uuid::new_v4().to_string();
        let token = GatedToken {
            issue_id,
            state_id,
            comment,
            close_out,
            issued_at: Instant::now(),
        };
        if let Ok(mut guard) = self.tokens.lock() {
            // Evict expired entries while we hold the lock — cheap bounded cleanup.
            guard.retain(|_, t| t.issued_at.elapsed() < TOKEN_TTL);
            guard.insert(token_str.clone(), token);
        }
        token_str
    }

    /// Consume and return the token — single-use destructive take.
    ///
    /// Returns `Err` if the token is unknown or expired.
    fn take(&self, token_str: &str) -> Result<GatedToken> {
        let mut guard = self
            .tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("token store lock poisoned"))?;
        let token = guard
            .remove(token_str)
            .ok_or_else(|| anyhow::anyhow!("unknown or already-used gated-write token"))?;
        if token.issued_at.elapsed() >= TOKEN_TTL {
            anyhow::bail!("gated-write token expired");
        }
        Ok(token)
    }

    /// Test-only constructor that injects a token with a synthetic `issued_at`.
    #[cfg(test)]
    fn insert_for_test(
        &self,
        token_str: &str,
        issue_id: &str,
        state_id: Option<&str>,
        comment: Option<&str>,
        close_out: bool,
        age: Duration,
    ) {
        let issued_at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        let token = GatedToken {
            issue_id: issue_id.to_string(),
            state_id: state_id.map(str::to_string),
            comment: comment.map(str::to_string),
            close_out,
            issued_at,
        };
        if let Ok(mut guard) = self.tokens.lock() {
            guard.insert(token_str.to_string(), token);
        }
    }
}

// ── Comment sanitizer ──

/// Neutralize Linear mention syntax that could ping team members.
///
/// Patterns neutralized:
/// - `@word` (`@` followed by a word char): covers `@username`, `@team`, etc.
///
/// Each `@` followed immediately by a word char is broken by inserting a
/// zero-width space (U+200B) between the `@` and the word.  The resulting
/// sequence is invisible to Linear's mention parser (which requires `@`
/// immediately adjacent to the name) but human-readable.  This is the same
/// approach used in `clickup/closure.rs::sanitize_comment_text`.
pub fn sanitize_comment_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        result.push(ch);
        if ch == '@' {
            // Only neutralize when the next char is a word char.
            if chars
                .peek()
                .map(|c| c.is_alphanumeric() || *c == '_')
                .unwrap_or(false)
            {
                result.push('\u{200b}');
            }
        }
    }
    result
}

// ── Per-write outcomes ──

/// State-half outcome.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StateOutcome {
    /// State half was not requested; skipped.
    Skipped,
    /// State successfully applied upstream.
    Ok,
    /// State write failed.  The comment half was still attempted.
    Failed { error: String },
}

/// Full per-write result returned by `linear_execute_gated_write`.
#[derive(Debug, serde::Serialize)]
pub struct GatedWriteResult {
    pub state: StateOutcome,
    pub comment: CommentOutcome,
    /// Whether the local close-out (unbind + durable marker) was applied.
    pub closed_out: bool,
}

// ── Tauri commands ──

/// Issue a gated-write token for posting a comment WITHOUT closing out the
/// issue (the detail composer path).
///
/// `close_out = false` in the issued token: `execute_gated_write` will post
/// the comment and nothing else — no unbind, no closed-out marker.
#[tauri::command]
pub async fn linear_request_comment_token(
    issue_id: String,
    comment: String,
    store: tauri::State<'_, GatedTokenStore>,
) -> Result<String, String> {
    if comment.trim().is_empty() {
        return Err("comment must not be empty".into());
    }
    const MAX_COMMENT_LEN: usize = 10_000;
    if comment.chars().count() > MAX_COMMENT_LEN {
        return Err(format!("comment exceeds {MAX_COMMENT_LEN}-character limit"));
    }
    let sanitized = sanitize_comment_text(&comment);
    Ok(store.issue(issue_id, None, Some(sanitized), false))
}

/// Issue a gated-write token for the closure prompt.
///
/// `close_out = true` in the issued token: `execute_gated_write` will apply
/// the state (if present) + post the comment (if present) + perform the local
/// close-out (unbind + durable marker) regardless of the Linear outcomes.
///
/// Both `state_id` and `comment` are optional: selecting neither still closes
/// out locally when `execute_gated_write` is called.
#[tauri::command]
pub async fn linear_request_closure_token(
    issue_id: String,
    state_id: Option<String>,
    comment: Option<String>,
    db: tauri::State<'_, crate::db::SharedDb>,
    store: tauri::State<'_, GatedTokenStore>,
) -> Result<String, String> {
    // Validate + sanitize the comment before touching the DB.
    let sanitized_comment = match comment {
        Some(ref text) if !text.trim().is_empty() => {
            const MAX_COMMENT_LEN: usize = 10_000;
            if text.chars().count() > MAX_COMMENT_LEN {
                return Err(format!("comment exceeds {MAX_COMMENT_LEN}-character limit"));
            }
            Some(sanitize_comment_text(text))
        }
        _ => None,
    };

    // Server-side state validation (non-synthetic team membership).
    if let Some(ref sid) = state_id {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let team_id = super::mirror::get_issue_team_id(guard.conn(), &issue_id)
            .map_err(|e| format!("{e:#}"))?;
        super::mirror::validate_state_for_team(guard.conn(), &team_id, sid)
            .map_err(|e| format!("{e:#}"))?;
    }

    Ok(store.issue(issue_id, state_id, sanitized_comment, true))
}

/// Execute a confirmed gated write using the provided token.
///
/// The token is consumed (single-use destructive take) before any write is
/// attempted.  An unknown, already-used, or expired token returns an error
/// with no write performed.
///
/// Write order (Decision 6):
///   1. State write (reversible/retryable half).
///   2. Comment post (irreversible half).
///   3. Local close-out (unbind + durable marker) — ONLY when `close_out=true`,
///      applied regardless of the Linear outcomes.
///
/// A `close_out=false` token (detail composer) executes only the comment and
/// nothing else.  A `close_out=true` token with neither half still closes out
/// locally.
#[tauri::command]
pub async fn linear_execute_gated_write(
    token: String,
    session_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
    store: tauri::State<'_, GatedTokenStore>,
    registry: tauri::State<'_, WritebackRegistry>,
) -> Result<GatedWriteResult, String> {
    // Consume the token first — no writes happen if this fails.
    let tok = store.take(&token).map_err(|e| format!("{e:#}"))?;

    let stored = super::load_active_stored_key(&db).await?;
    let client = super::build_client(stored.key);

    // ── 1. State half ──
    let state_outcome = match tok.state_id {
        None => StateOutcome::Skipped,
        Some(ref state_id) => {
            // Re-validate: mirror may have changed between token issuance and
            // execution (e.g. a workspace switch triggered a reconcile).
            let validate_result = {
                let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
                let team_id = super::mirror::get_issue_team_id(guard.conn(), &tok.issue_id)
                    .map_err(|e| format!("{e:#}"));
                let pre: Option<String> = {
                    use rusqlite::OptionalExtension;
                    guard
                        .conn()
                        .query_row(
                            "SELECT state_id FROM linear_issues WHERE id = ?1",
                            [&tok.issue_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .optional()
                        .map_err(|e| format!("{e:#}"))?
                        .flatten()
                };
                team_id.and_then(|tid| {
                    super::mirror::validate_state_for_team(guard.conn(), &tid, state_id)
                        .map_err(|e| format!("{e:#}"))
                        .map(|_| pre)
                })
            };

            match validate_result {
                Err(e) => StateOutcome::Failed { error: e },
                Ok(pre) => {
                    // Record BEFORE the API call (provisional, TOCTOU-safe).
                    registry.record(
                        &tok.issue_id,
                        WriteField::State,
                        state_id.as_str(),
                        pre.as_deref(),
                    );
                    let input = super::client::IssueUpdateInput {
                        state_id: Some(state_id.clone()),
                        assignee_id: None,
                        cycle_id: None,
                    };
                    match client.issue_update(&tok.issue_id, input).await {
                        Ok(_) => StateOutcome::Ok,
                        Err(e) => {
                            // Clear provisional record on failure.
                            registry.clear_entry(&tok.issue_id, &WriteField::State);
                            StateOutcome::Failed {
                                error: format!("{e:#}"),
                            }
                        }
                    }
                }
            }
        }
    };

    // ── 2. Comment half ──
    let comment_outcome = match tok.comment {
        None => CommentOutcome::Skipped,
        Some(ref text) => {
            let author_id = {
                let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
                super::mirror::get_sync_state(guard.conn())
                    .ok()
                    .and_then(|s| s.viewer_id)
            };

            // post_comment needs a &Connection; we must not hold the DB guard
            // across the .await, so we call the client directly and mirror the
            // insert ourselves (same pattern as clickup/closure.rs).
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            match client.comment_create(&tok.issue_id, text).await {
                Ok(comment_id) => {
                    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
                    if let Err(e) = super::mirror::upsert_comment(
                        guard.conn(),
                        &tok.issue_id,
                        &comment_id,
                        author_id.as_deref(),
                        text,
                        now_secs,
                    ) {
                        tracing::warn!(
                            issue = %tok.issue_id,
                            comment = %comment_id,
                            "linear closure comment mirror insert failed: {e:#}"
                        );
                    }
                    CommentOutcome::Posted { id: comment_id }
                }
                Err(e) => writeback::classify_comment_error(e),
            }
        }
    };

    // ── 3. Local close-out (only when close_out=true) ──
    let closed_out = if tok.close_out {
        // Unbind the issue from the session regardless of Linear outcomes.
        if let Err(e) = {
            let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
            guard.set_active_linear_issue(&session_id, None)
        } {
            tracing::warn!(session = %session_id, "linear closure unbind failed: {e:#}");
        }
        // Durable worked-closed marker.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Err(e) = {
            let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
            super::mirror::mark_closed_out(guard.conn(), &tok.issue_id, now_secs)
        } {
            tracing::warn!(issue = %tok.issue_id, "linear mark_closed_out failed: {e:#}");
        }
        true
    } else {
        false
    };

    Ok(GatedWriteResult {
        state: state_outcome,
        comment: comment_outcome,
        closed_out,
    })
}

/// Re-fetch comments from the API and verify whether an uncertain comment
/// landed.  Called by the frontend before offering a retry after an
/// `Uncertain` outcome to avoid posting a duplicate.
#[tauri::command]
pub async fn linear_verify_comment_landed(
    issue_id: String,
    body: String,
    posted_at_secs: i64,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<bool, String> {
    let stored = super::load_active_stored_key(&db).await?;
    let client = super::build_client(stored.key);
    let author_id = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        super::mirror::get_sync_state(guard.conn())
            .ok()
            .and_then(|s| s.viewer_id)
    };
    writeback::verify_comment_landed(
        &client,
        &issue_id,
        &body,
        author_id.as_deref(),
        posted_at_secs,
    )
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
        let out = sanitize_comment_text("hey @alice please review");
        assert!(
            !out.contains("@alice"),
            "bare @mention must not survive: {out:?}"
        );
        assert!(
            out.contains("alice"),
            "name must still be readable: {out:?}"
        );
    }

    #[test]
    fn sanitize_neutralizes_at_here_and_at_everyone() {
        let out_here = sanitize_comment_text("@here please look");
        let out_everyone = sanitize_comment_text("@everyone FYI");
        assert!(!out_here.contains("@here"), "got: {out_here:?}");
        assert!(!out_everyone.contains("@everyone"), "got: {out_everyone:?}");
        assert!(out_here.contains("here"), "got: {out_here:?}");
        assert!(out_everyone.contains("everyone"), "got: {out_everyone:?}");
    }

    #[test]
    fn sanitize_preserves_standalone_at() {
        let out = sanitize_comment_text("price: 5 @ ea");
        assert!(out.contains("@ ea"), "got: {out:?}");
    }

    #[test]
    fn sanitize_leaves_clean_text_unchanged() {
        let out = sanitize_comment_text("all good, no mentions here");
        assert_eq!(out, "all good, no mentions here");
    }

    #[test]
    fn sanitize_multiple_mentions() {
        let out = sanitize_comment_text("@alice and @bob please see this");
        assert!(!out.contains("@alice"));
        assert!(!out.contains("@bob"));
    }

    // ── GatedTokenStore ──

    #[test]
    fn token_unknown_returns_error() {
        let store = GatedTokenStore::default();
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
        let store = GatedTokenStore::default();
        let tok = store.issue("i1".into(), Some("s1".into()), None, true);
        assert!(store.take(&tok).is_ok(), "first take must succeed");
        let second = store.take(&tok);
        assert!(second.is_err(), "second take must error (single-use)");
    }

    #[test]
    fn token_expires_after_ttl() {
        let store = GatedTokenStore::default();
        store.insert_for_test(
            "expired-tok",
            "i1",
            Some("s1"),
            None,
            true,
            TOKEN_TTL + Duration::from_secs(1),
        );
        let err = store.take("expired-tok");
        assert!(err.is_err(), "expired token must error");
        let msg = format!("{:#}", err.unwrap_err());
        assert!(msg.contains("expired"), "error message: {msg}");
    }

    #[test]
    fn fresh_token_not_expired() {
        let store = GatedTokenStore::default();
        store.insert_for_test("fresh-tok", "i1", None, None, true, Duration::ZERO);
        assert!(store.take("fresh-tok").is_ok(), "fresh token must succeed");
    }

    // 5.4 Token is scoped to its stored tuple.
    #[test]
    fn token_scoped_to_stored_tuple() {
        let store = GatedTokenStore::default();
        let tok_str = store.issue(
            "real-issue".into(),
            Some("s1".into()),
            Some("my note".into()),
            true,
        );
        let tok = store.take(&tok_str).unwrap();
        assert_eq!(tok.issue_id, "real-issue");
        assert_eq!(tok.state_id.as_deref(), Some("s1"));
        assert_eq!(tok.comment.as_deref(), Some("my note"));
        assert!(tok.close_out);
    }

    // 5.4 close_out=false token does NOT close out (discriminant is preserved).
    #[test]
    fn comment_only_token_has_close_out_false() {
        let store = GatedTokenStore::default();
        let tok_str = store.issue("i1".into(), None, Some("a comment".into()), false);
        let tok = store.take(&tok_str).unwrap();
        assert!(
            !tok.close_out,
            "comment-only token must have close_out=false"
        );
        assert!(
            tok.state_id.is_none(),
            "comment-only token must not carry state"
        );
    }

    // 5.4 close_out=true token with neither half still carries close_out=true.
    #[test]
    fn closure_only_token_has_close_out_true_no_writes() {
        let store = GatedTokenStore::default();
        let tok_str = store.issue("i1".into(), None, None, true);
        let tok = store.take(&tok_str).unwrap();
        assert!(tok.close_out, "closure-only token must have close_out=true");
        assert!(tok.state_id.is_none());
        assert!(tok.comment.is_none());
    }

    // 5.4 Partial-failure result struct carries both halves independently.
    #[test]
    fn partial_failure_result_shape() {
        let r = GatedWriteResult {
            state: StateOutcome::Failed {
                error: "HTTP 500".into(),
            },
            comment: CommentOutcome::Posted { id: "cmt1".into() },
            closed_out: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            json.contains("\"state\":{\"status\":\"failed\""),
            "got: {json}"
        );
        assert!(
            json.contains("\"comment\":{\"status\":\"posted\""),
            "got: {json}"
        );
        assert!(json.contains("\"closed_out\":true"), "got: {json}");
    }

    // 5.4 Skipped-both shape.
    #[test]
    fn skipped_both_result_shape() {
        let r = GatedWriteResult {
            state: StateOutcome::Skipped,
            comment: CommentOutcome::Skipped,
            closed_out: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"status\":\"skipped\""), "got: {json}");
        assert!(json.contains("\"closed_out\":true"), "got: {json}");
    }

    // 5.4 comment sanitized at token issuance.
    #[test]
    fn comment_sanitized_at_issuance() {
        let raw = "@here the PR landed at https://github.com/org/repo/pull/42";
        let sanitized = sanitize_comment_text(raw);
        let store = GatedTokenStore::default();
        let tok_str = store.issue("i1".into(), None, Some(sanitized.clone()), true);
        let tok = store.take(&tok_str).unwrap();
        assert_eq!(tok.comment.as_deref(), Some(sanitized.as_str()));
        assert!(
            !tok.comment.as_deref().unwrap_or("").contains("@here"),
            "bare @here must not be stored"
        );
    }
}
