//! ClickUp mirror poller: interval fetch → one-transaction reconcile.
//!
//! Each cycle fetches everything it needs FIRST (hierarchy with statuses
//! inline on the List objects + all tasks per Space paginated to
//! exhaustion), then commits the mirror in a single SQLite transaction with
//! the FK-safe order `spaces → folders → lists → statuses → tasks →
//! subdata`. A mid-cycle network failure therefore commits nothing and the
//! prior mirror stays intact (Decision 6).
//!
//! The reconcile core (`reconcile_team`) is a pure-ish function — fetched
//! data in, transaction out — so tests drive it with fixture payloads and
//! capture side effects through the `SyncEffects` trait instead of needing
//! the interval loop or a Tauri `AppHandle`.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use tauri::{AppHandle, Emitter, Manager};

use super::client::ClickUpClient;
use super::writeback::{self, EchoCheckResult, WriteConflict};
use super::{auth, mirror, model};

pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 45;

/// Tombstoned rows briefly back the panel's "where did that task go"
/// affordance, but the mirror must not accumulate dead rows forever — a
/// week comfortably outlives any such investigation while keeping the
/// stale-aware queries from scanning unbounded garbage.
pub const STALE_RETENTION_SECS: i64 = 7 * 24 * 60 * 60;

/// Synthetic container ids embed a `:` (impossible in real ClickUp ids) so
/// the hierarchy-tombstone sweep can recognize and exempt placeholders —
/// they were never in a hierarchy fetch by definition (Decision 6).
pub const PLACEHOLDER_FOLDER_PREFIX: &str = "nergal:placeholder:";

/// Cap on poll-driven comment refreshes per cycle so a burst of edited
/// tasks cannot blow the rate budget; the remainder catches up on later
/// cycles or on detail-open.
const SUBDATA_REFRESH_CAP: usize = 5;

// ── Fetched cycle (everything gathered before any DB write) ──

#[derive(Debug, Clone)]
pub struct SpaceFetch {
    pub space: model::Space,
    pub folders: Vec<model::Folder>,
    pub folderless_lists: Vec<model::List>,
}

#[derive(Debug, Clone)]
pub struct FetchedCycle {
    pub team_id: String,
    pub spaces: Vec<SpaceFetch>,
    pub tasks: Vec<model::Task>,
}

/// Gather one cycle's worth of data without touching the DB. Statuses ride
/// inline on each List object — no per-List status call. The task fetch is
/// all-tasks scope (`subtasks=true`, `include_closed=false`, no assignee
/// filter — Decision 4) paginated to exhaustion via `last_page`.
pub async fn fetch_cycle(client: &ClickUpClient, team_id: &str) -> Result<FetchedCycle> {
    let spaces = client.get_spaces(team_id).await?;
    let mut space_fetches = Vec::with_capacity(spaces.len());
    let mut space_ids = Vec::with_capacity(spaces.len());
    for space in spaces {
        let folders = client.get_folders(&space.id).await?;
        let folderless_lists = client.get_folderless_lists(&space.id).await?;
        space_ids.push(space.id.clone());
        space_fetches.push(SpaceFetch {
            space,
            folders,
            folderless_lists,
        });
    }
    let tasks = if space_ids.is_empty() {
        Vec::new()
    } else {
        client
            .filter_team_tasks_all(team_id, &space_ids, false, &[])
            .await?
    };
    Ok(FetchedCycle {
        team_id: team_id.to_string(),
        spaces: space_fetches,
        tasks,
    })
}

/// Stable hash of the fetched payload. The poller compares it against the
/// previous cycle's to decide whether the committed reconcile "changed
/// anything" worth a `clickup:changed` emit — tombstones derive from the
/// fetch + prior mirror, so an identical fetch implies an idle cycle.
pub fn fingerprint(fetched: &FetchedCycle) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut lines: Vec<String> = Vec::new();
    for sf in &fetched.spaces {
        lines.push(format!("s|{}|{}", sf.space.id, sf.space.name));
        for folder in &sf.folders {
            lines.push(format!(
                "f|{}|{}|{}",
                folder.id,
                folder.name,
                folder.hidden.unwrap_or(false)
            ));
            for list in &folder.lists {
                lines.push(list_line(list));
            }
        }
        for list in &sf.folderless_lists {
            lines.push(list_line(list));
        }
    }
    for task in &fetched.tasks {
        let assignees: Vec<String> = task
            .assignees
            .iter()
            .map(|u| u.id.unwrap_or_default().to_string())
            .collect();
        lines.push(format!(
            "t|{}|{}|{:?}|{:?}|{:?}|{:?}|{}",
            task.id,
            task.name,
            task.list.as_ref().map(|l| &l.id),
            task.parent,
            task.status.as_ref().map(|s| &s.status),
            task.date_updated,
            assignees.join(",")
        ));
    }
    lines.sort();
    let mut hasher = std::hash::DefaultHasher::new();
    lines.hash(&mut hasher);
    hasher.finish()
}

fn list_line(list: &model::List) -> String {
    let statuses: Vec<String> = list
        .statuses
        .iter()
        .map(|s| format!("{}:{}", s.status, s.color.as_deref().unwrap_or("")))
        .collect();
    format!("l|{}|{}|{}", list.id, list.name, statuses.join(";"))
}

// ── Reconcile (the atomic core) ──

#[derive(Debug, Default)]
pub struct ReconcileOutcome {
    /// False during the silent baseline seed (Decision 7): `newly_assigned`
    /// is still computed but must not be surfaced.
    pub notifications_armed: bool,
    /// Names of tasks newly assigned to the token user this cycle.
    pub newly_assigned: Vec<String>,
    pub gc_removed: usize,
    /// Tasks whose comments are already materialized locally and whose
    /// `date_updated` advanced — the only poll-driven heavy-subdata refresh
    /// trigger (Decision 5).
    pub refresh_subdata: Vec<String>,
    /// Scalar-field conflicts detected this cycle (emitted as
    /// `clickup:write-conflict`). Empty when no registry is supplied or
    /// during the baseline seed.
    pub write_conflicts: Vec<WriteConflict>,
}

/// Commit one fetched cycle to the mirror in a single transaction. Order:
/// spaces → folders → lists → statuses → hierarchy tombstones → tasks
/// (two-pass for the `parent_id` FK) → subdata → task tombstones → GC →
/// sync state. Any error rolls the whole cycle back.
///
/// `registry` is consulted for echo/conflict before new-assignment detection
/// (cross-change ordering guarantee — Risk §10, design.md). Pass `None` in
/// tests that do not need echo logic.
pub fn reconcile_team(
    conn: &Connection,
    fetched: &FetchedCycle,
    me: Option<i64>,
    now: i64,
    registry: Option<&writeback::WritebackRegistry>,
) -> Result<ReconcileOutcome> {
    let tx = conn.unchecked_transaction()?;

    let prior = mirror::get_sync_state(&tx, &fetched.team_id)?.unwrap_or_default();
    let notifications_armed = prior.baseline_done;
    let prev_assigned = match me {
        Some(id) => assigned_task_ids(&tx, id)?,
        None => HashSet::new(),
    };
    let prior_tasks = prior_task_meta(&tx)?;

    for sf in &fetched.spaces {
        mirror::upsert_space(&tx, &sf.space, now)?;
    }
    for sf in &fetched.spaces {
        for folder in &sf.folders {
            mirror::upsert_folder(
                &tx,
                &folder.id,
                &sf.space.id,
                &folder.name,
                folder.hidden.unwrap_or(false),
            )?;
            for list in &folder.lists {
                // Hierarchy-nested lists omit `folder`; bind to the parent
                // payload so the mirror keeps the real containment.
                let mut bound = list.clone();
                bound.folder = Some(model::NamedRef {
                    id: folder.id.clone(),
                    name: folder.name.clone(),
                    hidden: folder.hidden,
                    access: None,
                });
                upsert_list_with_statuses(&tx, &bound, &sf.space.id)?;
            }
        }
        for list in &sf.folderless_lists {
            // Folderless lists carry their own synthetic `hidden:true`
            // folder ref; mirror it so the FK holds.
            if let Some(folder) = &list.folder {
                mirror::upsert_folder(
                    &tx,
                    &folder.id,
                    &sf.space.id,
                    &folder.name,
                    folder.hidden.unwrap_or(true),
                )?;
            }
            upsert_list_with_statuses(&tx, list, &sf.space.id)?;
        }
    }

    tombstone_absent_hierarchy(&tx, fetched, now)?;
    upsert_tasks(&tx, fetched, now)?;
    tombstone_absent_tasks(&tx, fetched, now)?;

    // Echo + conflict check MUST run before newly_assigned_names so an own
    // assignment-write does not self-notify (cross-change ordering, Risk §10).
    let (own_echo_task_ids, write_conflicts) = match registry {
        Some(reg) => {
            reg.purge_expired();
            run_echo_check(fetched, reg)
        }
        None => (HashSet::new(), Vec::new()),
    };

    let newly_assigned = match me {
        Some(id) => newly_assigned_names(fetched, id, &prev_assigned, &own_echo_task_ids),
        None => Vec::new(),
    };
    let refresh_subdata = subdata_refresh_candidates(fetched, &prior_tasks);
    let gc_removed = gc_stale_rows(&tx, now - STALE_RETENTION_SECS)?;

    mirror::upsert_sync_state(
        &tx,
        &fetched.team_id,
        &mirror::SyncState {
            baseline_done: true,
            last_full_sync: Some(now),
        },
    )?;
    tx.commit()?;

    Ok(ReconcileOutcome {
        notifications_armed,
        newly_assigned,
        gc_removed,
        refresh_subdata,
        write_conflicts,
    })
}

