//! Typed Linear GraphQL client over the shared `reqwest` client.
//!
//! One POST to `https://api.linear.app/graphql` with `{ query, variables }`.
//! Queries are hand-authored `const &str` documents; nested relations are
//! fetched inline so one issues page brings each issue's
//! state/assignee/labels/project/cycle/parent without a per-issue fan-out.
//!
//! Rate limiting diverges from ClickUp: Linear signals it as **HTTP 400 with a
//! GraphQL error code `RATELIMITED`** (no 429/Retry-After). So the client parses
//! the body even on a 4xx, and on `RATELIMITED` backs off until the reset of the
//! exhausted bucket (clamped), retrying a bounded number of times. A hard
//! complexity rejection is a different error code and is surfaced, not retried.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::auth::{AuthMode, authorization_header_value};
use super::model::{Comment, Connection, Issue, Team, Viewer};

const ENDPOINT: &str = "https://api.linear.app/graphql";
const USER_AGENT: &str = "nergal-linear-sync";
const MAX_RETRIES: u32 = 4;
const BACKOFF_FLOOR: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(60);
/// Modest page size: keeps a nested issues page well under the 10k per-query
/// complexity cap. ~25 issues × ~16 nodes ≈ ~400 points/page.
pub const ISSUES_PAGE_SIZE: u32 = 25;
const LABELS_INNER_SIZE: u32 = 10;
/// Set-3 by-id batch size; matches the page size.
pub const BY_ID_CHUNK: usize = 25;

pub struct LinearClient {
    http: reqwest::Client,
    key: String,
    mode: AuthMode,
}

#[derive(Debug, Deserialize)]
struct GraphQlResponse<T> {
    #[serde(default = "Option::default")]
    data: Option<T>,
    #[serde(default)]
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    extensions: Option<GraphQlErrorExt>,
}

#[derive(Debug, Deserialize)]
struct GraphQlErrorExt {
    #[serde(default)]
    code: Option<String>,
}

/// Outcome of classifying a raw GraphQL response body, independent of transport
/// so it is unit-testable.
#[derive(Debug, PartialEq)]
enum Classified {
    Ok,
    RateLimited,
    /// A non-rate-limit GraphQL error (incl. a hard complexity rejection):
    /// surface it, never retry as a rate-limit. The message is pre-redacted.
    HardError(String),
}

impl LinearClient {
    pub fn new(http: reqwest::Client, key: String, mode: AuthMode) -> Self {
        LinearClient { http, key, mode }
    }

