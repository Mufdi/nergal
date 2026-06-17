//! Linear mirror poller: bounded fetch → one-transaction reconcile.
//!
//! Per cycle (design D4/D5): capture the key generation, resolve the viewer
//! (skip on failure, never wipe), fetch teams + the bounded issue scope (window
//! ∪ viewer-assigned ∪ set-3 delta re-verify), then commit in a single
//! transaction that re-reads the generation first and discards the commit if it
//! changed. The `db.lock()` mutex serializes this commit against `set_key`'s
//! wipe, so a deferred transaction is race-free (the mutex subsumes the
//! SQLite-level BEGIN IMMEDIATE concern).
//!
//! Tombstoning is completeness-gated: only a cycle whose every branch reached
//! `hasNextPage == false` may tombstone/evict-on-absence. An interrupted fetch
//! commits upserts only.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use rusqlite::Connection;

use super::client::{BY_ID_CHUNK, LinearClient};
use super::mirror;
use super::model::Issue;

/// One fetched cycle's data — "data in, transaction out", so tests drive
/// `reconcile` with fixtures and no network.
pub struct FetchedCycle {
    pub teams: Vec<super::model::Team>,
    /// Workflow states fetched flat (each carries `team`); see client docs.
    pub states: Vec<super::model::WorkflowState>,
    /// Labels fetched flat (each carries `team`, null for workspace labels).
    pub labels: Vec<super::model::Label>,
    /// Sets 1 + 2 merged and deduped by id (window + viewer-assigned).
    pub scope_issues: Vec<Issue>,
    /// Set-3 by-id results: the delta of currently-mine issues re-fetched.
    pub set3_results: Vec<Issue>,
    /// Ids the mirror believed were the viewer's at cycle start (the set-3 input
    /// domain). An id in this set absent from `set3_results` is "gone".
    pub mine_before: Vec<String>,
    /// True iff every paginated branch reached `hasNextPage == false`.
    pub complete: bool,
    pub cycle_now: i64,
    pub window_start: i64,
    pub selected_team_ids: Vec<String>,
    pub viewer_id: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct ReconcileOutcome {
    /// Discarded because the key generation changed mid-cycle (account swap).
    pub discarded: bool,
    pub tombstoned: usize,
    pub evicted: usize,
    /// Ids newly assigned to the viewer this cycle (post-baseline → notify).
    pub newly_assigned: Vec<String>,
    pub baseline_set: bool,
    /// First error surfaced by a fetch branch (so the daemon shows the real
    /// cause instead of a silent empty list). Data fetched before the error is
    /// still committed.
    pub fetch_error: Option<String>,
}

/// What to do with a set-3 delta id given the re-fetch result + completeness.
#[derive(Debug, PartialEq)]
enum Set3Action {
    /// Returned, still the viewer's, team still selected → keep flag, full upsert.
    RetainMine,
    /// Returned but no longer the viewer's → clear the flag (normal scope rules
    /// then apply, e.g. eviction if aged-out).
    ClearFlag,
    /// Returned but in an unselected team, OR absent on a complete fetch → clear
    /// flag + evict (can't render an unsynced-team issue; absence = deleted).
    ClearAndEvict,
    /// Absent on an INCOMPLETE fetch → retain unchanged (we didn't reach it).
    RetainUnchanged,
}

fn set3_action(
    returned: Option<&Issue>,
    complete: bool,
    viewer_id: &str,
    selected_team_ids: &[String],
) -> Set3Action {
    match returned {
        Some(issue) => {
            let is_mine = issue
                .assignee
                .as_ref()
                .map(|a| a.id == viewer_id)
                .unwrap_or(false);
            let team_selected = selected_team_ids.contains(&issue.team.id);
            if !team_selected {
                Set3Action::ClearAndEvict
            } else if is_mine {
                Set3Action::RetainMine
            } else {
                Set3Action::ClearFlag
            }
        }
        None => {
            if complete {
                Set3Action::ClearAndEvict
            } else {
                Set3Action::RetainUnchanged
            }
        }
    }
}

/// Newly-assigned diff: ids the viewer now owns that the mirror did not flag as
/// theirs before this cycle.
fn newly_assigned_ids(mine_before: &HashSet<String>, mine_now: &HashSet<String>) -> Vec<String> {
    mine_now.difference(mine_before).cloned().collect()
}

/// Upsert one issue and its inline relations into the open transaction. Ensures
/// FK targets exist (placeholder if the hierarchy fetch missed them), then
/// upserts the issue and reconciles its labels.
fn upsert_issue_full(
    conn: &Connection,
    issue: &Issue,
    viewer_id: &str,
    cycle_now: i64,
) -> Result<()> {
    // Team FK: a real team should already be upserted from the teams fetch; if
    // not (out-of-scope team on a viewer-assigned issue), synthesize a stub.
    mirror::ensure_team_placeholder(conn, &issue.team.id, cycle_now)?;
    // Inline relations.
    if let Some(u) = &issue.assignee {
        mirror::upsert_user(conn, u)?;
    }
    if let Some(p) = &issue.project {
        mirror::upsert_project(conn, p)?;
    }
    if let Some(c) = &issue.cycle {
        mirror::upsert_cycle(conn, c, &issue.team.id)?;
    }
    // State FK: ensure it exists (placeholder if the team fetch didn't carry it).
    if let Some(state) = &issue.state {
        mirror::ensure_state_placeholder(conn, &state.id, &issue.team.id)?;
    }
    let is_mine = issue
        .assignee
        .as_ref()
        .map(|a| a.id == viewer_id)
        .unwrap_or(false);
    mirror::upsert_issue(conn, issue, is_mine)?;
    // Labels: upsert defs then reconcile the join.
    let mut label_ids = Vec::with_capacity(issue.labels.nodes.len());
    for l in &issue.labels.nodes {
        mirror::upsert_label(conn, l)?;
        label_ids.push(l.id.clone());
    }
    mirror::reconcile_issue_labels(conn, &issue.id, &label_ids)?;
    Ok(())
}

/// Commit one fetched cycle in a single transaction. Re-reads the key generation
/// first and discards everything if it changed (account swap mid-cycle).
pub fn reconcile(
    conn: &Connection,
    cycle: &FetchedCycle,
    captured_generation: i64,
) -> Result<ReconcileOutcome> {
    let tx = conn.unchecked_transaction()?;
    // Epoch guard: first statement of the transaction.
    let current_gen = mirror::current_key_generation(&tx)?;
    if current_gen != captured_generation {
        // A set_key wiped + bumped while we were fetching — discard this cycle's
        // old-account data rather than re-populate the wiped mirror.
        drop(tx);
        return Ok(ReconcileOutcome {
            discarded: true,
            ..Default::default()
        });
    }

    let mut out = ReconcileOutcome::default();

    // Vocabularies first (FK targets), fetched flat. Teams → states (each names
    // its team) → labels.
    for team in &cycle.teams {
        mirror::upsert_team(&tx, &team.id, &team.name, &team.key, cycle.cycle_now)?;
    }
    for s in &cycle.states {
        if let Some(t) = &s.team {
            // A state for an unfetched team (paging skew) still resolves via a
            // stub team rather than an FK abort.
            mirror::ensure_team_placeholder(&tx, &t.id, cycle.cycle_now)?;
            mirror::upsert_workflow_state(&tx, s, &t.id)?;
        }
    }
    for l in &cycle.labels {
        mirror::upsert_label(&tx, l)?;
    }

    // Scope issues (sets 1 + 2). Track the global fetched-id set for the
    // tombstone-candidate test (moved-between-selected-teams survives).
    let mut fetched_ids: HashSet<String> = HashSet::new();
    for issue in &cycle.scope_issues {
        upsert_issue_full(&tx, issue, &cycle.viewer_id, cycle.cycle_now)?;
        fetched_ids.insert(issue.id.clone());
    }

    // Set-3 delta re-verify. Build a map of returned-by-id, then act per id.
    let returned: HashMap<&str, &Issue> = cycle
        .set3_results
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();
    for id in &cycle.mine_before {
        if fetched_ids.contains(id) {
            // Already surfaced by set 1/2 — set 3 didn't need it.
            continue;
        }
        let action = set3_action(
            returned.get(id.as_str()).copied(),
            cycle.complete,
            &cycle.viewer_id,
            &cycle.selected_team_ids,
        );
        match action {
            Set3Action::RetainMine => {
                if let Some(issue) = returned.get(id.as_str()) {
                    upsert_issue_full(&tx, issue, &cycle.viewer_id, cycle.cycle_now)?;
                    fetched_ids.insert(id.clone());
                }
            }
            Set3Action::ClearFlag => {
                if let Some(issue) = returned.get(id.as_str()) {
                    upsert_issue_full(&tx, issue, &cycle.viewer_id, cycle.cycle_now)?;
                    fetched_ids.insert(id.clone());
                }
                mirror::clear_viewer_assigned(&tx, id)?;
            }
            Set3Action::ClearAndEvict => {
                mirror::clear_viewer_assigned(&tx, id)?;
                mirror::evict_issue(&tx, id)?;
            }
            Set3Action::RetainUnchanged => {}
        }
    }

    // Completeness-gated tombstoning (absence is authoritative only on a
    // complete cycle).
    if cycle.complete {
        out.tombstoned = mirror::tombstone_absent_issues(
            &tx,
            &cycle.selected_team_ids,
            cycle.window_start,
            &fetched_ids,
            cycle.cycle_now,
        )?;
    }

    // Age-out eviction (safe regardless of completeness for non-mine rows).
    out.evicted = mirror::evict_aged_out(&tx, cycle.window_start)?;

    // GC unreferenced vocabulary.
    mirror::gc_unreferenced_labels(&tx)?;
    mirror::gc_unreferenced_synthetic(&tx)?;

    // Compute newly-assigned (for notification) from the committed mine-set.
    let mine_before: HashSet<String> = cycle.mine_before.iter().cloned().collect();
    let mine_now: HashSet<String> = mirror::mine_issue_ids(&tx)?.into_iter().collect();
    out.newly_assigned = newly_assigned_ids(&mine_before, &mine_now);

    // Baseline: set only after a complete sync over a non-empty selection.
    let state = mirror::get_sync_state(&tx)?;
    if cycle.complete && !cycle.selected_team_ids.is_empty() && !state.baseline_done {
        mirror::set_baseline_done(&tx, true)?;
        out.baseline_set = true;
    }
    mirror::set_last_full_sync(&tx, cycle.cycle_now)?;

    tx.commit()?;
    Ok(out)
}

/// Chunk an id list into by-id batches for set-3 re-verify.
pub fn id_chunks(ids: &[String]) -> Vec<Vec<String>> {
    ids.chunks(BY_ID_CHUNK).map(|c| c.to_vec()).collect()
}

/// Paginate a connection-returning fetch to exhaustion. Returns the collected
/// issues, whether it completed (`hasNextPage == false` reached), and the first
/// error string if a page failed. An error yields `complete = false` so the
/// caller skips tombstoning, and the surfaced error lets the daemon show the
/// real cause rather than a silent empty list.
pub async fn paginate<F, Fut>(mut fetch: F) -> (Vec<Issue>, bool, Option<String>)
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<super::model::Connection<Issue>>>,
{
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        match fetch(cursor.clone()).await {
            Ok(conn) => {
                out.extend(conn.nodes);
                if conn.page_info.has_next_page {
                    match conn.page_info.end_cursor {
                        Some(c) => cursor = Some(c),
                        None => return (out, false, None), // hasNext but no cursor
                    }
                } else {
                    return (out, true, None);
                }
            }
            Err(e) => return (out, false, Some(format!("{e:#}"))),
        }
    }
}