/// Compare each fetched task against the registry.  Returns:
/// - `own_echo_task_ids`: task ids where at least one field matched the
///   written value (echo confirmed).  These are excluded from the
///   new-assignment check to prevent self-notification.
/// - `write_conflicts`: scalar conflicts to emit.
///
/// Echo entries are cleared from the registry immediately on confirmation;
/// additive divergences are silently accepted (merge semantics).
fn run_echo_check(
    fetched: &FetchedCycle,
    registry: &writeback::WritebackRegistry,
) -> (HashSet<String>, Vec<WriteConflict>) {
    let mut own_echo_ids: HashSet<String> = HashSet::new();
    let mut conflicts: Vec<WriteConflict> = Vec::new();

    for task in &fetched.tasks {
        let entries = registry.entries_for_task(&task.id);
        if entries.is_empty() {
            continue;
        }
        for entry in &entries {
            let server_value = task_field_value(task, &entry.field);
            let Some(server_value) = server_value else {
                continue;
            };
            match writeback::check_echo(entry, &server_value) {
                EchoCheckResult::OwnEcho => {
                    registry.clear_entry(&task.id, &entry.field);
                    own_echo_ids.insert(task.id.clone());
                    tracing::debug!(
                        task = %task.id,
                        field = ?entry.field,
                        "clickup echo confirmed: own write reflected, suppressing notification"
                    );
                }
                EchoCheckResult::ScalarConflict(conflict) => {
                    tracing::warn!(
                        task = %task.id,
                        field = ?entry.field,
                        your_value = %entry.written_value,
                        remote_value = %server_value,
                        "clickup write conflict: remote value supersedes local write"
                    );
                    conflicts.push(conflict);
                    // Clear entry: conflict is surfaced once, then the server
                    // value is the new truth via normal reconcile.
                    registry.clear_entry(&task.id, &entry.field);
                }
                EchoCheckResult::AdditiveDivergence => {
                    // Additive fields merge server-side; the mirror lands the
                    // merged state via normal upsert.  No warning.
                    registry.clear_entry(&task.id, &entry.field);
                    tracing::debug!(
                        task = %task.id,
                        field = ?entry.field,
                        "clickup additive field diverged; merged to server state silently"
                    );
                }
                EchoCheckResult::Unrelated => {}
            }
        }
    }
    (own_echo_ids, conflicts)
}

/// Extract the canonical string value for a field from a fetched task payload.
/// Returns `None` when the field is not present on this task (e.g. a custom
/// field not configured for this task).
fn task_field_value(task: &model::Task, field: &writeback::WriteField) -> Option<String> {
    match field {
        writeback::WriteField::Status => task.status.as_ref().map(|s| s.status.clone()),
        writeback::WriteField::Description => task.text_content.clone(),
        writeback::WriteField::DueDate => task.due_date.map(|d| d.to_string()),
        writeback::WriteField::Assignees => {
            // Canonical form: sorted comma-separated integer ids.  Sorting
            // makes comparison order-independent, which matters because
            // ClickUp returns assignees in arbitrary order and the write
            // command records add-only deltas (not the full list).
            let mut ids: Vec<i64> = task.assignees.iter().filter_map(|u| u.id).collect();
            ids.sort_unstable();
            Some(
                ids.iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            )
        }
        writeback::WriteField::ChecklistItem(key) => {
            // key = "{checklist_id}:{item_id}"
            let (checklist_id, item_id) = key.split_once(':')?;
            let checklist = task.checklists.iter().find(|c| c.id == checklist_id)?;
            let item = checklist.items.iter().find(|i| i.id == item_id)?;
            Some(
                if item.resolved.unwrap_or(false) {
                    "true"
                } else {
                    "false"
                }
                .into(),
            )
        }
        writeback::WriteField::CustomField(field_id) => {
            let field = task.custom_fields.iter().find(|f| &f.id == field_id)?;
            field.value.as_ref().map(|v| v.to_string())
        }
    }
}

fn upsert_list_with_statuses(tx: &Connection, list: &model::List, space_id: &str) -> Result<()> {
    mirror::upsert_list(tx, list, space_id)?;
    for status in &list.statuses {
        mirror::upsert_status(tx, &list.id, status)?;
    }
    Ok(())
}

/// Two-pass task upsert: pass 1 inserts every task with `parent` cleared
/// (a flat page can deliver a subtask before its parent, which would trip
/// the `parent_id` FK), pass 2 links parents now that every row exists.
/// The tree source is solely the flat `parent` field (Decision 8).
fn upsert_tasks(tx: &Connection, fetched: &FetchedCycle, now: i64) -> Result<()> {
    let mut known_lists = collect_ids(tx, "SELECT id FROM clickup_lists")?;
    let mut known_spaces = collect_ids(tx, "SELECT id FROM clickup_spaces")?;

    for task in &fetched.tasks {
        let Some(list_ref) = task.list.as_ref() else {
            tracing::warn!(task = %task.id, "clickup task without list reference; skipping");
            continue;
        };
        if !known_lists.contains(&list_ref.id)
            && !synthesize_placeholder_list(
                tx,
                task,
                list_ref,
                &mut known_spaces,
                &mut known_lists,
                now,
            )?
        {
            continue;
        }
        let mut flat = task.clone();
        flat.parent = None;
        mirror::upsert_task(tx, &flat)?;
        // Sub-data that rides inline on the all-tasks payload — no extra
        // request, so it is not part of the lazy heavy-subdata rule.
        for field in &task.custom_fields {
            mirror::upsert_custom_field_def(tx, field)?;
            mirror::upsert_task_custom_value(tx, &task.id, field)?;
        }
        for checklist in &task.checklists {
            mirror::upsert_checklist(tx, &task.id, checklist)?;
        }
        for attachment in &task.attachments {
            mirror::upsert_attachment(tx, &task.id, attachment)?;
        }
    }

    for task in &fetched.tasks {
        let Some(parent) = task.parent.as_ref() else {
            continue;
        };
        let updated = tx.execute(
            "UPDATE clickup_tasks SET parent_id = ?1 WHERE id = ?2 \
             AND EXISTS (SELECT 1 FROM clickup_tasks WHERE id = ?1)",
            params![parent, task.id],
        )?;
        if updated == 0 {
            tracing::debug!(
                task = %task.id,
                parent = %parent,
                "parent not in mirror; task stays top-level"
            );
        }
    }
    Ok(())
}

/// A fetched task can reference a list the hierarchy fetch did not return
/// (created mid-fetch / paginated past). Park it under a synthetic hidden
/// folder instead of aborting the poll on the FK; the next hierarchy fetch
/// that returns the real List takes over via the plain list upsert.
fn synthesize_placeholder_list(
    tx: &Connection,
    task: &model::Task,
    list_ref: &model::NamedRef,
    known_spaces: &mut HashSet<String>,
    known_lists: &mut HashSet<String>,
    now: i64,
) -> Result<bool> {
    let Some(space_ref) = task.space.as_ref() else {
        tracing::warn!(
            task = %task.id,
            list = %list_ref.id,
            "unknown list and no space reference; skipping task"
        );
        return Ok(false);
    };
    if !known_spaces.contains(&space_ref.id) {
        let space = model::Space {
            id: space_ref.id.clone(),
            name: display_name(&space_ref.name, &space_ref.id),
            ..Default::default()
        };
        mirror::upsert_space(tx, &space, now)?;
        known_spaces.insert(space_ref.id.clone());
    }
    let folder_id = format!("{PLACEHOLDER_FOLDER_PREFIX}{}", space_ref.id);
    mirror::upsert_folder(tx, &folder_id, &space_ref.id, "(pending hierarchy)", true)?;
    let list = model::List {
        id: list_ref.id.clone(),
        name: display_name(&list_ref.name, &list_ref.id),
        folder: Some(model::NamedRef {
            id: folder_id,
            name: "(pending hierarchy)".into(),
            hidden: Some(true),
            access: None,
        }),
        ..Default::default()
    };
    mirror::upsert_list(tx, &list, &space_ref.id)?;
    known_lists.insert(list_ref.id.clone());
    tracing::warn!(
        list = %list_ref.id,
        task = %task.id,
        "synthesized placeholder list for unknown list_id"
    );
    Ok(true)
}