    async fn execute<T: DeserializeOwned>(&self, query: &str, variables: Value) -> Result<T> {
        let body = json!({ "query": query, "variables": variables });
        let auth = authorization_header_value(self.mode, &self.key);
        let mut attempt = 0u32;
        loop {
            let resp = self
                .http
                .post(ENDPOINT)
                .header("Authorization", &auth)
                .header("User-Agent", USER_AGENT)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| anyhow!("linear request: {}", redact(&e.to_string())))?;

            let headers = collect_rate_headers(resp.headers());
            let text = resp
                .text()
                .await
                .map_err(|e| anyhow!("linear response body: {}", redact(&e.to_string())))?;

            match classify_body(&text) {
                Classified::Ok => {
                    let parsed: GraphQlResponse<T> = serde_json::from_str(&text)
                        .map_err(|e| anyhow!("parsing linear response: {e}"))?;
                    return parsed
                        .data
                        .ok_or_else(|| anyhow!("linear response had no data"));
                }
                Classified::HardError(msg) => bail!("linear graphql error: {msg}"),
                Classified::RateLimited => {
                    if attempt >= MAX_RETRIES {
                        bail!("linear rate-limited after {MAX_RETRIES} retries");
                    }
                    let wait = backoff_wait(&headers, now_ms());
                    tracing::warn!("linear RATELIMITED; backing off {:?}", wait);
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Resolve the authenticated user (token validation + cycle viewer resolve).
    pub async fn get_viewer(&self) -> Result<Viewer> {
        #[derive(Deserialize)]
        struct Data {
            viewer: Viewer,
        }
        let d: Data = self.execute(VIEWER_QUERY, json!({})).await?;
        Ok(d.viewer)
    }

    /// All teams with their workflow states + labels inline. States/labels per
    /// team are small vocabularies (first:250, no pagination).
    pub async fn get_teams(&self) -> Result<Vec<Team>> {
        #[derive(Deserialize)]
        struct Data {
            teams: Connection<Team>,
        }
        let d: Data = self.execute(TEAMS_QUERY, json!({})).await?;
        Ok(d.teams.nodes)
    }

    /// One page of issues updated after `updated_after` (ISO8601) for the given
    /// teams, ordered updatedAt desc.
    pub async fn issues_page(
        &self,
        team_ids: &[String],
        updated_after: &str,
        after: Option<String>,
    ) -> Result<Connection<Issue>> {
        #[derive(Deserialize)]
        struct Data {
            issues: Connection<Issue>,
        }
        let vars = json!({
            "teamIds": team_ids,
            "updatedAfter": updated_after,
            "after": after,
            "first": ISSUES_PAGE_SIZE,
            "labelsFirst": LABELS_INNER_SIZE,
        });
        let d: Data = self.execute(ISSUES_WINDOW_QUERY, vars).await?;
        Ok(d.issues)
    }

    /// One page of issues assigned to the viewer within the given teams.
    pub async fn viewer_assigned_issues(
        &self,
        team_ids: &[String],
        after: Option<String>,
    ) -> Result<Connection<Issue>> {
        #[derive(Deserialize)]
        struct Data {
            issues: Connection<Issue>,
        }
        let vars = json!({
            "teamIds": team_ids,
            "after": after,
            "first": ISSUES_PAGE_SIZE,
            "labelsFirst": LABELS_INNER_SIZE,
        });
        let d: Data = self.execute(ISSUES_ASSIGNED_QUERY, vars).await?;
        Ok(d.issues)
    }

    /// One page of issues by id (set-3 delta re-verify). Full nested shape so the
    /// upsert reconciles labels rather than stripping them.
    pub async fn issues_by_id(
        &self,
        ids: &[String],
        after: Option<String>,
    ) -> Result<Connection<Issue>> {
        #[derive(Deserialize)]
        struct Data {
            issues: Connection<Issue>,
        }
        let vars = json!({
            "ids": ids,
            "after": after,
            "first": ISSUES_PAGE_SIZE,
            "labelsFirst": LABELS_INNER_SIZE,
        });
        let d: Data = self.execute(ISSUES_BY_ID_QUERY, vars).await?;
        Ok(d.issues)
    }

    /// Comments for an issue (lazy, on detail-open / updatedAt advance).
    pub async fn issue_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        #[derive(Deserialize)]
        struct Data {
            issue: IssueComments,
        }
        #[derive(Deserialize)]
        struct IssueComments {
            comments: Connection<Comment>,
        }
        let d: Data = self
            .execute(ISSUE_COMMENTS_QUERY, json!({ "id": issue_id }))
            .await?;
        Ok(d.issue.comments.nodes)
    }
}

// ── Pure helpers (unit-testable without a network) ──

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Strip anything that looks like a key from an error string (defense in depth —
/// no method puts the key in a message, but transport errors can echo a URL).
fn redact(s: &str) -> String {
    // Keys are `lin_api_*` / `lin_oauth_*`; never let one survive into a log.
    let mut out = String::with_capacity(s.len());
    for token in s.split_whitespace() {
        if token.starts_with("lin_api_") || token.starts_with("lin_oauth_") {
            out.push_str("[redacted]");
        } else {
            out.push_str(token);
        }
        out.push(' ');
    }
    out.trim_end().to_string()
}

/// Classify a GraphQL response body. RATELIMITED is detected by the error code,
/// not the HTTP status (Linear returns it on a 400).
fn classify_body(text: &str) -> Classified {
    let parsed: GraphQlResponse<Value> = match serde_json::from_str(text) {
        Ok(p) => p,
        // Unparseable body: treat as a hard error (redacted), not a rate-limit.
        Err(e) => return Classified::HardError(redact(&e.to_string())),
    };
    if let Some(errors) = parsed.errors.filter(|e| !e.is_empty()) {
        let is_rate_limited = errors.iter().any(|e| {
            e.extensions
                .as_ref()
                .and_then(|x| x.code.as_deref())
                .map(|c| c.eq_ignore_ascii_case("RATELIMITED"))
                .unwrap_or(false)
        });
        if is_rate_limited {
            return Classified::RateLimited;
        }
        let msg = errors
            .iter()
            .map(|e| redact(&e.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Classified::HardError(msg);
    }
    Classified::Ok
}

fn collect_rate_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for name in [
        "x-ratelimit-requests-remaining",
        "x-ratelimit-requests-reset",
        "x-ratelimit-complexity-remaining",
        "x-ratelimit-complexity-reset",
    ] {
        if let Some(v) = headers.get(name).and_then(|v| v.to_str().ok()) {
            out.insert(name.to_string(), v.to_string());
        }
    }
    out
}

/// Compute the backoff wait from the rate-limit headers. Waits until the reset
/// of the EXHAUSTED bucket (the one whose `*-Remaining` is 0); if neither reads
/// 0, waits until the LATER of the two resets (never the nearer). Clamped to
/// `[1s, 60s]` so neither a forward- nor a backward-skewed clock breaks it.
fn backoff_wait(headers: &HashMap<String, String>, now_ms: u128) -> Duration {
    let get = |k: &str| headers.get(k).and_then(|v| v.parse::<i128>().ok());
    let req_remaining = get("x-ratelimit-requests-remaining");
    let req_reset = get("x-ratelimit-requests-reset");
    let cplx_remaining = get("x-ratelimit-complexity-remaining");
    let cplx_reset = get("x-ratelimit-complexity-reset");

    let target_reset = if req_remaining == Some(0) {
        req_reset
    } else if cplx_remaining == Some(0) {
        cplx_reset
    } else {
        // Neither exhausted (leaky-bucket race): wait for the LATER reset.
        match (req_reset, cplx_reset) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    };

    let wait = match target_reset {
        Some(reset_ms) => {
            let delta = reset_ms - now_ms as i128;
            if delta <= 0 {
                BACKOFF_FLOOR
            } else {
                Duration::from_millis(delta.min(BACKOFF_CAP.as_millis() as i128) as u64)
            }
        }
        None => BACKOFF_FLOOR,
    };
    wait.clamp(BACKOFF_FLOOR, BACKOFF_CAP)
}

// ── GraphQL documents ──

const VIEWER_QUERY: &str = "query { viewer { id name email } }";

const TEAMS_QUERY: &str = "query {
  teams(first: 250) {
    nodes {
      id name key
      states(first: 250) { nodes { id name type color position } }
      labels(first: 250) { nodes { id name color team { id } } }
    }
  }
}";

// Shared issue field selection (kept as one fragment-like block per query to
// avoid GraphQL fragment plumbing; labelsFirst bounds the inner connection).
const ISSUE_FIELDS: &str = "
  id identifier title description priority estimate url
  createdAt updatedAt completedAt dueDate
  team { id }
  state { id }
  assignee { id name displayName email avatarUrl }
  project { id name state }
  cycle { id number name startsAt endsAt }
  parent { id }
  labels(first: $labelsFirst) { nodes { id name color team { id } } }
";

// Issues updated after a cutoff for the given teams, ordered updatedAt desc.
const ISSUES_WINDOW_QUERY: &str = "query Win($teamIds: [ID!], $updatedAfter: DateTimeOrDuration, $after: String, $first: Int!, $labelsFirst: Int!) {
  issues(
    first: $first, after: $after,
    orderBy: updatedAt,
    filter: { team: { id: { in: $teamIds } }, updatedAt: { gt: $updatedAfter } }
  ) {
    nodes {ISSUE_FIELDS}
    pageInfo { hasNextPage endCursor }
  }
}";

const ISSUES_ASSIGNED_QUERY: &str =
    "query Assigned($teamIds: [ID!], $after: String, $first: Int!, $labelsFirst: Int!) {
  issues(
    first: $first, after: $after,
    filter: { team: { id: { in: $teamIds } }, assignee: { isMe: { eq: true } } }
  ) {
    nodes {ISSUE_FIELDS}
    pageInfo { hasNextPage endCursor }
  }
}";

const ISSUES_BY_ID_QUERY: &str =
    "query ById($ids: [ID!], $after: String, $first: Int!, $labelsFirst: Int!) {
  issues(first: $first, after: $after, filter: { id: { in: $ids } }) {
    nodes {ISSUE_FIELDS}
    pageInfo { hasNextPage endCursor }
  }
}";

