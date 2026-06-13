//! Serde payload types for the ClickUp API v2 read surface.
//!
//! All structs tolerate unknown fields (no `deny_unknown_fields`) and default
//! missing optionals, because ClickUp's shapes drift and several numeric-ish
//! fields arrive as strings (`date_updated: "1717090000000"`, `orderindex:
//! "1"`, `reply_count: "2"`). Lenient deserializers normalize those; only
//! entity ids are required, since a payload without an id is unusable for
//! the mirror anyway.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Accepts a JSON string or number, yields the string form. Ids show up both
/// ways across endpoints (`team_id: "9013"` vs `creator: 81234567`).
fn de_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        other => Err(serde::de::Error::custom(format!(
            "expected string or number, got {other:?}"
        ))),
    }
}

fn de_opt_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<Value>::deserialize(d)?.and_then(|v| match v {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }))
}

fn de_opt_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<Value>::deserialize(d)?.and_then(|v| match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let t = s.trim();
            t.parse::<i64>()
                .ok()
                .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
        }
        _ => None,
    }))
}

fn de_opt_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    Ok(Option::<Value>::deserialize(d)?.and_then(|v| match v {
        Value::Bool(b) => Some(b),
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        Value::String(s) => match s.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }))
}

/// Compact `{ id, name, hidden, access }` reference embedded in tasks and
/// lists (`task.list`, `task.folder`, `list.folder`, `list.space`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NamedRef {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub hidden: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub access: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct User {
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub id: Option<i64>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub initials: Option<String>,
    #[serde(default, rename = "profilePicture")]
    pub profile_picture: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Team {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Status {
    #[serde(default, deserialize_with = "de_opt_string")]
    pub id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub orderindex: Option<i64>,
    #[serde(default, rename = "type")]
    pub status_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Space {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub private: Option<bool>,
    #[serde(default)]
    pub statuses: Vec<Status>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub multiple_assignees: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Folder {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub space: Option<NamedRef>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub task_count: Option<i64>,
    #[serde(default)]
    pub lists: Vec<List>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct List {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Folderless lists carry a synthetic folder with `hidden: true`; the
    /// hierarchy fetch's nested lists omit it (the parent Folder is implied).
    #[serde(default)]
    pub folder: Option<NamedRef>,
    #[serde(default)]
    pub space: Option<NamedRef>,
    /// Inline per-List status workflow — the poll's only status source.
    #[serde(default)]
    pub statuses: Vec<Status>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub task_count: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub override_statuses: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Tag {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tag_fg: Option<String>,
    #[serde(default)]
    pub tag_bg: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Priority {
    #[serde(default, deserialize_with = "de_opt_string")]
    pub id: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CustomField {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub type_config: Option<Value>,
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChecklistItem {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub resolved: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub orderindex: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Checklist {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default, deserialize_with = "de_opt_string")]
    pub task_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub orderindex: Option<i64>,
    #[serde(default)]
    pub items: Vec<ChecklistItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Attachment {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub mimetype: Option<String>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub size: Option<i64>,
    #[serde(default)]
    pub extension: Option<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub thumbnail_small: Option<String>,
    #[serde(default)]
    pub thumbnail_medium: Option<String>,
    #[serde(default)]
    pub thumbnail_large: Option<String>,
}

impl Attachment {
    /// Best thumbnail for the detail view; the mirror stores one URL.
    pub fn best_thumbnail(&self) -> Option<&str> {
        self.thumbnail_url
            .as_deref()
            .or(self.thumbnail_medium.as_deref())
            .or(self.thumbnail_small.as_deref())
            .or(self.thumbnail_large.as_deref())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Comment {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    #[serde(default)]
    pub comment_text: Option<String>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub resolved: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub date: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub reply_count: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Task {
    #[serde(deserialize_with = "de_string")]
    pub id: String,
    /// Human-readable workspace identifier (e.g. "DEV-142"); only present when
    /// the workspace enabled custom task ids — falls back to `id` for display.
    #[serde(default, deserialize_with = "de_opt_string")]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text_content: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<Status>,
    /// Flat parent id from the all-tasks fetch (`subtasks=true`) — the sole
    /// source for the mirror's subtask tree.
    #[serde(default, deserialize_with = "de_opt_string")]
    pub parent: Option<String>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default)]
    pub creator: Option<User>,
    #[serde(default)]
    pub assignees: Vec<User>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub checklists: Vec<Checklist>,
    #[serde(default)]
    pub custom_fields: Vec<CustomField>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub due_date: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub start_date: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub date_created: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub date_updated: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub date_closed: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_bool")]
    pub archived: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_string")]
    pub team_id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub list: Option<NamedRef>,
    #[serde(default)]
    pub folder: Option<NamedRef>,
    #[serde(default)]
    pub space: Option<NamedRef>,
    /// Nested array returned only by the per-task detail call with
    /// `include_subtasks` — detail rendering only, never the reconcile tree.
    #[serde(default)]
    pub subtasks: Vec<Task>,
}

// ── Response envelopes ──

#[derive(Debug, Deserialize)]
pub struct UserResponse {
    pub user: User,
}

#[derive(Debug, Deserialize)]
pub struct TeamsResponse {
    #[serde(default)]
    pub teams: Vec<Team>,
}

#[derive(Debug, Deserialize)]
pub struct SpacesResponse {
    #[serde(default)]
    pub spaces: Vec<Space>,
}

#[derive(Debug, Deserialize)]
pub struct FoldersResponse {
    #[serde(default)]
    pub folders: Vec<Folder>,
}

#[derive(Debug, Deserialize)]
pub struct ListsResponse {
    #[serde(default)]
    pub lists: Vec<List>,
}

#[derive(Debug, Deserialize)]
pub struct CommentsResponse {
    #[serde(default)]
    pub comments: Vec<Comment>,
}

/// One page of the filtered team tasks endpoint. `last_page` is the ONLY
/// pagination terminator: the endpoint filters after the page slice, so a
/// short page does not mean the last page.
#[derive(Debug, Deserialize)]
pub struct TasksPage {
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub last_page: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_fixture_parses() {
        let resp: UserResponse = serde_json::from_str(include_str!("fixtures/user.json")).unwrap();
        assert_eq!(resp.user.id, Some(81234567));
        assert_eq!(resp.user.username.as_deref(), Some("Felipe"));
        assert_eq!(resp.user.email.as_deref(), Some("felipe@example.com"));
    }

    #[test]
    fn teams_fixture_parses() {
        let resp: TeamsResponse =
            serde_json::from_str(include_str!("fixtures/teams.json")).unwrap();
        assert_eq!(resp.teams.len(), 1);
        assert_eq!(resp.teams[0].id, "9013000000");
        assert_eq!(resp.teams[0].name, "Mufdi Workspace");
    }

    #[test]
    fn spaces_fixture_parses_with_inline_statuses() {
        let resp: SpacesResponse =
            serde_json::from_str(include_str!("fixtures/spaces.json")).unwrap();
        assert_eq!(resp.spaces.len(), 2);
        let producto = &resp.spaces[0];
        assert_eq!(producto.id, "901312445262");
        assert_eq!(producto.statuses.len(), 2);
        // Status objects may omit `id`.
        assert!(producto.statuses[0].id.is_some());
        assert!(producto.statuses[1].id.is_none());
        assert_eq!(producto.statuses[1].status_type.as_deref(), Some("closed"));
    }

    #[test]
    fn folders_fixture_parses_with_nested_lists_and_string_numbers() {
        let resp: FoldersResponse =
            serde_json::from_str(include_str!("fixtures/folders.json")).unwrap();
        let folder = &resp.folders[0];
        assert_eq!(folder.hidden, Some(false));
        // task_count arrives as a string here.
        assert_eq!(folder.task_count, Some(12));
        let list = &folder.lists[0];
        assert_eq!(list.statuses.len(), 3);
        // orderindex "1" (string) normalized.
        assert_eq!(list.statuses[1].orderindex, Some(1));
        assert_eq!(list.statuses[1].status, "en revisión - dev (pr)");
    }

    #[test]
    fn folderless_lists_fixture_carries_hidden_folder() {
        let resp: ListsResponse =
            serde_json::from_str(include_str!("fixtures/folderless_lists.json")).unwrap();
        let list = &resp.lists[0];
        let folder = list.folder.as_ref().unwrap();
        assert_eq!(folder.hidden, Some(true));
        assert_eq!(list.statuses.len(), 2);
    }

    #[test]
    fn tasks_page_fixture_parses_full_shape() {
        let page: TasksPage =
            serde_json::from_str(include_str!("fixtures/tasks_page.json")).unwrap();
        assert_eq!(page.last_page, Some(false));
        assert_eq!(page.tasks.len(), 2);

        let task = &page.tasks[0];
        assert_eq!(task.id, "86ahwtc67");
        // String epoch-millis normalized to i64.
        assert_eq!(task.date_updated, Some(1_717_090_000_000));
        assert_eq!(task.due_date, Some(1_717_500_000_000));
        assert_eq!(task.parent, None);
        assert_eq!(task.assignees.len(), 1);
        assert_eq!(task.assignees[0].id, Some(81234567));
        assert_eq!(
            task.status.as_ref().map(|s| s.status.as_str()),
            Some("en revisión - dev (pr)")
        );
        assert_eq!(
            task.priority.as_ref().and_then(|p| p.priority.as_deref()),
            Some("high")
        );
        assert_eq!(task.custom_fields.len(), 2);
        assert_eq!(task.custom_fields[0].field_type, "drop_down");
        assert!(task.custom_fields[0].value.is_some());
        assert_eq!(task.checklists.len(), 1);
        assert_eq!(task.checklists[0].items.len(), 2);
        assert_eq!(task.checklists[0].items[0].resolved, Some(true));
        // orderindex "1" (string) on the second item.
        assert_eq!(task.checklists[0].items[1].orderindex, Some(1));
        assert_eq!(task.attachments.len(), 1);
        assert!(
            task.attachments[0]
                .best_thumbnail()
                .unwrap()
                .contains("medium")
        );
        assert_eq!(task.attachments[0].mimetype.as_deref(), Some("image/png"));
        assert_eq!(
            task.list.as_ref().map(|l| l.id.as_str()),
            Some("901317020124")
        );

        // Subtask comes flat with a `parent` pointer.
        let sub = &page.tasks[1];
        assert_eq!(sub.parent.as_deref(), Some("86ahwtc67"));
        assert!(sub.status.as_ref().unwrap().id.is_none());
    }

    #[test]
    fn comments_fixture_parses_with_lenient_counters() {
        let resp: CommentsResponse =
            serde_json::from_str(include_str!("fixtures/comments.json")).unwrap();
        assert_eq!(resp.comments.len(), 2);
        // reply_count "2" (string) and date "1717050000000" (string).
        assert_eq!(resp.comments[0].reply_count, Some(2));
        assert_eq!(resp.comments[0].date, Some(1_717_050_000_000));
        // date numeric on the second comment.
        assert_eq!(resp.comments[1].date, Some(1_717_060_000_000));
        assert_eq!(resp.comments[1].resolved, Some(true));
    }

    #[test]
    fn unknown_fields_and_id_shapes_are_tolerated() {
        // Numeric id, unknown keys, junk types in lenient slots.
        let task: Task = serde_json::from_str(
            r#"{
                "id": 12345,
                "name": "n",
                "brand_new_field": {"deep": true},
                "due_date": "not-a-number",
                "archived": "1",
                "parent": 999
            }"#,
        )
        .unwrap();
        assert_eq!(task.id, "12345");
        assert_eq!(task.due_date, None);
        assert_eq!(task.archived, Some(true));
        assert_eq!(task.parent.as_deref(), Some("999"));
    }
}