fn display_name(name: &str, id: &str) -> String {
    if name.trim().is_empty() {
        id.to_string()
    } else {
        name.to_string()
    }
}

/// Folders/lists mirrored for a fetched Space but absent from the fetch are
/// tombstoned. Placeholder containers are exempt — they were never in a
/// hierarchy fetch; their tasks stay visible until the real List arrives.
fn tombstone_absent_hierarchy(tx: &Connection, fetched: &FetchedCycle, now: i64) -> Result<()> {
    fill_temp_id_table(
        tx,
        "fetched_space_ids",
        fetched.spaces.iter().map(|sf| sf.space.id.as_str()),
    )?;
    let folder_ids = fetched.spaces.iter().flat_map(|sf| {
        sf.folders.iter().map(|f| f.id.as_str()).chain(
            sf.folderless_lists
                .iter()
                .filter_map(|l| l.folder.as_ref().map(|f| f.id.as_str())),
        )
    });
    fill_temp_id_table(tx, "fetched_folder_ids", folder_ids)?;
    let list_ids = fetched.spaces.iter().flat_map(|sf| {
        sf.folders
            .iter()
            .flat_map(|f| f.lists.iter().map(|l| l.id.as_str()))
            .chain(sf.folderless_lists.iter().map(|l| l.id.as_str()))
    });
    fill_temp_id_table(tx, "fetched_list_ids", list_ids)?;

    let placeholder_like = format!("{PLACEHOLDER_FOLDER_PREFIX}%");
    tx.execute(
        "UPDATE clickup_folders SET stale = 1, stale_since = ?1 \
         WHERE stale = 0 \
           AND space_id IN (SELECT id FROM temp.fetched_space_ids) \
           AND id NOT IN (SELECT id FROM temp.fetched_folder_ids) \
           AND id NOT LIKE ?2",
        params![now, placeholder_like],
    )?;
    tx.execute(
        "UPDATE clickup_lists SET stale = 1, stale_since = ?1 \
         WHERE stale = 0 \
           AND space_id IN (SELECT id FROM temp.fetched_space_ids) \
           AND id NOT IN (SELECT id FROM temp.fetched_list_ids) \
           AND COALESCE(folder_id, '') NOT LIKE ?2",
        params![now, placeholder_like],
    )?;
    Ok(())
}

/// The complete per-Space fetch is authoritative: a mirrored task in a
/// fetched Space that is absent went closed/deleted/moved-away (Decision
/// 6). Un-assignment is NOT absence — the task stays in the all-tasks
/// fetch with an updated assignee array and never lands here.
fn tombstone_absent_tasks(tx: &Connection, fetched: &FetchedCycle, now: i64) -> Result<()> {
    fill_temp_id_table(
        tx,
        "fetched_task_ids",
        fetched.tasks.iter().map(|t| t.id.as_str()),
    )?;
    tx.execute(
        "UPDATE clickup_tasks SET stale = 1, stale_since = ?1 \
         WHERE stale = 0 \
           AND id NOT IN (SELECT id FROM temp.fetched_task_ids) \
           AND list_id IN (SELECT id FROM clickup_lists \
                           WHERE space_id IN (SELECT id FROM temp.fetched_space_ids))",
        params![now],
    )?;
    Ok(())
}

/// Temp table instead of a giant `IN (?,?,…)` so a large workspace can't
/// hit the SQL parameter ceiling.
fn fill_temp_id_table<'a>(
    tx: &Connection,
    name: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<()> {
    tx.execute_batch(&format!(
        "DROP TABLE IF EXISTS temp.{name}; CREATE TEMP TABLE {name} (id TEXT PRIMARY KEY);"
    ))?;
    let mut stmt = tx.prepare(&format!("INSERT OR IGNORE INTO {name} (id) VALUES (?1)"))?;
    for id in ids {
        stmt.execute([id])?;
    }
    Ok(())
}

fn gc_stale_rows(tx: &Connection, cutoff: i64) -> Result<usize> {
    let mut removed = 0;
    // Leaf-first: deleting a stale parent would CASCADE a still-live
    // subtask (an open child of a closed parent), so only childless tasks
    // go; deeper chains drain over successive cycles.
    removed += tx.execute(
        "DELETE FROM clickup_tasks \
         WHERE stale = 1 AND stale_since IS NOT NULL AND stale_since < ?1 \
           AND NOT EXISTS (SELECT 1 FROM clickup_tasks c WHERE c.parent_id = clickup_tasks.id)",
        params![cutoff],
    )?;
    removed += tx.execute(
        "DELETE FROM clickup_lists \
         WHERE stale = 1 AND stale_since IS NOT NULL AND stale_since < ?1 \
           AND NOT EXISTS (SELECT 1 FROM clickup_tasks t WHERE t.list_id = clickup_lists.id)",
        params![cutoff],
    )?;
    removed += tx.execute(
        "DELETE FROM clickup_folders \
         WHERE stale = 1 AND stale_since IS NOT NULL AND stale_since < ?1 \
           AND NOT EXISTS (SELECT 1 FROM clickup_lists l WHERE l.folder_id = clickup_folders.id)",
        params![cutoff],
    )?;
    // A placeholder whose real List arrived leaves an empty synthetic
    // folder behind; sweep it as soon as nothing references it.
    removed += tx.execute(
        "DELETE FROM clickup_folders \
         WHERE id LIKE ?1 \
           AND NOT EXISTS (SELECT 1 FROM clickup_lists l WHERE l.folder_id = clickup_folders.id)",
        params![format!("{PLACEHOLDER_FOLDER_PREFIX}%")],
    )?;
    Ok(removed)
}

/// Mirror task ids currently assigned to `me` — stale rows included, so a
/// tombstoned task reappearing still-assigned does not re-notify.
fn assigned_task_ids(tx: &Connection, me: i64) -> Result<HashSet<String>> {
    let mut stmt = tx.prepare("SELECT id, assignees_json FROM clickup_tasks")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = HashSet::new();
    for row in rows {
        let (id, json) = row?;
        let users: Vec<model::User> = serde_json::from_str(&json).unwrap_or_default();
        if users.iter().any(|u| u.id == Some(me)) {
            out.insert(id);
        }
    }
    Ok(out)
}

fn newly_assigned_names(
    fetched: &FetchedCycle,
    me: i64,
    prev: &HashSet<String>,
    own_echo_ids: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    fetched
        .tasks
        .iter()
        .filter(|t| seen.insert(t.id.clone()))
        .filter(|t| t.assignees.iter().any(|u| u.id == Some(me)))
        .filter(|t| !prev.contains(&t.id))
        // Own-echo tasks: the assignment change was ours, suppress notification.
        .filter(|t| !own_echo_ids.contains(&t.id))
        .map(|t| t.name.clone())
        .collect()
}

/// `id → (stored date_updated, has materialized comments)` read before the
/// upserts overwrite `date_updated`.
fn prior_task_meta(tx: &Connection) -> Result<HashMap<String, (Option<i64>, bool)>> {
    let mut stmt = tx.prepare(
        "SELECT t.id, t.date_updated, \
                EXISTS (SELECT 1 FROM clickup_comments c WHERE c.task_id = t.id) \
         FROM clickup_tasks t",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (r.get::<_, Option<i64>>(1)?, r.get::<_, i64>(2)? != 0),
        ))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, meta) = row?;
        out.insert(id, meta);
    }
    Ok(out)
}

fn subdata_refresh_candidates(
    fetched: &FetchedCycle,
    prior: &HashMap<String, (Option<i64>, bool)>,
) -> Vec<String> {
    fetched
        .tasks
        .iter()
        .filter_map(|task| {
            let (stored_updated, has_comments) = prior.get(&task.id)?;
            // Never-materialized comments stay lazy: detail-open owns the
            // first fetch (Decision 5).
            if !has_comments {
                return None;
            }
            match (task.date_updated, stored_updated) {
                (Some(new), Some(old)) if new > *old => Some(task.id.clone()),
                (Some(_), None) => Some(task.id.clone()),
                _ => None,
            }
        })
        .collect()
}

fn collect_ids(conn: &Connection, sql: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for row in rows {
        out.insert(row?);
    }
    Ok(out)
}

// ── Detail refresh (lazy heavy sub-data entry point) ──

