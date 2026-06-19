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
use super::model::{Comment, Connection, HistoryEntry, Issue, OrganizationInfo, Team, Viewer};

/// Raw detail payload from a single detail-open fetch (comments + attachments +
/// relations + history + creation meta), normalized into the panel's view model
/// by `linear::mod`.
pub struct RawIssueDetail {
    pub comments: Vec<Comment>,
    pub attachments: Vec<super::model::Attachment>,
    pub relations: Vec<super::model::Relation>,
    pub history: Vec<HistoryEntry>,
    pub created_at: Option<String>,
    pub creator: Option<super::model::User>,
}

const ENDPOINT: &str = "https://api.linear.app/graphql";
const USER_AGENT: &str = "nergal-linear-sync";
/// Inline-image proxy: the only host the authenticated fetcher will touch.
const UPLOADS_HOST: &str = "uploads.linear.app";
/// Cap on a single proxied image (data URLs round-trip over IPC; keep it sane).
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;
const MAX_RETRIES: u32 = 4;
const BACKOFF_FLOOR: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(60);
/// Modest page size: keeps a nested issues page well under the 10k per-query
/// complexity cap. ~25 issues × ~16 nodes ≈ ~400 points/page.
pub const ISSUES_PAGE_SIZE: u32 = 25;
const LABELS_INNER_SIZE: u32 = 10;
/// Set-3 by-id batch size; matches the page size.
pub const BY_ID_CHUNK: usize = 25;
/// Flat vocabulary page size (teams/states/labels). Each flat query costs
/// ~first × (a few scalars), well under the 10k cap; pagination handles volume.
const VOCAB_PAGE_SIZE: u32 = 100;

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

    /// Resolve the workspace (organization) this key belongs to — a key is
    /// workspace-scoped, so this identifies which workspace it manages.
    pub async fn resolve_organization(&self) -> Result<OrganizationInfo> {
        #[derive(Deserialize)]
        struct Data {
            organization: OrganizationInfo,
        }
        let d: Data = self.execute(ORGANIZATION_QUERY, json!({})).await?;
        Ok(d.organization)
    }

    /// Teams (bare). States and labels are fetched flat via separate top-level
    /// queries (`all_workflow_states`/`all_labels`) — nesting them under teams
    /// would multiply the per-query complexity past Linear's 10k cap. Paginated.
    pub async fn all_teams(&self) -> Result<Vec<Team>> {
        #[derive(Deserialize)]
        struct Data {
            teams: Connection<Team>,
        }
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let d: Data = self
                .execute(
                    TEAMS_QUERY,
                    json!({ "after": after, "first": VOCAB_PAGE_SIZE }),
                )
                .await?;
            out.extend(d.teams.nodes);
            if d.teams.page_info.has_next_page {
                match d.teams.page_info.end_cursor {
                    Some(c) => after = Some(c),
                    None => break,
                }
            } else {
                break;
            }
        }
        Ok(out)
    }

    /// All workflow states across teams (each carries `team { id }`). Paginated.
    pub async fn all_workflow_states(&self) -> Result<Vec<super::model::WorkflowState>> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "workflowStates")]
            workflow_states: Connection<super::model::WorkflowState>,
        }
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let d: Data = self
                .execute(
                    WORKFLOW_STATES_QUERY,
                    json!({ "after": after, "first": VOCAB_PAGE_SIZE }),
                )
                .await?;
            out.extend(d.workflow_states.nodes);
            if d.workflow_states.page_info.has_next_page {
                match d.workflow_states.page_info.end_cursor {
                    Some(c) => after = Some(c),
                    None => break,
                }
            } else {
                break;
            }
        }
        Ok(out)
    }

    /// All issue labels (each carries `team { id }`, null for workspace labels).
    /// Paginated.
    pub async fn all_labels(&self) -> Result<Vec<super::model::Label>> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "issueLabels")]
            issue_labels: Connection<super::model::Label>,
        }
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let d: Data = self
                .execute(
                    LABELS_QUERY,
                    json!({ "after": after, "first": VOCAB_PAGE_SIZE }),
                )
                .await?;
            out.extend(d.issue_labels.nodes);
            if d.issue_labels.page_info.has_next_page {
                match d.issue_labels.page_info.end_cursor {
                    Some(c) => after = Some(c),
                    None => break,
                }
            } else {
                break;
            }
        }
        Ok(out)
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
        let d: Data = self.execute(&issues_window_query(), vars).await?;
        Ok(d.issues)
    }

    /// One page of issues assigned to the viewer within the given teams. Uses
    /// the resolved viewer id (`assignee: { id: { eq } }`) — the confirmed
    /// comparator pattern — rather than `isMe`.
    pub async fn viewer_assigned_issues(
        &self,
        team_ids: &[String],
        viewer_id: &str,
        after: Option<String>,
    ) -> Result<Connection<Issue>> {
        #[derive(Deserialize)]
        struct Data {
            issues: Connection<Issue>,
        }
        let vars = json!({
            "teamIds": team_ids,
            "viewerId": viewer_id,
            "after": after,
            "first": ISSUES_PAGE_SIZE,
            "labelsFirst": LABELS_INNER_SIZE,
        });
        let d: Data = self.execute(&issues_assigned_query(), vars).await?;
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
        let d: Data = self.execute(&issues_by_id_query(), vars).await?;
        Ok(d.issues)
    }

    /// Comments + attachments + relations + history (activity) for an issue
    /// (lazy, on detail-open). Not persisted — fetched fresh each open.
    pub async fn issue_detail(&self, issue_id: &str) -> Result<RawIssueDetail> {
        #[derive(Deserialize)]
        struct Data {
            issue: IssueDetail,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueDetail {
            #[serde(default)]
            created_at: Option<String>,
            #[serde(default)]
            creator: Option<super::model::User>,
            #[serde(default)]
            comments: Connection<Comment>,
            #[serde(default)]
            attachments: Connection<super::model::Attachment>,
            #[serde(default)]
            relations: Connection<super::model::Relation>,
            #[serde(default)]
            history: Connection<super::model::HistoryEntry>,
        }
        let d: Data = self
            .execute(ISSUE_DETAIL_QUERY, json!({ "id": issue_id }))
            .await?;
        Ok(RawIssueDetail {
            comments: d.issue.comments.nodes,
            attachments: d.issue.attachments.nodes,
            relations: d.issue.relations.nodes,
            history: d.issue.history.nodes,
            created_at: d.issue.created_at,
            creator: d.issue.creator,
        })
    }

    /// Fetch an `uploads.linear.app` asset with the Linear auth header attached.
    /// The webview can't send that header itself (a bare `<img src>` to the CDN
    /// 401s), so the panel routes inline issue images through here. Host-pinned
    /// to `uploads.linear.app` (SSRF-safe) and `image/*`-only so it can't be
    /// repurposed as a generic authenticated fetcher. Returns `(content_type,
    /// bytes)`.
    pub async fn fetch_image(&self, url: &str) -> Result<(String, Vec<u8>)> {
        if !is_uploads_url(url) {
            bail!("linear image url not allowed");
        }
        let auth = authorization_header_value(self.mode, &self.key);
        let resp = self
            .http
            .get(url)
            .header("Authorization", &auth)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|e| anyhow!("linear image request: {}", redact(&e.to_string())))?;
        if !resp.status().is_success() {
            bail!("linear image fetch: HTTP {}", resp.status().as_u16());
        }
        if let Some(len) = resp.content_length()
            && len as usize > MAX_IMAGE_BYTES
        {
            bail!("linear image too large");
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        if !content_type.starts_with("image/") {
            bail!("linear image fetch: unexpected content-type");
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| anyhow!("linear image body: {}", redact(&e.to_string())))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            bail!("linear image too large");
        }
        Ok((content_type, bytes.to_vec()))
    }

    // ── Write mutations (linear-writeback) ──

    /// Update an issue's state and/or assignee.
    ///
    /// The outer `Option` on `assignee_id` in `IssueUpdateInput` is intentionally
    /// handled by serde's `skip_serializing_if = "Option::is_none"` (via the
    /// wrapper below) so a state-only write **omits** the `assigneeId` key
    /// entirely and does NOT unassign.  An inner `None` (`Some(None)`) explicitly
    /// clears the assignee.
    ///
    /// Returns `Err` on a GraphQL error OR when `success != true` (Decision 8).
    pub async fn issue_update(
        &self,
        issue_id: &str,
        input: IssueUpdateInput,
    ) -> Result<UpdatedIssue> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct IssueUpdatePayload {
            success: bool,
            issue: Option<UpdatedIssue>,
        }
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "issueUpdate")]
            issue_update: IssueUpdatePayload,
        }
        const MUTATION: &str = "mutation Update($id: String!, $input: IssueUpdateInput!) {
  issueUpdate(id: $id, input: $input) {
    success
    issue { state { id } assignee { id } cycle { id } }
  }
}";
        let d: Data = self
            .execute(
                MUTATION,
                serde_json::json!({ "id": issue_id, "input": input }),
            )
            .await?;
        if !d.issue_update.success {
            bail!("issueUpdate returned success=false for issue {issue_id}");
        }
        d.issue_update
            .issue
            .ok_or_else(|| anyhow!("issueUpdate succeeded but returned no issue"))
    }

    /// Create a comment on an issue.
    ///
    /// Returns the created comment id on success.  Returns `Err` on a GraphQL
    /// error OR when `success != true` (Decision 8).
    pub async fn comment_create(&self, issue_id: &str, body: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct CreatedComment {
            id: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CommentCreatePayload {
            success: bool,
            comment: Option<CreatedComment>,
        }
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "commentCreate")]
            comment_create: CommentCreatePayload,
        }
        const MUTATION: &str = "mutation Comment($input: CommentCreateInput!) {
  commentCreate(input: $input) {
    success
    comment { id }
  }
}";
        let d: Data = self
            .execute(
                MUTATION,
                serde_json::json!({ "input": { "issueId": issue_id, "body": body } }),
            )
            .await?;
        if !d.comment_create.success {
            bail!("commentCreate returned success=false for issue {issue_id}");
        }
        d.comment_create
            .comment
            .map(|c| c.id)
            .ok_or_else(|| anyhow!("commentCreate succeeded but returned no comment"))
    }
}

