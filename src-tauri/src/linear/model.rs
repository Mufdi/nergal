//! Serde shapes for Linear's GraphQL responses + the ISO8601→epoch helper.
//!
//! Linear returns nested relations inline, so one issues page brings each
//! issue's state/assignee/labels/project/cycle/parent without a per-issue
//! fan-out. All relation fields are `Option`/empty-tolerant: a Linear issue can
//! legitimately have a null assignee/project/cycle/parent. Dates arrive as
//! ISO8601 `DateTime` strings (UTC, `Z`); the mirror stores epoch seconds, so
//! `iso8601_to_epoch` converts at the boundary.

use serde::Deserialize;

/// Relay cursor connection wrapper. `nodes` plus `pageInfo` drives pagination.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection<T> {
    #[serde(default = "Vec::new")]
    pub nodes: Vec<T>,
    #[serde(default)]
    pub page_info: PageInfo,
}

impl<T> Default for Connection<T> {
    fn default() -> Self {
        Connection {
            nodes: Vec::new(),
            page_info: PageInfo::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    #[serde(default)]
    pub has_next_page: bool,
    #[serde(default)]
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Viewer {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub states: Connection<WorkflowState>,
    #[serde(default)]
    pub labels: Connection<Label>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowState {
    pub id: String,
    pub name: String,
    /// Native Linear state type: triage/backlog/unstarted/started/completed/canceled.
    #[serde(rename = "type")]
    pub state_type: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub position: Option<f64>,
    /// Owning team — present when fetched from the top-level `workflowStates`
    /// query (states are queried flat, not nested under teams, to stay under the
    /// per-query complexity cap).
    #[serde(default)]
    pub team: Option<IdRef>,
}

#[derive(Debug, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    /// Workspace labels have no team; team labels carry a ref.
    #[serde(default)]
    pub team: Option<IdRef>,
}

// camelCase: Linear sends `displayName`/`avatarUrl`. Without this the queries'
// camelCase fields silently never populated display_name/avatar_url (they fell
// back to `name`/initials) — root-caused while adding the activity feed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Cycle {
    pub id: String,
    #[serde(default)]
    pub number: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
}

/// A bare `{ id }` reference (used for `parent`, `team` on a label, etc.).
#[derive(Debug, Deserialize)]
pub struct IdRef {
    pub id: String,
}

/// Full nested issue — the shape returned by both the window/assigned pages and
/// the set-3 by-id re-verify (so the set-3 upsert reconciles labels, not strips
/// them).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    #[serde(default)]
    pub identifier: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Linear priority: 0=none, 1=urgent, 2=high, 3=medium, 4=low.
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub estimate: Option<f64>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    pub team: IdRef,
    /// State carries only its id; the full row comes from the team's `states`
    /// connection (upserted before issues, so the FK resolves).
    #[serde(default)]
    pub state: Option<IdRef>,
    /// Assignee/project/cycle are fetched inline in full, so the poller
    /// populates `linear_users`/`linear_projects`/`linear_cycles` from the
    /// issues themselves — no separate fetch.
    #[serde(default)]
    pub assignee: Option<User>,
    #[serde(default)]
    pub project: Option<Project>,
    #[serde(default)]
    pub cycle: Option<Cycle>,
    /// Parent carries only its id; the tree is built in app, tolerant of a
    /// dangling (out-of-scope) parent.
    #[serde(default)]
    pub parent: Option<IdRef>,
    #[serde(default)]
    pub labels: Connection<Label>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub user: Option<User>,
}

/// Issue attachment (link, file ref) — fetched lazily on detail-open, not
/// persisted (like comments).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    pub url: String,
}

/// A typed relation from this issue to another (blocks/related/duplicate/…).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    #[serde(rename = "type")]
    pub relation_type: String,
    #[serde(default)]
    pub related_issue: Option<RelatedIssue>,
}