/// Replace a task's comments wholesale: remote deletions must disappear
/// locally, and comment ids are stable so a diff buys nothing.
pub fn store_comments(conn: &Connection, task_id: &str, comments: &[model::Comment]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM clickup_comments WHERE task_id = ?1", [task_id])?;
    for comment in comments {
        mirror::upsert_comment(&tx, task_id, comment)?;
    }
    tx.commit()?;
    Ok(())
}

/// Commit a `get_task` detail payload + its comments. The nested `subtasks`
/// array is intentionally ignored — the reconcile tree source is solely the
/// flat `parent` field (Decision 8).
pub fn store_task_detail(
    conn: &Connection,
    task: &model::Task,
    comments: &[model::Comment],
    now: i64,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let was_stale: Option<i64> = tx
        .query_row(
            "SELECT stale FROM clickup_tasks WHERE id = ?1",
            [&task.id],
            |r| r.get(0),
        )
        .optional()?;
    let mut known_lists = collect_ids(&tx, "SELECT id FROM clickup_lists")?;
    let mut known_spaces = collect_ids(&tx, "SELECT id FROM clickup_spaces")?;
    let Some(list_ref) = task.list.as_ref() else {
        bail!("task {} has no list reference", task.id);
    };
    if !known_lists.contains(&list_ref.id)
        && !synthesize_placeholder_list(
            &tx,
            task,
            list_ref,
            &mut known_spaces,
            &mut known_lists,
            now,
        )?
    {
        bail!(
            "task {} references an unknown list without a space",
            task.id
        );
    }
    let mut flat = task.clone();
    flat.parent = None;
    mirror::upsert_task(&tx, &flat)?;
    if let Some(parent) = task.parent.as_ref() {
        tx.execute(
            "UPDATE clickup_tasks SET parent_id = ?1 WHERE id = ?2 \
             AND EXISTS (SELECT 1 FROM clickup_tasks WHERE id = ?1)",
            params![parent, task.id],
        )?;
    }
    // A detail refresh must not resurrect a tombstoned task in the panel:
    // only the authoritative poll fetch un-tombstones (Decision 6).
    if was_stale == Some(1) {
        tx.execute(
            "UPDATE clickup_tasks SET stale = 1 WHERE id = ?1",
            [&task.id],
        )?;
    }
    for field in &task.custom_fields {
        mirror::upsert_custom_field_def(&tx, field)?;
        mirror::upsert_task_custom_value(&tx, &task.id, field)?;
    }
    for checklist in &task.checklists {
        mirror::upsert_checklist(&tx, &task.id, checklist)?;
    }
    for attachment in &task.attachments {
        mirror::upsert_attachment(&tx, &task.id, attachment)?;
    }
    tx.execute(
        "DELETE FROM clickup_comments WHERE task_id = ?1",
        [&task.id],
    )?;
    for comment in comments {
        mirror::upsert_comment(&tx, &task.id, comment)?;
    }
    tx.commit()?;
    Ok(())
}

// ── Team selection ──

/// Resolve which teams to sync: rows in `clickup_sync_state` mark the
/// selection. A single-team account auto-selects (no ambiguity — Decision 9
/// only forbids silently picking `teams[0]` when there are several); a
/// multi-team account with no selection returns empty so the UI prompts.
pub fn resolve_selected_teams(conn: &Connection, teams: &[model::Team]) -> Result<Vec<String>> {
    let stored = mirror::list_sync_state_teams(conn)?;
    let valid: Vec<String> = stored
        .into_iter()
        .filter(|id| teams.iter().any(|t| &t.id == id))
        .collect();
    if !valid.is_empty() {
        return Ok(valid);
    }
    if let [only] = teams {
        conn.execute(
            "INSERT OR IGNORE INTO clickup_sync_state (team_id, baseline_done) VALUES (?1, 0)",
            [&only.id],
        )?;
        return Ok(vec![only.id.clone()]);
    }
    Ok(Vec::new())
}