/// Updated issue fields returned by `issueUpdate`.
#[derive(Debug, Deserialize)]
pub struct UpdatedIssue {
    #[serde(default)]
    pub state: Option<IdRef>,
    #[serde(default)]
    pub assignee: Option<IdRef>,
    #[serde(default)]
    pub cycle: Option<IdRef>,
}

/// Input for `issueUpdate`.  Both fields are individually optional:
/// - omit `state_id` → do not change the state
/// - omit `assignee_id` (outer `None`) → do not change the assignee
/// - `assignee_id = Some(None)` → clear the assignee (unassign)
/// - `assignee_id = Some(Some(id))` → set assignee
///
/// The `skip_serializing_if` guard on `assignee_id` is critical: without it
/// serde serialises `outer-None` as `assigneeId: null`, silently unassigning
/// on a state-only write (Decision 8, review N4).
#[derive(Debug, Default, serde::Serialize)]
pub struct IssueUpdateInput {
    #[serde(rename = "stateId", skip_serializing_if = "Option::is_none")]
    pub state_id: Option<String>,
    #[serde(rename = "assigneeId", skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<Option<String>>,
    /// `Some(Some(id))` → move to cycle; `Some(None)` → remove from cycle
    /// (`cycleId: null`); outer `None` → leave unchanged.
    #[serde(rename = "cycleId", skip_serializing_if = "Option::is_none")]
    pub cycle_id: Option<Option<String>>,
}