#[derive(Debug, Deserialize)]
pub struct RelatedIssue {
    pub id: String,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// One issue-history (activity) entry — fetched lazily on detail-open, not
/// persisted. Only the fields the activity feed renders are deserialized.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub actor: Option<User>,
    #[serde(default)]
    pub bot_actor: Option<BotActor>,
    #[serde(default)]
    pub from_state: Option<NamedRef>,
    #[serde(default)]
    pub to_state: Option<NamedRef>,
    #[serde(default)]
    pub from_assignee: Option<User>,
    #[serde(default)]
    pub to_assignee: Option<User>,
    #[serde(default)]
    pub added_label_ids: Option<Vec<String>>,
    #[serde(default)]
    pub removed_label_ids: Option<Vec<String>>,
    #[serde(default)]
    pub from_cycle: Option<CycleRef>,
    #[serde(default)]
    pub to_cycle: Option<CycleRef>,
    #[serde(default)]
    pub from_priority: Option<i64>,
    #[serde(default)]
    pub to_priority: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct BotActor {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NamedRef {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleRef {
    #[serde(default)]
    pub number: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Parse a Linear ISO8601 `DateTime` (`YYYY-MM-DDTHH:MM:SS[.fff]Z`, UTC) into
/// epoch seconds. Returns `None` for a shape we can't read rather than guessing.
/// Dependency-free: uses Howard Hinnant's days-from-civil algorithm.
pub fn iso8601_to_epoch(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        // Linear can also send a bare date (`due_date` is a Date, not DateTime):
        // YYYY-MM-DD with no time component.
        if s.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-' {
            let y = s.get(0..4)?.parse::<i64>().ok()?;
            let mo = s.get(5..7)?.parse::<u32>().ok()?;
            let d = s.get(8..10)?.parse::<u32>().ok()?;
            return Some(days_from_civil(y, mo, d) * 86_400);
        }
        return None;
    }
    let year = s.get(0..4)?.parse::<i64>().ok()?;
    let month = s.get(5..7)?.parse::<u32>().ok()?;
    let day = s.get(8..10)?.parse::<u32>().ok()?;
    let hour = s.get(11..13)?.parse::<i64>().ok()?;
    let min = s.get(14..16)?.parse::<i64>().ok()?;
    let sec = s.get(17..19)?.parse::<i64>().ok()?;
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + min * 60 + sec)
}

/// Days since 1970-01-01 for a proleptic-Gregorian Y-M-D. Howard Hinnant's
/// algorithm; valid for the full range we'll ever see.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_of_unix_epoch_is_zero() {
        assert_eq!(iso8601_to_epoch("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn epoch_of_known_instant() {
        // 2021-01-01T00:00:00Z = 1609459200
        assert_eq!(
            iso8601_to_epoch("2021-01-01T00:00:00Z"),
            Some(1_609_459_200)
        );
    }

    #[test]
    fn epoch_handles_fractional_seconds() {
        // Fractional part ignored; seconds floor.
        assert_eq!(
            iso8601_to_epoch("2021-01-01T00:00:00.789Z"),
            Some(1_609_459_200)
        );
    }

    #[test]
    fn epoch_of_bare_date() {
        assert_eq!(iso8601_to_epoch("2021-01-01"), Some(1_609_459_200));
    }

    #[test]
    fn epoch_of_garbage_is_none() {
        assert_eq!(iso8601_to_epoch("not-a-date"), None);
        assert_eq!(iso8601_to_epoch(""), None);
    }

    #[test]
    fn issue_parses_with_null_relations() {
        let json = r#"{
            "id": "iss1", "identifier": "ENG-1", "title": "T",
            "priority": 2, "team": { "id": "team1" }
        }"#;
        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.id, "iss1");
        assert_eq!(issue.priority, Some(2));
        assert!(issue.assignee.is_none());
        assert!(issue.project.is_none());
        assert!(issue.labels.nodes.is_empty());
    }

    #[test]
    fn workflow_state_parses_flat_with_team() {
        let json = r##"{ "id":"s1","name":"In Progress","type":"started","color":"#fff","team":{"id":"t1"} }"##;
        let s: WorkflowState = serde_json::from_str(json).unwrap();
        assert_eq!(s.state_type, "started");
        assert_eq!(s.team.unwrap().id, "t1");
    }

    #[test]
    fn connection_paginates_shape() {
        let json = r#"{
            "nodes": [{"id":"iss1","title":"T","team":{"id":"t1"}}],
            "pageInfo": { "hasNextPage": true, "endCursor": "abc" }
        }"#;
        let conn: Connection<Issue> = serde_json::from_str(json).unwrap();
        assert_eq!(conn.nodes.len(), 1);
        assert!(conn.page_info.has_next_page);
        assert_eq!(conn.page_info.end_cursor.as_deref(), Some("abc"));
    }
}
