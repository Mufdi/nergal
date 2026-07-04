//! Closed-out marker trio shared by the ClickUp and Linear mirrors.
//!
//! Both trackers persist a purely local "worked & closed from a session"
//! marker (separate from whatever status the upstream tracker holds) in a
//! tracker-specific table — `clickup_closed_out (task_id, closed_at)` /
//! `linear_closed_out (issue_id, closed_at)`. The three operations are
//! identical modulo table + id-column name, so they're parametrized free
//! functions rather than duplicated per tracker.
//!
//! `table` and `id_col` are interpolated into the SQL text via `format!`
//! rather than bound as `?`-params — sqlite doesn't allow binding identifiers,
//! only values. This is safe because every call site passes a compile-time
//! string literal (`"clickup_closed_out"`/`"task_id"` etc.), never anything
//! derived from user input.

use anyhow::Result;
use rusqlite::{Connection, params};

/// Record that an item was closed out from a session.
///
/// `ON CONFLICT DO UPDATE` so re-closing the same item (e.g. after a crash
/// and reopen) is safe: it just refreshes the timestamp.
pub fn mark_closed_out(
    conn: &Connection,
    table: &str,
    id_col: &str,
    id: &str,
    closed_at: i64,
) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO {table} ({id_col}, closed_at) VALUES (?1, ?2) \
             ON CONFLICT({id_col}) DO UPDATE SET closed_at=?2"
        ),
        params![id, closed_at],
    )?;
    Ok(())
}

/// All ids that were closed out from a session, oldest-closed first.
///
/// Deterministic `ORDER BY closed_at` on both trackers (ClickUp previously
/// relied on unordered insertion order) — both consumers only use this as a
/// membership set (`.includes()` / `Set`), so ordering it is behavior-preserving.
pub fn read_closed_out(conn: &Connection, table: &str, id_col: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("SELECT {id_col} FROM {table} ORDER BY closed_at"))?;
    let ids = stmt
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(ids)
}

/// Remove the worked-closed marker for an item, allowing it to be re-closed later.
pub fn unmark_closed_out(conn: &Connection, table: &str, id_col: &str, id: &str) -> Result<()> {
    conn.execute(
        &format!("DELETE FROM {table} WHERE {id_col} = ?1"),
        params![id],
    )?;
    Ok(())
}

// ── Tests (consolidated from clickup::mirror + linear::mirror) ──

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "closed_out_test";
    const ID_COL: &str = "item_id";

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE} ({ID_COL} TEXT PRIMARY KEY, closed_at INTEGER NOT NULL);"
        ))
        .unwrap();
        conn
    }

    #[test]
    fn closed_out_marker_round_trips_and_is_idempotent() {
        let conn = test_conn();
        assert!(read_closed_out(&conn, TABLE, ID_COL).unwrap().is_empty());
        mark_closed_out(&conn, TABLE, ID_COL, "item-a", 1000).unwrap();
        mark_closed_out(&conn, TABLE, ID_COL, "item-b", 1001).unwrap();
        // Re-mark updates closed_at without duplicating the row.
        mark_closed_out(&conn, TABLE, ID_COL, "item-a", 2000).unwrap();
        let ids = read_closed_out(&conn, TABLE, ID_COL).unwrap();
        assert_eq!(ids, vec!["item-b".to_string(), "item-a".to_string()]);

        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {TABLE}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "re-close must not duplicate the row");
    }

    #[test]
    fn unmark_closed_out_removes_marker() {
        let conn = test_conn();
        mark_closed_out(&conn, TABLE, ID_COL, "item-x", 5000).unwrap();
        assert_eq!(
            read_closed_out(&conn, TABLE, ID_COL).unwrap(),
            vec!["item-x".to_string()]
        );
        unmark_closed_out(&conn, TABLE, ID_COL, "item-x").unwrap();
        assert!(read_closed_out(&conn, TABLE, ID_COL).unwrap().is_empty());
        // Idempotent: clearing an absent marker is a no-op, not an error.
        unmark_closed_out(&conn, TABLE, ID_COL, "item-x").unwrap();
    }
}