/// A bare `{ id }` reference (re-exported for use in `client.rs` tests without
/// reaching into model).
use super::model::IdRef;

/// Host-pinned allowlist for the inline-image proxy: only `https` URLs whose
/// host is exactly `uploads.linear.app`. Parsing via `url::Url` defeats
/// `uploads.linear.app.evil.com` / userinfo-prefix spoofing.
pub fn is_uploads_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(u) => u.scheme() == "https" && u.host_str() == Some(UPLOADS_HOST),
        Err(_) => false,
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
        // `contains` (not `starts_with`): catch a key embedded mid-token, e.g.
        // `key=lin_api_…` or `(lin_api_…)` in a transport/URL error echo.
        if token.contains("lin_api_") || token.contains("lin_oauth_") {
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
const ORGANIZATION_QUERY: &str = "query { organization { id name urlKey } }";

// Vocabularies (teams/states/labels) are fetched FLAT, not nested, because
// Linear bills complexity as the PRODUCT of nested `first` values — a
// teams(250){ states(250) labels(250) } query is ~125k complexity, far past the
// 10k per-query cap. Flat top-level queries are each independently small.
const TEAMS_QUERY: &str = "query Teams($first: Int!, $after: String) {
  teams(first: $first, after: $after) {
    nodes { id name key estimationType: issueEstimationType }
    pageInfo { hasNextPage endCursor }
  }
}";