/// Persist the user's team choice. Switching teams tombstones the whole
/// mirror: the old team's rows are never fetched again (so they would
/// otherwise linger forever), while the new team's first cycle
/// un-tombstones whatever it actually owns and GC drains the rest. The new
/// team starts with `baseline_done = 0` → silent seed.
pub fn select_team(conn: &Connection, team_id: &str, now: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let current = mirror::list_sync_state_teams(&tx)?;
    if !(current.len() == 1 && current[0] == team_id) {
        if !current.is_empty() {
            tx.execute(
                "UPDATE clickup_tasks SET stale = 1, stale_since = ?1 WHERE stale = 0",
                params![now],
            )?;
            tx.execute(
                "UPDATE clickup_lists SET stale = 1, stale_since = ?1 WHERE stale = 0",
                params![now],
            )?;
            tx.execute(
                "UPDATE clickup_folders SET stale = 1, stale_since = ?1 WHERE stale = 0",
                params![now],
            )?;
        }
        tx.execute(
            "DELETE FROM clickup_sync_state WHERE team_id != ?1",
            [team_id],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO clickup_sync_state (team_id, baseline_done) VALUES (?1, 0)",
            [team_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ── Effects (abstracted so tests can capture them) ──

pub trait SyncEffects {
    fn notify(&mut self, title: &str, body: &str);
    fn emit_changed(&mut self);
    /// Emit a `clickup:write-conflict` event.  Called once per conflict
    /// detected this cycle.  The default is a no-op so existing impls
    /// (test recording structs) do not need updating.
    fn emit_write_conflict(&mut self, _conflict: &writeback::WriteConflict) {}
    /// Emit a `clickup:assigned` event so the frontend can raise an in-app
    /// toast (the desktop notification can be missed when the window is
    /// focused or the notification daemon is flaky). Default no-op for tests.
    fn emit_assigned(&mut self, _names: &[String]) {}
}

pub fn apply_outcome_effects(
    outcome: &ReconcileOutcome,
    fetch_changed: bool,
    effects: &mut dyn SyncEffects,
) {
    if outcome.notifications_armed && !outcome.newly_assigned.is_empty() {
        if let Some((title, body)) = assignment_notification(&outcome.newly_assigned) {
            effects.notify(&title, &body);
        }
        effects.emit_assigned(&outcome.newly_assigned);
    }
    if fetch_changed || outcome.gc_removed > 0 {
        effects.emit_changed();
    }
    for conflict in &outcome.write_conflicts {
        effects.emit_write_conflict(conflict);
    }
}

/// Coalescing rule (Decision 7): one ping for one task, one "N new tasks"
/// ping for a bulk assignment between two polls.
pub fn assignment_notification(newly: &[String]) -> Option<(String, String)> {
    match newly {
        [] => None,
        [name] => Some(("ClickUp".to_string(), format!("New task assigned: {name}"))),
        many => Some((
            "ClickUp".to_string(),
            format!("{} new tasks assigned", many.len()),
        )),
    }
}

struct TauriEffects {
    app: AppHandle,
}

impl SyncEffects for TauriEffects {
    fn notify(&mut self, title: &str, body: &str) {
        // Routed through crate::notify::send: Linux uses notify-send (primary
        // path per Decision 3a — plugin's Ok(()) indistinguishable from silent
        // no-display under WebKitGTK); macOS uses the notification plugin.
        crate::notify::send(&self.app, title, body);
    }

    fn emit_changed(&mut self) {
        let _ = self.app.emit("clickup:changed", ());
    }

    fn emit_write_conflict(&mut self, conflict: &writeback::WriteConflict) {
        if let Err(e) = self.app.emit("clickup:write-conflict", conflict) {
            tracing::warn!("clickup write-conflict emit failed: {e}");
        }
    }

    fn emit_assigned(&mut self, names: &[String]) {
        if let Err(e) = self.app.emit("clickup:assigned", names) {
            tracing::warn!("clickup assigned emit failed: {e}");
        }
    }
}

// ── Status surface (team picker + settings UI) ──

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TeamInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct SyncStatus {
    /// "idle" | "no_token" | "needs_team" | "ok" | "error"
    pub state: String,
    pub teams: Vec<TeamInfo>,
    pub team_id: Option<String>,
    /// The token user's id — feeds the panel's assigned-to-me filter.
    pub user_id: Option<i64>,
    pub last_sync: Option<i64>,
    pub baseline_done: bool,
    pub error: Option<String>,
}

pub struct ClickUpSyncState {
    handle: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    status: std::sync::Mutex<SyncStatus>,
}

impl Default for ClickUpSyncState {
    fn default() -> Self {
        Self {
            handle: std::sync::Mutex::new(None),
            status: std::sync::Mutex::new(SyncStatus {
                state: "idle".into(),
                ..SyncStatus::default()
            }),
        }
    }
}

impl ClickUpSyncState {
    pub fn snapshot(&self) -> SyncStatus {
        self.status
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| SyncStatus {
                state: "error".into(),
                error: Some("status lock poisoned".into()),
                ..SyncStatus::default()
            })
    }
}

fn set_status(app: &AppHandle, status: SyncStatus) {
    let state = app.state::<ClickUpSyncState>();
    if let Ok(mut guard) = state.status.lock() {
        if *guard == status {
            return;
        }
        *guard = status.clone();
    }
    let _ = app.emit("clickup:sync-status", status);
}

fn error_status(message: String, me: Option<i64>) -> SyncStatus {
    SyncStatus {
        state: "error".into(),
        user_id: me,
        error: Some(message),
        ..SyncStatus::default()
    }
}

// ── Poller lifecycle ──

/// (Re)start the poll loop. Safe to call any time: token set/clear and team
/// selection all funnel here. The loop itself decides whether it can run
/// (token present, team resolved) and parks the status accordingly.
pub fn restart(app: &AppHandle) {
    let state = app.state::<ClickUpSyncState>();
    let Ok(mut guard) = state.handle.lock() else {
        return;
    };
    if let Some(handle) = guard.take() {
        handle.abort();
    }
    let loop_app = app.clone();
    *guard = Some(tauri::async_runtime::spawn(async move {
        run_loop(loop_app).await;
    }));
}

/// Immediate status feedback for the token/team commands: the restarted loop
/// takes a full network cycle to push its first status, and the UI must not
/// sit on a stale snapshot meanwhile (the settings Select wouldn't mark the
/// just-picked team, and a stale no_token would read as "token erased").
pub fn note_team_selected(app: &AppHandle, team_id: &str) {
    let mut status = app.state::<ClickUpSyncState>().snapshot();
    status.state = "syncing".into();
    status.team_id = Some(team_id.to_string());
    status.error = None;
    set_status(app, status);
}

/// Manual "sync now": flip to syncing and restart the loop so it runs a fresh
/// cycle immediately. No-op when no token is configured.
pub fn sync_now(app: &AppHandle) {
    let mut status = app.state::<ClickUpSyncState>().snapshot();
    if status.state == "no_token" {
        return;
    }
    status.state = "syncing".into();
    status.error = None;
    set_status(app, status);
    restart(app);
}

pub fn note_token_set(app: &AppHandle) {
    let mut status = app.state::<ClickUpSyncState>().snapshot();
    status.state = "syncing".into();
    status.user_id = None;
    status.error = None;
    set_status(app, status);
}

pub fn note_token_cleared(app: &AppHandle) {
    set_status(
        app,
        SyncStatus {
            state: "no_token".into(),
            ..SyncStatus::default()
        },
    );
}

fn poll_interval() -> Duration {
    let secs = crate::config::Config::load()
        .clickup_poll_interval_secs
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
    // Floor guards a fat-fingered config from hammering the rate limit.
    Duration::from_secs(secs.max(10))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn run_loop(app: AppHandle) {
    // Keyring access blocks on D-Bus; keep it off the async workers. A
    // transient load error (D-Bus hiccup) must retry, not kill the loop —
    // a dead loop only revives on the next token/team command.
    let stored = loop {
        match tauri::async_runtime::spawn_blocking(auth::load_token).await {
            Ok(Ok(Some(stored))) => break stored,
            Ok(Ok(None)) => {
                set_status(
                    &app,
                    SyncStatus {
                        state: "no_token".into(),
                        ..SyncStatus::default()
                    },
                );
                return;
            }
            Ok(Err(e)) => {
                set_status(
                    &app,
                    error_status(format!("token load failed: {e:#}"), None),
                );
                tokio::time::sleep(poll_interval()).await;
            }
            Err(e) => {
                set_status(
                    &app,
                    error_status(format!("token load task failed: {e}"), None),
                );
                return;
            }
        }
    };
    let client = match ClickUpClient::new(&stored.token) {
        Ok(c) => c,
        Err(e) => {
            set_status(&app, error_status(format!("{e:#}"), None));
            return;
        }
    };

    // Resolved once per token and persisted: feeds the assigned-to-me filter
    // and post-baseline assignment detection. The DB cache covers the gap
    // between a restart and the first GET /user (token commands clear it, so
    // a rotated token re-resolves).
    let mut me: Option<i64> = {
        let db = app.state::<crate::db::SharedDb>();
        db.lock()
            .ok()
            .and_then(|guard| mirror::cached_user_id(guard.conn()).ok())
            .flatten()
    };
    let mut fingerprints: HashMap<String, u64> = HashMap::new();
    loop {
        if me.is_none() {
            match client.get_user().await {
                Ok(user) => {
                    me = user.id;
                    let db = app.state::<crate::db::SharedDb>();
                    if let Ok(guard) = db.lock()
                        && let Err(e) = mirror::set_cached_user_id(guard.conn(), me)
                    {
                        tracing::warn!("user id cache write failed: {e:#}");
                    }
                }
                Err(e) => set_status(&app, error_status(format!("{e:#}"), None)),
            }
        }
        if me.is_some()
            && let Err(e) = poll_once(&app, &client, me, &mut fingerprints).await
        {
            tracing::warn!("clickup poll cycle failed: {e:#}");
            set_status(&app, error_status(format!("{e:#}"), me));
        }
        tokio::time::sleep(poll_interval()).await;
    }
}

async fn poll_once(
    app: &AppHandle,
    client: &ClickUpClient,
    me: Option<i64>,
    fingerprints: &mut HashMap<String, u64>,
) -> Result<()> {
    let teams = client.get_teams().await?;
    let team_infos: Vec<TeamInfo> = teams
        .iter()
        .map(|t| TeamInfo {
            id: t.id.clone(),
            name: t.name.clone(),
        })
        .collect();

    let selected = {
        let db = app.state::<crate::db::SharedDb>();
        let guard = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
        resolve_selected_teams(guard.conn(), &teams)?
    };
    if selected.is_empty() {
        set_status(
            app,
            SyncStatus {
                state: "needs_team".into(),
                teams: team_infos,
                user_id: me,
                ..SyncStatus::default()
            },
        );
        return Ok(());
    }

    for team_id in &selected {
        let fetched = fetch_cycle(client, team_id).await?;
        let fp = fingerprint(&fetched);
        let now = now_secs();
        let registry = app.state::<super::writeback::WritebackRegistry>();
        let outcome = {
            let db = app.state::<crate::db::SharedDb>();
            let guard = db.lock().map_err(|_| anyhow!("db lock poisoned"))?;
            reconcile_team(guard.conn(), &fetched, me, now, Some(&*registry))?
        };
        let fetch_changed = fingerprints.get(team_id) != Some(&fp);
        fingerprints.insert(team_id.clone(), fp);
        let mut effects = TauriEffects { app: app.clone() };
        apply_outcome_effects(&outcome, fetch_changed, &mut effects);
        refresh_stale_subdata(app, client, &outcome.refresh_subdata).await;
        set_status(
            app,
            SyncStatus {
                state: "ok".into(),
                teams: team_infos.clone(),
                team_id: Some(team_id.clone()),
                user_id: me,
                last_sync: Some(now),
                baseline_done: true,
                error: None,
            },
        );
    }
    Ok(())
}

/// Poll-side leg of the lazy heavy-subdata rule: only tasks whose comments
/// are already materialized and whose `date_updated` advanced, capped per
/// cycle. Failures are logged, never fatal to the loop.
async fn refresh_stale_subdata(app: &AppHandle, client: &ClickUpClient, task_ids: &[String]) {
    for task_id in task_ids.iter().take(SUBDATA_REFRESH_CAP) {
        let comments = match client.get_task_comments(task_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(task = %task_id, "comment refresh failed: {e:#}");
                continue;
            }
        };
        let db = app.state::<crate::db::SharedDb>();
        let Ok(guard) = db.lock() else {
            continue;
        };
        if let Err(e) = store_comments(guard.conn(), task_id, &comments) {
            tracing::warn!(task = %task_id, "comment store failed: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clickup::model::{
        FoldersResponse, ListsResponse, NamedRef, SpacesResponse, Status, Task, TasksPage, Team,
        User,
    };
    use mirror::TaskFilter;

    const ME: i64 = 81234567;
    const TEAM: &str = "9013000000";
    const SPACE_PRODUCTO: &str = "901312445262";
    const LIST_SPRINT: &str = "901317020124";
    const LIST_INBOX: &str = "901317010101";
    const TASK_PARENT: &str = "86ahwtc67";
    const TASK_SUB: &str = "86ahwtd99";

    fn mirror_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/015_clickup_mirror.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/016_clickup_stale_since.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/020_clickup_status_type.sql"))
            .unwrap();
        conn
    }

    fn fixture_cycle() -> FetchedCycle {
        let spaces: SpacesResponse =
            serde_json::from_str(include_str!("fixtures/spaces.json")).unwrap();
        let folders: FoldersResponse =
            serde_json::from_str(include_str!("fixtures/folders.json")).unwrap();
        let folderless: ListsResponse =
            serde_json::from_str(include_str!("fixtures/folderless_lists.json")).unwrap();
        let page: TasksPage =
            serde_json::from_str(include_str!("fixtures/tasks_page.json")).unwrap();

        let mut space_fetches: Vec<SpaceFetch> = spaces
            .spaces
            .into_iter()
            .map(|space| SpaceFetch {
                space,
                folders: Vec::new(),
                folderless_lists: Vec::new(),
            })
            .collect();
        space_fetches[0].folders = folders.folders;
        space_fetches[0].folderless_lists = folderless.lists;
        FetchedCycle {
            team_id: TEAM.to_string(),
            spaces: space_fetches,
            tasks: page.tasks,
        }
    }

    fn new_task(id: &str, name: &str, list_id: &str, assignee: Option<i64>) -> Task {
        Task {
            id: id.into(),
            name: name.into(),
            list: Some(NamedRef {
                id: list_id.into(),
                name: String::new(),
                hidden: None,
                access: None,
            }),
            space: Some(NamedRef {
                id: SPACE_PRODUCTO.into(),
                ..Default::default()
            }),
            assignees: assignee
                .map(|a| {
                    vec![User {
                        id: Some(a),
                        username: Some("Felipe".into()),
                        ..Default::default()
                    }]
                })
                .unwrap_or_default(),
            ..Default::default()
        }
    }

    fn task_col_i64(conn: &Connection, id: &str, col: &str) -> i64 {
        conn.query_row(
            &format!("SELECT {col} FROM clickup_tasks WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn task_col_str(conn: &Connection, id: &str, col: &str) -> String {
        conn.query_row(
            &format!("SELECT {col} FROM clickup_tasks WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[derive(Default)]
    struct RecordingEffects {
        notifications: Vec<(String, String)>,
        changed: usize,
    }

    impl SyncEffects for RecordingEffects {
        fn notify(&mut self, title: &str, body: &str) {
            self.notifications.push((title.into(), body.into()));
        }
        fn emit_changed(&mut self) {
            self.changed += 1;
        }
    }

    #[test]
    fn first_sync_of_preassigned_workspace_is_silent() {
        let conn = mirror_conn();
        let outcome = reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        // The empty mirror sees every assigned task as "new" — the unarmed
        // baseline gate is what keeps the seed silent.
        assert!(!outcome.notifications_armed);
        assert!(!outcome.newly_assigned.is_empty());

        let mut fx = RecordingEffects::default();
        apply_outcome_effects(&outcome, true, &mut fx);
        assert!(fx.notifications.is_empty(), "baseline seed must be silent");
        assert_eq!(fx.changed, 1);

        let state = mirror::get_sync_state(&conn, TEAM).unwrap().unwrap();
        assert!(state.baseline_done);
        assert_eq!(state.last_full_sync, Some(1000));

        let tasks = mirror::read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn post_baseline_assignment_notifies_once() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let mut cycle2 = fixture_cycle();
        cycle2
            .tasks
            .push(new_task("hotfix1", "Hotfix login", LIST_SPRINT, Some(ME)));
        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, None).unwrap();
        assert!(outcome.notifications_armed);
        assert_eq!(outcome.newly_assigned, vec!["Hotfix login".to_string()]);

        let mut fx = RecordingEffects::default();
        apply_outcome_effects(&outcome, true, &mut fx);
        assert_eq!(fx.notifications.len(), 1);
        assert!(fx.notifications[0].1.contains("Hotfix login"));
    }

    #[test]
    fn bulk_assignment_coalesces_into_one_notification() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let mut cycle2 = fixture_cycle();
        cycle2
            .tasks
            .push(new_task("bulk1", "Task A", LIST_SPRINT, Some(ME)));
        cycle2
            .tasks
            .push(new_task("bulk2", "Task B", LIST_INBOX, Some(ME)));
        // Re-assignment of an already-mirrored unassigned task counts too.
        if let Some(sub) = cycle2.tasks.iter_mut().find(|t| t.id == TASK_SUB) {
            sub.assignees = vec![User {
                id: Some(ME),
                ..Default::default()
            }];
        }
        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, None).unwrap();
        assert_eq!(outcome.newly_assigned.len(), 3);

        let mut fx = RecordingEffects::default();
        apply_outcome_effects(&outcome, true, &mut fx);
        assert_eq!(fx.notifications.len(), 1);
        assert!(fx.notifications[0].1.contains("3 new tasks assigned"));
    }

    #[test]
    fn status_change_and_list_move_update_mirror() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let mut cycle2 = fixture_cycle();
        let parent = cycle2
            .tasks
            .iter_mut()
            .find(|t| t.id == TASK_PARENT)
            .unwrap();
        parent.status = Some(Status {
            id: Some("sc901317020124_done".into()),
            status: "complete".into(),
            color: Some("#6bc950".into()),
            orderindex: Some(2),
            status_type: Some("closed".into()),
        });
        parent.list = Some(NamedRef {
            id: LIST_INBOX.into(),
            name: "Inbox".into(),
            hidden: None,
            access: None,
        });
        reconcile_team(&conn, &cycle2, Some(ME), 2000, None).unwrap();

        assert_eq!(task_col_str(&conn, TASK_PARENT, "status_name"), "complete");
        assert_eq!(task_col_str(&conn, TASK_PARENT, "list_id"), LIST_INBOX);
        assert_eq!(task_col_i64(&conn, TASK_PARENT, "stale"), 0);
    }

    #[test]
    fn unassignment_updates_assignees_but_never_tombstones() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let mut cycle2 = fixture_cycle();
        cycle2
            .tasks
            .iter_mut()
            .find(|t| t.id == TASK_PARENT)
            .unwrap()
            .assignees
            .clear();
        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, None).unwrap();
        assert!(outcome.newly_assigned.is_empty());

        // Still present (not stale), just hidden by the assigned-to-me filter.
        assert_eq!(task_col_i64(&conn, TASK_PARENT, "stale"), 0);
        let mine = mirror::read_tasks(
            &conn,
            &TaskFilter {
                assignee_id: Some(ME),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(mine.is_empty());
        let all = mirror::read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn absent_task_tombstones_and_reappearance_untombstones() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let mut cycle2 = fixture_cycle();
        cycle2.tasks.retain(|t| t.id != TASK_SUB);
        reconcile_team(&conn, &cycle2, Some(ME), 2000, None).unwrap();
        assert_eq!(task_col_i64(&conn, TASK_SUB, "stale"), 1);
        assert_eq!(task_col_i64(&conn, TASK_SUB, "stale_since"), 2000);

        // Reappearance un-tombstones without a notification (it was already
        // in the prev-assigned baseline set).
        let outcome = reconcile_team(&conn, &fixture_cycle(), Some(ME), 3000, None).unwrap();
        assert_eq!(task_col_i64(&conn, TASK_SUB, "stale"), 0);
        assert!(outcome.newly_assigned.is_empty());
    }

    #[test]
    fn absent_hierarchy_rows_tombstone_too() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let mut cycle2 = fixture_cycle();
        cycle2.spaces[0].folderless_lists.clear();
        cycle2
            .tasks
            .retain(|t| t.list.as_ref().map(|l| l.id.as_str()) != Some(LIST_INBOX));
        reconcile_team(&conn, &cycle2, Some(ME), 2000, None).unwrap();

        let list_stale: i64 = conn
            .query_row(
                "SELECT stale FROM clickup_lists WHERE id = ?1",
                [LIST_INBOX],
                |r| r.get(0),
            )
            .unwrap();
        let folder_stale: i64 = conn
            .query_row(
                "SELECT stale FROM clickup_folders WHERE id = '901310888888'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(list_stale, 1);
        assert_eq!(folder_stale, 1);

        // Reappearance resets both.
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 3000, None).unwrap();
        let list_stale: i64 = conn
            .query_row(
                "SELECT stale FROM clickup_lists WHERE id = ?1",
                [LIST_INBOX],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(list_stale, 0);
    }

    #[test]
    fn unknown_list_synthesizes_placeholder_and_survives_hierarchy_sweep() {
        let conn = mirror_conn();
        let mut cycle = fixture_cycle();
        cycle
            .tasks
            .push(new_task("orphan1", "Orphan task", "L-unknown", Some(ME)));
        reconcile_team(&conn, &cycle, Some(ME), 1000, None).unwrap();

        let folder_id: String = conn
            .query_row(
                "SELECT folder_id FROM clickup_lists WHERE id = 'L-unknown'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            folder_id,
            format!("{PLACEHOLDER_FOLDER_PREFIX}{SPACE_PRODUCTO}")
        );
        let hidden: i64 = conn
            .query_row(
                "SELECT hidden FROM clickup_folders WHERE id = ?1",
                [&folder_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hidden, 1);
        assert_eq!(task_col_i64(&conn, "orphan1", "stale"), 0);

        // Second cycle: hierarchy still doesn't know the list — the
        // placeholder is exempt from the absent-hierarchy tombstone.
        reconcile_team(&conn, &cycle, Some(ME), 2000, None).unwrap();
        let list_stale: i64 = conn
            .query_row(
                "SELECT stale FROM clickup_lists WHERE id = 'L-unknown'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(list_stale, 0);
        assert_eq!(task_col_i64(&conn, "orphan1", "stale"), 0);

        // The real List arrives via the hierarchy: it takes over and the
        // empty placeholder folder is swept.
        let mut cycle3 = cycle.clone();
        cycle3.spaces[0].folderless_lists.push(model::List {
            id: "L-unknown".into(),
            name: "Now real".into(),
            folder: Some(NamedRef {
                id: "901310888888".into(),
                name: "hidden".into(),
                hidden: Some(true),
                access: None,
            }),
            ..Default::default()
        });
        reconcile_team(&conn, &cycle3, Some(ME), 3000, None).unwrap();
        let folder_id: String = conn
            .query_row(
                "SELECT folder_id FROM clickup_lists WHERE id = 'L-unknown'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(folder_id, "901310888888");
        let placeholder_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_folders WHERE id LIKE 'nergal:placeholder:%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(placeholder_count, 0);
    }

    #[test]
    fn subtask_delivered_before_parent_still_links() {
        let conn = mirror_conn();
        let mut cycle = fixture_cycle();
        cycle.tasks.reverse(); // subtask first, parent second
        reconcile_team(&conn, &cycle, Some(ME), 1000, None).unwrap();

        let parent_id: Option<String> = conn
            .query_row(
                "SELECT parent_id FROM clickup_tasks WHERE id = ?1",
                [TASK_SUB],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent_id.as_deref(), Some(TASK_PARENT));
    }

    #[test]
    fn mid_transaction_failure_rolls_back_the_whole_cycle() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();
        let original_updated = task_col_i64(&conn, TASK_PARENT, "date_updated");

        conn.execute_batch(
            "CREATE TRIGGER boom BEFORE INSERT ON clickup_tasks WHEN NEW.id = 'boom' \
             BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
        )
        .unwrap();

        let mut cycle2 = fixture_cycle();
        cycle2
            .tasks
            .iter_mut()
            .find(|t| t.id == TASK_PARENT)
            .unwrap()
            .date_updated = Some(9_999_999_999_999);
        cycle2
            .tasks
            .push(new_task("good1", "Good task", LIST_SPRINT, None));
        cycle2
            .tasks
            .push(new_task("boom", "Boom", LIST_SPRINT, None));

        assert!(reconcile_team(&conn, &cycle2, Some(ME), 2000, None).is_err());

        // Partial work (the date bump + the good task) rolled back with it.
        assert_eq!(
            task_col_i64(&conn, TASK_PARENT, "date_updated"),
            original_updated
        );
        let good_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_tasks WHERE id = 'good1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(good_count, 0);
        let state = mirror::get_sync_state(&conn, TEAM).unwrap().unwrap();
        assert_eq!(state.last_full_sync, Some(1000));
    }

    #[test]
    fn stale_gc_removes_old_rows_but_keeps_parents_with_children() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        // Cycle without the subtask → tombstoned at t=2000.
        let mut no_sub = fixture_cycle();
        no_sub.tasks.retain(|t| t.id != TASK_SUB);
        reconcile_team(&conn, &no_sub, Some(ME), 2000, None).unwrap();

        // Within retention: kept.
        let outcome = reconcile_team(&conn, &no_sub, Some(ME), 2000 + 60, None).unwrap();
        assert_eq!(outcome.gc_removed, 0);

        // Past retention: the childless subtask is GC'd.
        let late = 2000 + STALE_RETENTION_SECS + 1;
        let outcome = reconcile_team(&conn, &no_sub, Some(ME), late, None).unwrap();
        assert!(outcome.gc_removed >= 1);
        let sub_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_tasks WHERE id = ?1",
                [TASK_SUB],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sub_count, 0);

        // A stale parent with a live child is never GC'd (CASCADE would
        // take the open child with it).
        let mut no_parent = fixture_cycle();
        no_parent.tasks.retain(|t| t.id != TASK_PARENT);
        reconcile_team(&conn, &no_parent, Some(ME), late + 10, None).unwrap();
        let later = late + 10 + STALE_RETENTION_SECS + 1;
        reconcile_team(&conn, &no_parent, Some(ME), later, None).unwrap();
        let parent_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_tasks WHERE id = ?1",
                [TASK_PARENT],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent_count, 1, "stale parent with live child survives GC");
    }

    #[test]
    fn subdata_refresh_only_for_materialized_and_advanced_tasks() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        // Materialize comments for the parent task only.
        let comments: crate::clickup::model::CommentsResponse =
            serde_json::from_str(include_str!("fixtures/comments.json")).unwrap();
        store_comments(&conn, TASK_PARENT, &comments.comments).unwrap();

        // Unchanged date_updated → no refresh.
        let outcome = reconcile_team(&conn, &fixture_cycle(), Some(ME), 2000, None).unwrap();
        assert!(outcome.refresh_subdata.is_empty());

        // Advanced date_updated on both tasks → only the materialized one.
        let mut cycle3 = fixture_cycle();
        for task in &mut cycle3.tasks {
            task.date_updated = Some(9_999_999_999_999);
        }
        let outcome = reconcile_team(&conn, &cycle3, Some(ME), 3000, None).unwrap();
        assert_eq!(outcome.refresh_subdata, vec![TASK_PARENT.to_string()]);
    }

    #[test]
    fn store_task_detail_keeps_tombstone_and_replaces_comments() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();
        conn.execute(
            "UPDATE clickup_tasks SET stale = 1, stale_since = 999 WHERE id = ?1",
            [TASK_PARENT],
        )
        .unwrap();

        let page: TasksPage =
            serde_json::from_str(include_str!("fixtures/tasks_page.json")).unwrap();
        let task = page
            .tasks
            .into_iter()
            .find(|t| t.id == TASK_PARENT)
            .unwrap();
        let comments: crate::clickup::model::CommentsResponse =
            serde_json::from_str(include_str!("fixtures/comments.json")).unwrap();

        store_task_detail(&conn, &task, &comments.comments, 2000).unwrap();
        // Detail refresh must not resurrect the tombstone.
        assert_eq!(task_col_i64(&conn, TASK_PARENT, "stale"), 1);
        assert_eq!(mirror::read_comments(&conn, TASK_PARENT).unwrap().len(), 2);

        // Wholesale replace: a remotely deleted comment disappears.
        store_task_detail(&conn, &task, &comments.comments[..1], 3000).unwrap();
        assert_eq!(mirror::read_comments(&conn, TASK_PARENT).unwrap().len(), 1);
    }

    #[test]
    fn resolve_selected_teams_auto_selects_single_and_prompts_multi() {
        let conn = mirror_conn();
        let one = vec![Team {
            id: TEAM.into(),
            name: "Mufdi Workspace".into(),
            ..Default::default()
        }];
        assert_eq!(
            resolve_selected_teams(&conn, &one).unwrap(),
            vec![TEAM.to_string()]
        );
        // Auto-selection persisted the row with the baseline unarmed.
        let state = mirror::get_sync_state(&conn, TEAM).unwrap().unwrap();
        assert!(!state.baseline_done);

        let conn = mirror_conn();
        let two = vec![
            Team {
                id: "team-a".into(),
                name: "A".into(),
                ..Default::default()
            },
            Team {
                id: "team-b".into(),
                name: "B".into(),
                ..Default::default()
            },
        ];
        assert!(resolve_selected_teams(&conn, &two).unwrap().is_empty());

        select_team(&conn, "team-b", 1000).unwrap();
        assert_eq!(
            resolve_selected_teams(&conn, &two).unwrap(),
            vec!["team-b".to_string()]
        );
    }

    #[test]
    fn switching_teams_tombstones_the_old_mirror() {
        let conn = mirror_conn();
        select_team(&conn, TEAM, 500).unwrap();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        select_team(&conn, "other-team", 2000).unwrap();
        let live: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_tasks WHERE stale = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live, 0);
        assert_eq!(
            mirror::list_sync_state_teams(&conn).unwrap(),
            vec!["other-team".to_string()]
        );

        // Re-selecting the same team is a no-op.
        select_team(&conn, "other-team", 3000).unwrap();
        assert_eq!(
            mirror::list_sync_state_teams(&conn).unwrap(),
            vec!["other-team".to_string()]
        );
    }

    #[test]
    fn assignment_notification_messages() {
        assert!(assignment_notification(&[]).is_none());
        let (_, body) = assignment_notification(&["Fix the build".into()]).unwrap();
        assert_eq!(body, "New task assigned: Fix the build");
        let (_, body) =
            assignment_notification(&["A".into(), "B".into(), "C".into(), "D".into()]).unwrap();
        assert_eq!(body, "4 new tasks assigned");
    }

    #[test]
    fn fingerprint_is_stable_and_sensitive() {
        let a = fixture_cycle();
        let b = fixture_cycle();
        assert_eq!(fingerprint(&a), fingerprint(&b));

        let mut c = fixture_cycle();
        c.tasks
            .iter_mut()
            .find(|t| t.id == TASK_PARENT)
            .unwrap()
            .date_updated = Some(1);
        assert_ne!(fingerprint(&a), fingerprint(&c));
    }

    // ── Fetch-all-then-commit over a mock HTTP responder ──

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimal scripted one-request-per-connection responder (same pattern
    /// as the client tests; `Connection: close` forces reqwest to reconnect
    /// so responses are consumed in order).
    async fn spawn_mock(responses: Vec<(u16, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
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
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn failed_fetch_commits_nothing() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        // Hierarchy succeeds, the task fetch 500s mid-cycle: fetch_cycle
        // errors before any DB write — the design is fetch-all-then-commit.
        let base = spawn_mock(vec![
            (200, include_str!("fixtures/spaces.json").to_string()),
            (200, include_str!("fixtures/folders.json").to_string()),
            (
                200,
                include_str!("fixtures/folderless_lists.json").to_string(),
            ),
            (200, r#"{"folders":[]}"#.to_string()),
            (200, r#"{"lists":[]}"#.to_string()),
            (500, r#"{"err":"internal"}"#.to_string()),
        ])
        .await;
        let client = ClickUpClient::with_base_url("pk_test_token", &base).unwrap();
        assert!(fetch_cycle(&client, TEAM).await.is_err());

        // Prior mirror fully intact.
        let tasks = mirror::read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(tasks.len(), 2);
        let state = mirror::get_sync_state(&conn, TEAM).unwrap().unwrap();
        assert_eq!(state.last_full_sync, Some(1000));
    }

    #[tokio::test]
    async fn fetch_cycle_gathers_hierarchy_and_paginated_tasks() {
        // Two spaces, then a two-page task fetch terminated by last_page.
        let page1 = r#"{"tasks":[{"id":"tail1","name":"Tail","list":{"id":"901317020124","name":"Sprint 23"},"space":{"id":"901312445262"}}],"last_page":true}"#;
        let base = spawn_mock(vec![
            (200, include_str!("fixtures/spaces.json").to_string()),
            (200, include_str!("fixtures/folders.json").to_string()),
            (
                200,
                include_str!("fixtures/folderless_lists.json").to_string(),
            ),
            (200, r#"{"folders":[]}"#.to_string()),
            (200, r#"{"lists":[]}"#.to_string()),
            (200, include_str!("fixtures/tasks_page.json").to_string()),
            (200, page1.to_string()),
        ])
        .await;
        let client = ClickUpClient::with_base_url("pk_test_token", &base).unwrap();
        let fetched = fetch_cycle(&client, TEAM).await.unwrap();
        assert_eq!(fetched.spaces.len(), 2);
        assert_eq!(fetched.tasks.len(), 3);

        // The fetched cycle reconciles cleanly end to end.
        let conn = mirror_conn();
        reconcile_team(&conn, &fetched, Some(ME), 1000, None).unwrap();
        let tasks = mirror::read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(tasks.len(), 3);
    }

    // ── Echo + conflict tests (tasks 3.3) ──

    use crate::clickup::writeback::{WriteField, WritebackRegistry};

    /// 3.3 — own write value-match → no notification + entry cleared.
    #[test]
    fn own_status_write_suppresses_echo_cycle_notification() {
        let conn = mirror_conn();
        // Baseline seed: arms notifications.
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let reg = WritebackRegistry::default();
        // Simulate: we wrote status "en revisión - dev (pr)" for TASK_PARENT.
        // The fixture already has that status, so the next cycle reflects our
        // write — own echo.
        reg.record(
            TASK_PARENT,
            WriteField::Status,
            "en revisión - dev (pr)",
            Some("backlog"),
        );

        let cycle2 = fixture_cycle();
        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, Some(&reg)).unwrap();

        // Echo matched → no conflict events.
        assert!(
            outcome.write_conflicts.is_empty(),
            "own echo must not produce a conflict"
        );
        // Entry cleared after echo.
        assert!(
            reg.entries_for_task(TASK_PARENT).is_empty(),
            "registry entry must be cleared after echo"
        );
    }

    /// 3.3 — scalar conflict: server value ≠ written AND ≠ pre-write → conflict event.
    #[test]
    fn scalar_conflict_produces_write_conflict_event() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let reg = WritebackRegistry::default();
        // We wrote "complete", but the server now shows "en revisión - dev (pr)"
        // (which is neither what we wrote nor the pre-write "backlog").
        reg.record(TASK_PARENT, WriteField::Status, "complete", Some("backlog"));

        let cycle2 = fixture_cycle();
        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, Some(&reg)).unwrap();

        assert_eq!(
            outcome.write_conflicts.len(),
            1,
            "scalar conflict must produce exactly one conflict event"
        );
        let c = &outcome.write_conflicts[0];
        assert_eq!(c.task_id, TASK_PARENT);
        assert_eq!(c.your_value, "complete");
        assert_eq!(c.remote_value, "en revisión - dev (pr)");

        // Entry cleared after conflict surfaced.
        assert!(
            reg.entries_for_task(TASK_PARENT).is_empty(),
            "registry entry must be cleared after conflict"
        );
    }

    /// 3.3 — additive field divergence (assignees) → no conflict warning.
    #[test]
    fn additive_assignee_divergence_produces_no_conflict() {
        let conn = mirror_conn();
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        let reg = WritebackRegistry::default();
        // We added assignee 99999, but server still shows original list —
        // additive divergence, no scalar conflict.
        reg.record(TASK_PARENT, WriteField::Assignees, "[99999]", Some("[]"));

        let cycle2 = fixture_cycle();
        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, Some(&reg)).unwrap();

        assert!(
            outcome.write_conflicts.is_empty(),
            "additive field divergence must not produce a conflict event"
        );
    }

    /// 3.3 REGRESSION — own assignment-write does not self-notify.
    ///
    /// If we wrote an assignee addition for TASK_PARENT and the server
    /// reflects exactly what we wrote, the echo check must suppress the
    /// new-assignment notification (cross-change ordering guarantee).
    #[test]
    fn own_assignment_write_does_not_self_notify() {
        let conn = mirror_conn();
        // Baseline seed: arms notifications and establishes prev_assigned.
        reconcile_team(&conn, &fixture_cycle(), Some(ME), 1000, None).unwrap();

        // Simulate: a task NOT previously assigned to ME now appears assigned,
        // and we have a recent_writes entry saying WE made the assignment.
        let reg = WritebackRegistry::default();
        // TASK_SUB had no assignees in the baseline. Add ME as assignee in
        // cycle2, with a matching registry entry to mark it as our own write.
        // Use the canonical form: sorted comma-separated ids (same as
        // task_field_value and the write command recording).
        let written_assignees = ME.to_string();
        reg.record(
            TASK_SUB,
            WriteField::Assignees,
            &written_assignees,
            Some(""),
        );

        let mut cycle2 = fixture_cycle();
        if let Some(sub) = cycle2.tasks.iter_mut().find(|t| t.id == TASK_SUB) {
            sub.assignees = vec![model::User {
                id: Some(ME),
                username: Some("Felipe".into()),
                ..Default::default()
            }];
        }

        let outcome = reconcile_team(&conn, &cycle2, Some(ME), 2000, Some(&reg)).unwrap();

        assert!(
            outcome.notifications_armed,
            "post-baseline: notifications must be armed"
        );
        assert!(
            outcome.newly_assigned.is_empty(),
            "own assignment-write must not appear in newly_assigned (self-notify suppressed)"
        );
        let mut fx = RecordingEffects::default();
        apply_outcome_effects(&outcome, true, &mut fx);
        assert!(
            fx.notifications.is_empty(),
            "own assignment-write must not fire a notification"
        );
    }
}