/// Format epoch seconds as a Linear-compatible ISO8601 UTC instant for the
/// `updatedAt > x` filter.
pub fn epoch_to_iso8601(epoch_secs: i64) -> String {
    // Inverse of model::iso8601_to_epoch (days-from-civil → civil-from-days).
    let days = epoch_secs.div_euclid(86_400);
    let secs_of_day = epoch_secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.000Z")
}

/// Howard Hinnant's civil-from-days (inverse of model's days_from_civil).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Run one full poll cycle against the live client: viewer resolve → fetch →
/// reconcile. Returns the outcome, or `None` when the cycle was skipped (no
/// viewer, network failure) — never wipes on skip.
pub async fn run_cycle(
    client: &LinearClient,
    conn_lock: &crate::db::SharedDb,
) -> Result<Option<ReconcileOutcome>> {
    // Capture generation + selection + mine-set under the lock.
    let (captured_gen, selected_team_ids, mine_before, stored_viewer) = {
        let guard = conn_lock.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        let st = mirror::get_sync_state(guard.conn())?;
        let mine = mirror::mine_issue_ids(guard.conn())?;
        (st.key_generation, st.selected_team_ids, mine, st.viewer_id)
    };

    // Resolve viewer; a failure SKIPS the cycle and never wipes.
    let viewer = match client.get_viewer().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("linear viewer resolve failed; skipping cycle: {e:#}");
            return Ok(None);
        }
    };
    // Seed the viewer id if the slot is null (post set_key). No "differs → wipe"
    // branch: the epoch already handled the swap.
    if stored_viewer.as_deref() != Some(viewer.id.as_str()) {
        let guard = conn_lock.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        mirror::set_viewer_id(guard.conn(), &viewer.id)?;
    }

    let cycle_now = now_secs();
    // window_days unset or 0 = NO window: poll ALL issues in the selected teams
    // (team selection already bounds the volume, and this matches ClickUp's
    // "all tasks per space" expectation). A positive value bounds the scope for
    // a workspace with very large teams. window_start = 0 (epoch) makes the
    // `updatedAt > x` filter match everything and disables age-out eviction.
    let window_days = crate::config::Config::load()
        .linear_active_window_days
        .unwrap_or(0);
    let window_start = if window_days == 0 {
        0
    } else {
        cycle_now - (window_days as i64) * 86_400
    };
    let updated_after = epoch_to_iso8601(window_start);

    // Vocabularies, fetched flat (separate top-level queries to stay under the
    // per-query complexity cap).
    let teams = client.all_teams().await?;
    let states = client.all_workflow_states().await?;
    let labels = client.all_labels().await?;

    let mut fetch_error: Option<String> = None;

    // Set 1: window. Set 2: viewer-assigned. Both scoped to selected teams.
    let (win_issues, win_complete) = if selected_team_ids.is_empty() {
        (Vec::new(), true)
    } else {
        let (v, c, e) =
            paginate(|after| client.issues_page(&selected_team_ids, &updated_after, after)).await;
        fetch_error = fetch_error.or(e);
        (v, c)
    };
    let (assigned_issues, assigned_complete) = if selected_team_ids.is_empty() {
        (Vec::new(), true)
    } else {
        let (v, c, e) =
            paginate(|after| client.viewer_assigned_issues(&selected_team_ids, &viewer.id, after))
                .await;
        fetch_error = fetch_error.or(e);
        (v, c)
    };

    // Merge + dedup sets 1 + 2.
    let mut seen = HashSet::new();
    let mut scope_issues = Vec::new();
    for issue in win_issues.into_iter().chain(assigned_issues.into_iter()) {
        if seen.insert(issue.id.clone()) {
            scope_issues.push(issue);
        }
    }
    let scope_ids: HashSet<&String> = scope_issues.iter().map(|i| &i.id).collect();

    // Set 3: delta = mine_before − scope. Chunked + paginated by id.
    let delta: Vec<String> = mine_before
        .iter()
        .filter(|id| !scope_ids.contains(*id))
        .cloned()
        .collect();
    let mut set3_results = Vec::new();
    let mut set3_complete = true;
    for chunk in id_chunks(&delta) {
        let (issues, c, e) = paginate(|after| client.issues_by_id(&chunk, after)).await;
        set3_results.extend(issues);
        set3_complete &= c;
        fetch_error = fetch_error.or(e);
    }

    let complete = win_complete && assigned_complete && set3_complete;

    let fetched = FetchedCycle {
        teams,
        states,
        labels,
        scope_issues,
        set3_results,
        mine_before,
        complete,
        cycle_now,
        window_start,
        selected_team_ids,
        viewer_id: viewer.id,
    };

    let mut outcome = {
        let guard = conn_lock.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        reconcile(guard.conn(), &fetched, captured_gen)?
    };
    outcome.fetch_error = fetch_error;
    Ok(Some(outcome))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::model;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/023_linear_mirror.sql"))
            .unwrap();
        conn
    }

    fn issue(id: &str, team: &str, updated: &str, assignee: Option<&str>) -> Issue {
        let mut v = serde_json::json!({
            "id": id, "identifier": "ENG-1", "title": "T",
            "priority": 2, "team": { "id": team }, "updatedAt": updated
        });
        if let Some(a) = assignee {
            v["assignee"] = serde_json::json!({ "id": a });
        }
        serde_json::from_value(v).unwrap()
    }

    fn team(id: &str) -> model::Team {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": "Eng", "key": "ENG",
            "states": { "nodes": [] }, "labels": { "nodes": [] }
        }))
        .unwrap()
    }

    fn base_cycle(viewer: &str) -> FetchedCycle {
        FetchedCycle {
            teams: vec![team("t1")],
            states: vec![],
            labels: vec![],
            scope_issues: vec![],
            set3_results: vec![],
            mine_before: vec![],
            complete: true,
            cycle_now: 2_000_000_000,
            window_start: 1_900_000_000,
            selected_team_ids: vec!["t1".into()],
            viewer_id: viewer.into(),
        }
    }

    #[test]
    fn set3_action_table() {
        let mine = issue("i", "t1", "2026-06-16T00:00:00Z", Some("me"));
        let not_mine = issue("i", "t1", "2026-06-16T00:00:00Z", Some("other"));
        let unsel = issue("i", "tZ", "2026-06-16T00:00:00Z", Some("me"));
        let teams = vec!["t1".to_string()];
        assert_eq!(
            set3_action(Some(&mine), true, "me", &teams),
            Set3Action::RetainMine
        );
        assert_eq!(
            set3_action(Some(&not_mine), true, "me", &teams),
            Set3Action::ClearFlag
        );
        assert_eq!(
            set3_action(Some(&unsel), true, "me", &teams),
            Set3Action::ClearAndEvict
        );
        assert_eq!(
            set3_action(None, true, "me", &teams),
            Set3Action::ClearAndEvict
        );
        assert_eq!(
            set3_action(None, false, "me", &teams),
            Set3Action::RetainUnchanged
        );
    }

    #[test]
    fn complete_cycle_tombstones_absent_in_scope() {
        let conn = mem_db();
        // Pre-seed an in-window non-mine issue that the fetch will omit.
        mirror::upsert_team(&conn, "t1", "Eng", "ENG", 1).unwrap();
        mirror::upsert_issue(
            &conn,
            &issue("gone", "t1", "2033-05-18T00:00:00Z", None),
            false,
        )
        .unwrap();
        let mut cycle = base_cycle("me");
        cycle.scope_issues = vec![issue("here", "t1", "2033-05-18T00:00:00Z", None)];
        let out = reconcile(&conn, &cycle, 0).unwrap();
        assert_eq!(out.tombstoned, 1);
        let stale: i64 = conn
            .query_row("SELECT stale FROM linear_issues WHERE id='gone'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stale, 1);
    }

    #[test]
    fn incomplete_cycle_never_tombstones() {
        let conn = mem_db();
        mirror::upsert_team(&conn, "t1", "Eng", "ENG", 1).unwrap();
        mirror::upsert_issue(
            &conn,
            &issue("gone", "t1", "2033-05-18T00:00:00Z", None),
            false,
        )
        .unwrap();
        let mut cycle = base_cycle("me");
        cycle.complete = false;
        cycle.scope_issues = vec![issue("here", "t1", "2033-05-18T00:00:00Z", None)];
        let out = reconcile(&conn, &cycle, 0).unwrap();
        assert_eq!(out.tombstoned, 0);
        let stale: i64 = conn
            .query_row("SELECT stale FROM linear_issues WHERE id='gone'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stale, 0);
    }

    #[test]
    fn epoch_mismatch_discards_commit() {
        let conn = mem_db();
        let mut cycle = base_cycle("me");
        cycle.scope_issues = vec![issue("i1", "t1", "2033-05-18T00:00:00Z", None)];
        // captured generation 5 ≠ current 0 → discard.
        let out = reconcile(&conn, &cycle, 5).unwrap();
        assert!(out.discarded);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn set3_absent_on_complete_evicts_mine() {
        let conn = mem_db();
        mirror::upsert_team(&conn, "t1", "Eng", "ENG", 1).unwrap();
        // A previously-mine issue, now deleted (absent from set 3), complete cycle.
        // (was_viewer_assigned is set by the flag, not the assignee_id, so no
        // user row is needed.)
        mirror::upsert_issue(
            &conn,
            &issue("mine-deleted", "t1", "2033-05-18T00:00:00Z", None),
            true,
        )
        .unwrap();
        let mut cycle = base_cycle("me");
        cycle.mine_before = vec!["mine-deleted".into()];
        cycle.complete = true;
        let _ = reconcile(&conn, &cycle, 0).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM linear_issues WHERE id='mine-deleted'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn set3_absent_on_incomplete_retains_mine() {
        let conn = mem_db();
        mirror::upsert_team(&conn, "t1", "Eng", "ENG", 1).unwrap();
        mirror::upsert_issue(
            &conn,
            &issue("mine", "t1", "2033-05-18T00:00:00Z", None),
            true,
        )
        .unwrap();
        let mut cycle = base_cycle("me");
        cycle.mine_before = vec!["mine".into()];
        cycle.complete = false;
        let _ = reconcile(&conn, &cycle, 0).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM linear_issues WHERE id='mine'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn baseline_set_only_on_complete_nonempty() {
        let conn = mem_db();
        let cycle = base_cycle("me");
        let out = reconcile(&conn, &cycle, 0).unwrap();
        assert!(out.baseline_set);
        // Zero-team cycle never sets baseline.
        let conn2 = mem_db();
        let mut empty = base_cycle("me");
        empty.selected_team_ids = vec![];
        let out2 = reconcile(&conn2, &empty, 0).unwrap();
        assert!(!out2.baseline_set);
    }

    #[test]
    fn newly_assigned_diff() {
        let before: HashSet<String> = ["a".to_string()].into_iter().collect();
        let now: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
        let mut got = newly_assigned_ids(&before, &now);
        got.sort();
        assert_eq!(got, vec!["b".to_string()]);
    }

    #[test]
    fn moved_between_selected_teams_survives() {
        let conn = mem_db();
        mirror::upsert_team(&conn, "t1", "Eng", "ENG", 1).unwrap();
        mirror::upsert_team(&conn, "t2", "Ops", "OPS", 1).unwrap();
        // Issue lived in t1; now fetched under t2 (moved). Must survive, team=t2.
        mirror::upsert_issue(
            &conn,
            &issue("moved", "t1", "2033-05-18T00:00:00Z", None),
            false,
        )
        .unwrap();
        let mut cycle = base_cycle("me");
        cycle.teams = vec![team("t1"), team("t2")];
        cycle.selected_team_ids = vec!["t1".into(), "t2".into()];
        cycle.scope_issues = vec![issue("moved", "t2", "2033-05-18T00:00:00Z", None)];
        let out = reconcile(&conn, &cycle, 0).unwrap();
        assert_eq!(out.tombstoned, 0);
        let team_id: String = conn
            .query_row(
                "SELECT team_id FROM linear_issues WHERE id='moved'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(team_id, "t2");
    }

    #[test]
    fn iso8601_round_trips_through_epoch() {
        // epoch_to_iso8601 ∘ iso8601_to_epoch identity on a known instant.
        let epoch = 1_609_459_200; // 2021-01-01T00:00:00Z
        let iso = epoch_to_iso8601(epoch);
        assert_eq!(iso, "2021-01-01T00:00:00.000Z");
        assert_eq!(model::iso8601_to_epoch(&iso), Some(epoch));
    }
}