const ISSUE_COMMENTS_QUERY: &str = "query Comments($id: String!) {
  issue(id: $id) {
    comments(first: 100) { nodes { id body createdAt user { id name displayName email avatarUrl } } }
  }
}";

/// Inline the shared field block into the query documents at build time.
fn expand(query: &str) -> String {
    query.replace("ISSUE_FIELDS", ISSUE_FIELDS)
}

// Public accessors so the poller/tests can see the expanded documents if needed.
pub fn issues_window_query() -> String {
    expand(ISSUES_WINDOW_QUERY)
}
pub fn issues_assigned_query() -> String {
    expand(ISSUES_ASSIGNED_QUERY)
}
pub fn issues_by_id_query() -> String {
    expand(ISSUES_BY_ID_QUERY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_ok() {
        let body = r#"{"data":{"viewer":{"id":"u1"}}}"#;
        assert_eq!(classify_body(body), Classified::Ok);
    }

    #[test]
    fn classify_ratelimited_on_400_body() {
        // Linear sends RATELIMITED in the errors array, on an HTTP 400.
        let body = r#"{"errors":[{"message":"rate limited","extensions":{"code":"RATELIMITED"}}]}"#;
        assert_eq!(classify_body(body), Classified::RateLimited);
    }

    #[test]
    fn classify_hard_complexity_error_is_not_ratelimited() {
        let body = r#"{"errors":[{"message":"query too complex","extensions":{"code":"COMPLEXITY_LIMIT"}}]}"#;
        match classify_body(body) {
            Classified::HardError(m) => assert!(m.contains("too complex")),
            other => panic!("expected HardError, got {other:?}"),
        }
    }

    #[test]
    fn classify_redacts_key_in_error() {
        let body = r#"{"errors":[{"message":"bad key lin_api_SECRET123 rejected"}]}"#;
        match classify_body(body) {
            Classified::HardError(m) => {
                assert!(!m.contains("lin_api_SECRET123"));
                assert!(m.contains("[redacted]"));
            }
            other => panic!("expected HardError, got {other:?}"),
        }
    }

    #[test]
    fn backoff_waits_on_exhausted_complexity_bucket_not_nearer() {
        // requests resets sooner but isn't exhausted; complexity is exhausted and
        // resets later → wait must target complexity's (later) reset, clamped.
        let now = 1_000_000u128;
        let mut h = HashMap::new();
        h.insert("x-ratelimit-requests-remaining".into(), "500".into());
        h.insert("x-ratelimit-requests-reset".into(), "1002000".into()); // +2s
        h.insert("x-ratelimit-complexity-remaining".into(), "0".into());
        h.insert("x-ratelimit-complexity-reset".into(), "1030000".into()); // +30s
        let w = backoff_wait(&h, now);
        assert_eq!(w, Duration::from_millis(30_000));
    }

    #[test]
    fn backoff_floors_on_backward_skew() {
        // reset already in the past → floor, not zero (don't busy-retry).
        let now = 5_000_000u128;
        let mut h = HashMap::new();
        h.insert("x-ratelimit-complexity-remaining".into(), "0".into());
        h.insert("x-ratelimit-complexity-reset".into(), "1000000".into()); // past
        let w = backoff_wait(&h, now);
        assert_eq!(w, BACKOFF_FLOOR);
    }

    #[test]
    fn backoff_neither_exhausted_takes_later_reset() {
        let now = 1_000_000u128;
        let mut h = HashMap::new();
        h.insert("x-ratelimit-requests-remaining".into(), "10".into());
        h.insert("x-ratelimit-requests-reset".into(), "1005000".into()); // +5s
        h.insert("x-ratelimit-complexity-remaining".into(), "10".into());
        h.insert("x-ratelimit-complexity-reset".into(), "1010000".into()); // +10s later
        let w = backoff_wait(&h, now);
        assert_eq!(w, Duration::from_millis(10_000));
    }

    #[test]
    fn backoff_caps_at_60s() {
        let now = 0u128;
        let mut h = HashMap::new();
        h.insert("x-ratelimit-requests-remaining".into(), "0".into());
        h.insert("x-ratelimit-requests-reset".into(), "999999999".into());
        let w = backoff_wait(&h, now);
        assert_eq!(w, BACKOFF_CAP);
    }

    #[test]
    fn query_documents_expand_fields() {
        let q = issues_window_query();
        assert!(q.contains("assignee { id name displayName"));
        assert!(!q.contains("ISSUE_FIELDS"));
    }

    #[test]
    fn redact_strips_keys() {
        assert!(!redact("error with lin_api_abc123 here").contains("lin_api_abc123"));
        assert_eq!(redact("clean message"), "clean message");
    }
}
