//! Typed ClickUp API v2 read client over one shared `reqwest::Client`.
//!
//! Auth header is `Authorization: <token>` — the v2 personal-token scheme,
//! no `Bearer` prefix. The header value is marked sensitive so it can never
//! surface through Debug formatting, and no error path in this module embeds
//! the token (reqwest errors carry URLs, never header values).

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::model::{
    Comment, CommentsResponse, Folder, FoldersResponse, List, ListsResponse, Space, SpacesResponse,
    Task, TasksPage, Team, TeamsResponse, User, UserResponse,
};

const DEFAULT_BASE_URL: &str = "https://api.clickup.com/api/v2";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounded 429 retries: after this many waits the call gives up.
const MAX_RATE_LIMIT_RETRIES: u32 = 4;
/// Cap on a server-provided Retry-After so a bogus header can't park a poll.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);
/// Defensive ceiling for `last_page`-driven pagination: a shape change that
/// drops the flag must not loop forever.
const MAX_TASK_PAGES: u32 = 1000;

pub struct ClickUpClient {
    http: reqwest::Client,
    base_url: String,
    auth: HeaderValue,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilterParams {
    pub space_ids: Vec<String>,
    pub page: u32,
    pub include_closed: bool,
    /// Server-side assignee filter. Empty = no filter (whole space). Used to
    /// scope the on-demand closed-task fetch to the current user so it doesn't
    /// page through the entire workspace's closed history.
    pub assignee_ids: Vec<i64>,
}

impl ClickUpClient {
    pub fn new(token: &str) -> Result<Self> {
        Self::with_base_url(token, DEFAULT_BASE_URL)
    }

    /// `base_url` override exists for tests against a local mock server.
    pub fn with_base_url(token: &str, base_url: &str) -> Result<Self> {
        let mut auth = HeaderValue::from_str(token.trim())
            // The header error would echo the invalid value — redact.
            .map_err(|_| anyhow!("token contains characters invalid in a header (redacted)"))?;
        auth.set_sensitive(true);
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("nergal-clickup-sync")
            .build()
            .map_err(|e| anyhow!("building http client: {e}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth,
        })
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt: u32 = 0;
        loop {
            let resp = self
                .http
                .get(&url)
                .query(query)
                .header(AUTHORIZATION, self.auth.clone())
                .send()
                .await
                .map_err(|e| anyhow!("clickup GET {path}: {e}"))?;

            let status = resp.status();
            if status == StatusCode::TOO_MANY_REQUESTS {
                attempt += 1;
                if attempt > MAX_RATE_LIMIT_RETRIES {
                    bail!(
                        "clickup GET {path}: rate limited, gave up after {MAX_RATE_LIMIT_RETRIES} retries"
                    );
                }
                let wait = retry_after(resp.headers())
                    .unwrap_or_else(|| Duration::from_secs(u64::from(attempt)))
                    .min(MAX_RETRY_AFTER);
                tracing::debug!("clickup 429 on {path}; retrying in {wait:?} (attempt {attempt})");
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                bail!("clickup GET {path}: HTTP {status}");
            }
            return resp
                .json::<T>()
                .await
                .map_err(|e| anyhow!("clickup GET {path}: parsing response: {e}"));
        }
    }

    pub async fn get_user(&self) -> Result<User> {
        let resp: UserResponse = self.get_json("/user", &[]).await?;
        Ok(resp.user)
    }

    pub async fn get_teams(&self) -> Result<Vec<Team>> {
        let resp: TeamsResponse = self.get_json("/team", &[]).await?;
        Ok(resp.teams)
    }

    pub async fn get_spaces(&self, team_id: &str) -> Result<Vec<Space>> {
        let resp: SpacesResponse = self
            .get_json(&format!("/team/{team_id}/space"), &[])
            .await?;
        Ok(resp.spaces)
    }