const WORKFLOW_STATES_QUERY: &str = "query States($first: Int!, $after: String) {
  workflowStates(first: $first, after: $after) {
    nodes { id name type color position team { id } }
    pageInfo { hasNextPage endCursor }
  }
}";

const LABELS_QUERY: &str = "query Labels($first: Int!, $after: String) {
  issueLabels(first: $first, after: $after) {
    nodes { id name color team { id } }
    pageInfo { hasNextPage endCursor }
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
    "query Assigned($teamIds: [ID!], $viewerId: ID!, $after: String, $first: Int!, $labelsFirst: Int!) {
  issues(
    first: $first, after: $after,
    filter: { team: { id: { in: $teamIds } }, assignee: { id: { eq: $viewerId } } }
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

const ISSUE_DETAIL_QUERY: &str = "query Detail($id: String!) {
  issue(id: $id) {
    createdAt
    creator { id name displayName email avatarUrl }
    comments(first: 100) { nodes { id body createdAt user { id name displayName email avatarUrl } parent { id } } }
    attachments(first: 50) { nodes { id title subtitle url } }
    relations(first: 50) { nodes { type relatedIssue { id identifier title } } }
    history(first: 50) {
      nodes {
        id createdAt
        actor { id name displayName email avatarUrl }
        botActor { name }
        fromState { name } toState { name }
        fromAssignee { id name displayName email avatarUrl }
        toAssignee { id name displayName email avatarUrl }
        addedLabelIds removedLabelIds
        fromCycle { number name } toCycle { number name }
        fromPriority toPriority
        fromEstimate toEstimate
        fromDueDate toDueDate
        fromTitle toTitle
        updatedDescription
      }
    }
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
    fn is_uploads_url_pins_host_and_scheme() {
        assert!(is_uploads_url("https://uploads.linear.app/a/b/c"));
        // wrong scheme
        assert!(!is_uploads_url("http://uploads.linear.app/a"));
        // suffix-spoof host
        assert!(!is_uploads_url("https://uploads.linear.app.evil.com/a"));
        // unrelated host
        assert!(!is_uploads_url("https://evil.com/a"));
        // the api host is not an image host
        assert!(!is_uploads_url("https://api.linear.app/graphql"));
        assert!(!is_uploads_url("not a url"));
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

    // ── IssueUpdateInput serde shape (Decision 8 / review N4) ──

    // 1.3 State-only write omits assigneeId entirely (not null).
    #[test]
    fn issue_update_state_only_omits_assignee_id() {
        let input = IssueUpdateInput {
            state_id: Some("s1".into()),
            assignee_id: None,
            cycle_id: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("stateId"), "must contain stateId: {json}");
        assert!(
            !json.contains("assigneeId"),
            "state-only write must NOT contain assigneeId (not even null): {json}"
        );
    }

    // 1.3 Assignee-only write (set).
    #[test]
    fn issue_update_assignee_only_contains_assignee_id() {
        let input = IssueUpdateInput {
            state_id: None,
            assignee_id: Some(Some("u1".into())),
            cycle_id: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(
            json.contains("assigneeId"),
            "must contain assigneeId: {json}"
        );
        assert!(!json.contains("stateId"), "must omit stateId: {json}");
    }

    // 1.3 Explicit unassign: assigneeId present as null.
    #[test]
    fn issue_update_explicit_unassign_produces_null() {
        let input = IssueUpdateInput {
            state_id: None,
            assignee_id: Some(None),
            cycle_id: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(
            json.contains(r#""assigneeId":null"#),
            "explicit unassign must be assigneeId:null: {json}"
        );
    }

    // Cycle set vs remove vs untouched serialization (mirrors the assignee
    // double-option semantics: Some(None) → cycleId:null removes from cycle).
    #[test]
    fn issue_update_cycle_set_remove_and_omit() {
        let set = serde_json::to_string(&IssueUpdateInput {
            cycle_id: Some(Some("c1".into())),
            ..Default::default()
        })
        .unwrap();
        assert!(
            set.contains(r#""cycleId":"c1""#),
            "set must carry the id: {set}"
        );

        let remove = serde_json::to_string(&IssueUpdateInput {
            cycle_id: Some(None),
            ..Default::default()
        })
        .unwrap();
        assert!(
            remove.contains(r#""cycleId":null"#),
            "remove must be cycleId:null: {remove}"
        );

        let untouched = serde_json::to_string(&IssueUpdateInput {
            state_id: Some("s1".into()),
            ..Default::default()
        })
        .unwrap();
        assert!(
            !untouched.contains("cycleId"),
            "state-only write must omit cycleId: {untouched}"
        );
    }

    // 1.3 success=false with no errors is classified as a write failure.
    // We can't call the method without a live server, but we verify the shape
    // serde expects so the runtime path works correctly: a response with
    // success=false must NOT be treated as Ok.
    #[test]
    fn issue_update_payload_success_false_deserialized() {
        // The `execute` helper returns Ok when there are no errors in the body;
        // the issueUpdate method must check the `success` field in the payload.
        #[derive(serde::Deserialize)]
        struct Payload {
            success: bool,
        }
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueUpdate")]
            issue_update: Payload,
        }
        let body = r#"{"data":{"issueUpdate":{"success":false,"issue":null}}}"#;
        let parsed: super::GraphQlResponse<Data> = serde_json::from_str(body).unwrap();
        let d = parsed.data.unwrap();
        assert!(
            !d.issue_update.success,
            "success=false must not be swallowed"
        );
    }

    // 1.3 commentCreate shape: issueId + body.
    #[test]
    fn comment_create_payload_shape() {
        #[derive(serde::Deserialize)]
        struct Payload {
            success: bool,
            comment: Option<Comment>,
        }
        #[derive(serde::Deserialize)]
        struct Comment {
            id: String,
        }
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "commentCreate")]
            comment_create: Payload,
        }
        let body = r#"{"data":{"commentCreate":{"success":true,"comment":{"id":"cmt1"}}}}"#;
        let parsed: super::GraphQlResponse<Data> = serde_json::from_str(body).unwrap();
        let d = parsed.data.unwrap();
        assert!(d.comment_create.success);
        assert_eq!(d.comment_create.comment.unwrap().id, "cmt1");
    }

    // 1.3 commentCreate success=false → classified as failure.
    #[test]
    fn comment_create_success_false_deserialized() {
        #[derive(serde::Deserialize)]
        struct Payload {
            success: bool,
        }
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "commentCreate")]
            comment_create: Payload,
        }
        let body = r#"{"data":{"commentCreate":{"success":false,"comment":null}}}"#;
        let parsed: super::GraphQlResponse<Data> = serde_json::from_str(body).unwrap();
        let d = parsed.data.unwrap();
        assert!(!d.comment_create.success, "success=false must be caught");
    }
}