    /// Folders with their nested lists; each List carries `statuses[]`
    /// inline, so the poll never needs a per-List status call.
    pub async fn get_folders(&self, space_id: &str) -> Result<Vec<Folder>> {
        let resp: FoldersResponse = self
            .get_json(&format!("/space/{space_id}/folder"), &[])
            .await?;
        Ok(resp.folders)
    }

    /// Folderless lists; the payload's `folder` ref carries `hidden: true`.
    pub async fn get_folderless_lists(&self, space_id: &str) -> Result<Vec<List>> {
        let resp: ListsResponse = self
            .get_json(&format!("/space/{space_id}/list"), &[])
            .await?;
        Ok(resp.lists)
    }

    pub async fn get_lists(&self, folder_id: &str) -> Result<Vec<List>> {
        let resp: ListsResponse = self
            .get_json(&format!("/folder/{folder_id}/list"), &[])
            .await?;
        Ok(resp.lists)
    }

    /// One page of the all-tasks poll scope. Always `subtasks=true` (flat
    /// subtasks with `parent`); no assignee filter — assigned-to-me is a
    /// local panel filter (Decision 4).
    pub async fn filter_team_tasks(
        &self,
        team_id: &str,
        params: &TaskFilterParams,
    ) -> Result<TasksPage> {
        let mut query: Vec<(String, String)> = vec![
            ("page".into(), params.page.to_string()),
            ("subtasks".into(), "true".into()),
            ("include_closed".into(), params.include_closed.to_string()),
        ];
        for space_id in &params.space_ids {
            query.push(("space_ids[]".into(), space_id.clone()));
        }
        for assignee in &params.assignee_ids {
            query.push(("assignees[]".into(), assignee.to_string()));
        }
        self.get_json(&format!("/team/{team_id}/task"), &query)
            .await
    }

    /// All pages of the filtered team tasks endpoint. Terminates on the
    /// response `last_page` flag, never on row count — the endpoint filters
    /// after the page slice, so a short page is not the last page.
    pub async fn filter_team_tasks_all(
        &self,
        team_id: &str,
        space_ids: &[String],
        include_closed: bool,
        assignee_ids: &[i64],
    ) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        let mut params = TaskFilterParams {
            space_ids: space_ids.to_vec(),
            page: 0,
            include_closed,
            assignee_ids: assignee_ids.to_vec(),
        };
        loop {
            let page = self.filter_team_tasks(team_id, &params).await?;
            let empty = page.tasks.is_empty();
            tasks.extend(page.tasks);
            match page.last_page {
                Some(true) => break,
                Some(false) => {}
                // Flag absent (shape drift): an empty page is the only safe
                // terminator left.
                None if empty => break,
                None => {}
            }
            params.page += 1;
            if params.page >= MAX_TASK_PAGES {
                bail!("clickup task pagination exceeded {MAX_TASK_PAGES} pages without last_page");
            }
        }
        Ok(tasks)
    }

    /// Detail-only fetch; the nested `subtasks` array it returns is never
    /// the reconcile tree source (Decision 8).
    pub async fn get_task(&self, task_id: &str, include_subtasks: bool) -> Result<Task> {
        let query = vec![("include_subtasks".into(), include_subtasks.to_string())];
        self.get_json(&format!("/task/{task_id}"), &query).await
    }

    pub async fn get_task_comments(&self, task_id: &str) -> Result<Vec<Comment>> {
        let resp: CommentsResponse = self
            .get_json(&format!("/task/{task_id}/comment"), &[])
            .await?;
        Ok(resp.comments)
    }

    /// On-demand detail / show-closed only — not part of the poll loop.
    pub async fn get_list(&self, list_id: &str) -> Result<List> {
        self.get_json(&format!("/list/{list_id}"), &[]).await
    }

    // ── Write helpers (clickup-writeback) ──

    async fn post_json_body<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = serde_json::to_vec(body)
            .map_err(|e| anyhow!("clickup POST {path}: serializing body: {e}"))?;
        let mut attempt: u32 = 0;
        loop {
            let resp = self
                .http
                .post(&url)
                .header(AUTHORIZATION, self.auth.clone())
                .header(CONTENT_TYPE, "application/json")
                .body(body_bytes.clone())
                .send()
                .await
                .map_err(|e| anyhow!("clickup POST {path}: {e}"))?;

            let status = resp.status();
            if status == StatusCode::TOO_MANY_REQUESTS {
                attempt += 1;
                if attempt > MAX_RATE_LIMIT_RETRIES {
                    bail!(
                        "clickup POST {path}: rate limited, gave up after {MAX_RATE_LIMIT_RETRIES} retries"
                    );
                }
                let wait = retry_after(resp.headers())
                    .unwrap_or_else(|| Duration::from_secs(u64::from(attempt)))
                    .min(MAX_RETRY_AFTER);
                tracing::debug!(
                    "clickup 429 on POST {path}; retrying in {wait:?} (attempt {attempt})"
                );
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                bail!("clickup POST {path}: HTTP {status}");
            }
            return resp
                .json::<T>()
                .await
                .map_err(|e| anyhow!("clickup POST {path}: parsing response: {e}"));
        }
    }

    async fn put_json_body<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = serde_json::to_vec(body)
            .map_err(|e| anyhow!("clickup PUT {path}: serializing body: {e}"))?;
        let mut attempt: u32 = 0;
        loop {
            let resp = self
                .http
                .put(&url)
                .header(AUTHORIZATION, self.auth.clone())
                .header(CONTENT_TYPE, "application/json")
                .body(body_bytes.clone())
                .send()
                .await
                .map_err(|e| anyhow!("clickup PUT {path}: {e}"))?;

            let status = resp.status();
            if status == StatusCode::TOO_MANY_REQUESTS {
                attempt += 1;
                if attempt > MAX_RATE_LIMIT_RETRIES {
                    bail!(
                        "clickup PUT {path}: rate limited, gave up after {MAX_RATE_LIMIT_RETRIES} retries"
                    );
                }
                let wait = retry_after(resp.headers())
                    .unwrap_or_else(|| Duration::from_secs(u64::from(attempt)))
                    .min(MAX_RETRY_AFTER);
                tracing::debug!(
                    "clickup 429 on PUT {path}; retrying in {wait:?} (attempt {attempt})"
                );
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                bail!("clickup PUT {path}: HTTP {status}");
            }
            return resp
                .json::<T>()
                .await
                .map_err(|e| anyhow!("clickup PUT {path}: parsing response: {e}"));
        }
    }

    /// `PUT /task/{id}` — moves the task to `status_name` in its list's workflow.
    /// The caller is responsible for validating that `status_name` is a real
    /// status for that list (command-boundary check in `mod.rs`).
    pub async fn set_task_status(&self, task_id: &str, status_name: &str) -> Result<Task> {
        self.put_json_body(
            &format!("/task/{task_id}"),
            &json!({ "status": status_name }),
        )
        .await
    }

    /// `POST /task/{id}/comment` — posts a new comment, returns the created
    /// comment id. The comment body shape ClickUp expects is
    /// `{ "comment_text": "…", "notify_all": false }`.
    pub async fn add_comment(&self, task_id: &str, text: &str) -> Result<String> {
        let body = json!({ "comment_text": text, "notify_all": false });
        let resp: Value = self
            .post_json_body(&format!("/task/{task_id}/comment"), &body)
            .await?;
        resp.get("id")
            .and_then(|v| {
                v.as_str()
                    .map(String::from)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .ok_or_else(|| anyhow!("clickup add_comment: response missing 'id'"))
    }

    /// `PUT /checklist/{checklist_id}/checklist_item/{item_id}` — resolves or
    /// un-resolves a checklist item.
    pub async fn set_checklist_item(
        &self,
        checklist_id: &str,
        item_id: &str,
        resolved: bool,
    ) -> Result<Value> {
        self.put_json_body(
            &format!("/checklist/{checklist_id}/checklist_item/{item_id}"),
            &json!({ "resolved": resolved }),
        )
        .await
    }

    /// `PUT /task/{id}` — partial update for description, assignees, and
    /// due_date. Only fields wrapped in `Some` are sent; absent fields are
    /// not touched. Assignees follow the ClickUp v2 delta shape
    /// `{ "add": [ids…], "rem": [ids…] }` rather than replacement.
    pub async fn update_task(&self, task_id: &str, update: &TaskUpdate) -> Result<Task> {
        let body = update.to_json();
        self.put_json_body(&format!("/task/{task_id}"), &body).await
    }

    /// `POST /task/{id}/field/{field_id}` — sets a custom field value.
    /// The `value` parameter must already be serialized to the correct JSON
    /// shape for the field type; see `serialize_custom_field_value` in
    /// `mod.rs`.
    pub async fn set_custom_field(
        &self,
        task_id: &str,
        field_id: &str,
        value: Value,
    ) -> Result<()> {
        let body = json!({ "value": value });
        // ClickUp returns 200 with `{}` on success.
        let _: Value = self
            .post_json_body(&format!("/task/{task_id}/field/{field_id}"), &body)
            .await?;
        Ok(())
    }
}

/// Delta payload for `update_task`. Unset fields are omitted from the request.
#[derive(Debug, Default)]
pub struct TaskUpdate {
    pub description: Option<String>,
    /// User ids to add as assignees.
    pub assignees_add: Vec<i64>,
    /// User ids to remove as assignees.
    pub assignees_rem: Vec<i64>,
    /// Due date as Unix milliseconds; `Some(None)` clears the field.
    pub due_date: Option<Option<i64>>,
    /// Whether the due date carries a meaningful time-of-day. Sent alongside
    /// `due_date` so ClickUp renders date-only instead of inventing a time.
    pub due_date_time: Option<bool>,
}

impl TaskUpdate {
    pub fn to_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        if let Some(ref d) = self.description {
            obj.insert("description".into(), json!(d));
        }
        if !self.assignees_add.is_empty() || !self.assignees_rem.is_empty() {
            obj.insert(
                "assignees".into(),
                json!({
                    "add": self.assignees_add,
                    "rem": self.assignees_rem,
                }),
            );
        }
        if let Some(dd) = &self.due_date {
            obj.insert("due_date".into(), json!(dd));
        }
        if let Some(dt) = self.due_date_time {
            obj.insert("due_date_time".into(), json!(dt));
        }
        Value::Object(obj)
    }
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?;
    let secs = raw.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    const TOKEN: &str = "pk_812345_SECRETSECRETSECRET";

    struct MockResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    }

    fn ok(body: &str) -> MockResponse {
        MockResponse {
            status: 200,
            headers: vec![],
            body: body.to_string(),
        }
    }

    fn too_many(retry_after: &str) -> MockResponse {
        MockResponse {
            status: 429,
            headers: vec![("Retry-After".into(), retry_after.into())],
            body: r#"{"err":"Rate limit reached"}"#.into(),
        }
    }

    /// Minimal one-request-per-connection HTTP responder. `Connection:
    /// close` forces reqwest to reconnect, so each request consumes the next
    /// scripted response in order. Returns (base_url, recorded raw requests).
    async fn spawn_mock(responses: Vec<MockResponse>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        tokio::spawn(async move {
            for resp in responses {
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
                let mut head = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                    resp.status,
                    resp.body.len()
                );
                for (k, v) in &resp.headers {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(resp.body.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    #[tokio::test]
    async fn auth_header_is_raw_token_without_bearer() {
        let (base, seen) = spawn_mock(vec![ok(include_str!("fixtures/user.json"))]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let user = client.get_user().await.unwrap();
        assert_eq!(user.id, Some(81234567));
        assert_eq!(user.username.as_deref(), Some("Felipe"));

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(
            req.contains(&format!("authorization: {TOKEN}"))
                || req.contains(&format!("Authorization: {TOKEN}"))
        );
        assert!(!req.to_lowercase().contains("bearer"));
    }

    #[tokio::test]
    async fn short_page_with_last_page_false_keeps_paging() {
        // Page 0 carries 2 tasks (< page size) but last_page=false; page 1
        // closes with last_page=true.
        let page0 = include_str!("fixtures/tasks_page.json");
        let page1 = r#"{"tasks":[{"id":"86ahwte11","name":"Tail task","list":{"id":"901317020124","name":"Sprint 23","access":true}}],"last_page":true}"#;
        let (base, seen) = spawn_mock(vec![ok(page0), ok(page1)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();

        let tasks = client
            .filter_team_tasks_all("9013000000", &["901312445262".into()], false, &[])
            .await
            .unwrap();
        assert_eq!(tasks.len(), 3);

        let requests = seen.lock().await;
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("page=0"));
        assert!(requests[1].contains("page=1"));
        assert!(requests[0].contains("subtasks=true"));
        assert!(requests[0].contains("include_closed=false"));
        // All-tasks scope: never an assignee filter.
        assert!(!requests[0].contains("assignees"));
    }

    #[tokio::test]
    async fn rate_limit_retries_then_succeeds() {
        let (base, seen) = spawn_mock(vec![
            too_many("0"),
            too_many("0"),
            ok(include_str!("fixtures/user.json")),
        ])
        .await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let user = client.get_user().await.unwrap();
        assert_eq!(user.id, Some(81234567));
        assert_eq!(seen.lock().await.len(), 3);
    }

    #[tokio::test]
    async fn rate_limit_gives_up_after_bounded_retries() {
        let responses = (0..=MAX_RATE_LIMIT_RETRIES)
            .map(|_| too_many("0"))
            .collect();
        let (base, seen) = spawn_mock(responses).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let err = client.get_user().await.unwrap_err();
        assert!(format!("{err:#}").contains("gave up"));
        // Initial attempt + MAX retries.
        assert_eq!(seen.lock().await.len() as u32, MAX_RATE_LIMIT_RETRIES + 1);
    }

    #[tokio::test]
    async fn errors_never_contain_the_token() {
        let (base, _seen) = spawn_mock(vec![MockResponse {
            status: 401,
            headers: vec![],
            body: r#"{"err":"Token invalid","ECODE":"OAUTH_025"}"#.into(),
        }])
        .await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let err = client.get_user().await.unwrap_err();
        assert!(!format!("{err:#}").contains(TOKEN));
        assert!(!format!("{err:?}").contains(TOKEN));

        // Connection-refused path (server consumed): still no token.
        let err = client.get_teams().await.unwrap_err();
        assert!(!format!("{err:#}").contains(TOKEN));
    }

    #[tokio::test]
    async fn invalid_header_token_error_is_redacted() {
        let Err(err) = ClickUpClient::with_base_url("bad\ntoken", "http://127.0.0.1:1") else {
            panic!("expected header error");
        };
        assert!(!format!("{err:#}").contains("bad\ntoken"));
    }

    // ── Write method request-shape tests ──

    #[tokio::test]
    async fn set_task_status_sends_put_with_status_field() {
        // Minimal Task response shape.
        let task_body =
            r#"{"id":"task1","name":"T","status":{"status":"in progress","type":"custom"}}"#;
        let (base, seen) = spawn_mock(vec![ok(task_body)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        client
            .set_task_status("task1", "in progress")
            .await
            .unwrap();

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(
            req.starts_with("PUT /task/task1 "),
            "expected PUT /task/task1, got: {req}"
        );
        assert!(
            req.contains(r#""status":"in progress""#),
            "body missing status field: {req}"
        );
        assert!(
            req.to_lowercase()
                .contains("content-type: application/json")
        );
    }

    #[tokio::test]
    async fn add_comment_sends_post_and_returns_id() {
        let resp_body =
            r#"{"id":"comment99","comment_text":"hello","user":{},"date":1717000000000}"#;
        let (base, seen) = spawn_mock(vec![ok(resp_body)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let id = client.add_comment("task1", "hello").await.unwrap();
        assert_eq!(id, "comment99");

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(
            req.starts_with("POST /task/task1/comment "),
            "expected POST, got: {req}"
        );
        assert!(req.contains(r#""comment_text":"hello""#));
        assert!(req.contains(r#""notify_all":false"#));
    }

    #[tokio::test]
    async fn set_checklist_item_sends_put_to_correct_path() {
        let resp_body = r#"{"checklist":{"id":"cl1","items":[]}}"#;
        let (base, seen) = spawn_mock(vec![ok(resp_body)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        client
            .set_checklist_item("cl1", "item1", true)
            .await
            .unwrap();

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(
            req.starts_with("PUT /checklist/cl1/checklist_item/item1 "),
            "expected PUT to checklist item path, got: {req}"
        );
        assert!(req.contains(r#""resolved":true"#));
    }

    #[tokio::test]
    async fn update_task_sends_delta_assignees() {
        let task_body = r#"{"id":"task1","name":"T"}"#;
        let (base, seen) = spawn_mock(vec![ok(task_body)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let update = TaskUpdate {
            assignees_add: vec![12345],
            assignees_rem: vec![67890],
            ..Default::default()
        };
        client.update_task("task1", &update).await.unwrap();

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(
            req.starts_with("PUT /task/task1 "),
            "expected PUT /task/task1, got: {req}"
        );
        // Delta shape — not a replace.
        assert!(req.contains(r#""assignees""#));
        assert!(req.contains(r#""add""#));
        assert!(req.contains(r#""rem""#));
        // Description not sent when None.
        assert!(!req.contains(r#""description""#));
    }

    #[tokio::test]
    async fn update_task_omits_unset_fields() {
        let task_body = r#"{"id":"task1","name":"T"}"#;
        let (base, seen) = spawn_mock(vec![ok(task_body)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        let update = TaskUpdate {
            description: Some("new desc".into()),
            ..Default::default()
        };
        client.update_task("task1", &update).await.unwrap();

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(req.contains(r#""description":"new desc""#));
        // Assignees and due_date are absent — ClickUp would reset on empty arrays.
        assert!(!req.contains(r#""assignees""#));
        assert!(!req.contains(r#""due_date""#));
    }

    #[tokio::test]
    async fn set_custom_field_sends_post_with_value_wrapper() {
        let resp_body = r#"{}"#;
        let (base, seen) = spawn_mock(vec![ok(resp_body)]).await;
        let client = ClickUpClient::with_base_url(TOKEN, &base).unwrap();
        client
            .set_custom_field("task1", "field-uuid", json!(42))
            .await
            .unwrap();

        let requests = seen.lock().await;
        let req = &requests[0];
        assert!(
            req.starts_with("POST /task/task1/field/field-uuid "),
            "expected POST to field path, got: {req}"
        );
        assert!(req.contains(r#""value":42"#));
    }

    #[tokio::test]
    async fn task_update_to_json_omits_empty_assignees() {
        // Pure helper — no network needed.
        let empty = TaskUpdate::default();
        let j = empty.to_json();
        assert!(!j.as_object().unwrap().contains_key("assignees"));
        assert!(!j.as_object().unwrap().contains_key("due_date"));

        let with_add = TaskUpdate {
            assignees_add: vec![1],
            ..Default::default()
        };
        let j2 = with_add.to_json();
        assert!(j2.get("assignees").is_some());
        assert_eq!(j2["assignees"]["add"][0], 1);
        assert!(j2["assignees"]["rem"].as_array().unwrap().is_empty());
    }
}
