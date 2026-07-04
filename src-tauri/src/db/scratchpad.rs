use super::*;

impl Database {
    // ── Scratchpad ──

    /// Upsert a scratchpad tab's metadata. `last_modified` is bumped to now.
    pub fn upsert_scratchpad_meta(&self, tab_id: &str, position: i64) -> Result<()> {
        let now = now_secs() as i64;
        self.conn.execute(
            "INSERT INTO scratchpad_meta (tab_id, position, created_at, last_modified) \
             VALUES (?1, ?2, ?3, ?3) \
             ON CONFLICT(tab_id) DO UPDATE SET position=?2, last_modified=?3",
            params![tab_id, position, now],
        )?;
        Ok(())
    }

    pub fn delete_scratchpad_meta(&self, tab_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM scratchpad_meta WHERE tab_id = ?1", [tab_id])?;
        Ok(())
    }

    pub fn list_scratchpad_meta(&self) -> Result<Vec<(String, i64, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tab_id, position, created_at, last_modified FROM scratchpad_meta \
             ORDER BY position ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Replace all scratchpad metadata in a single transaction (used on
    /// path change and watcher-driven reconciliation).
    pub fn replace_scratchpad_meta(&self, entries: &[(String, i64)]) -> Result<()> {
        let now = now_secs() as i64;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM scratchpad_meta", [])?;
        for (tab_id, position) in entries {
            tx.execute(
                "INSERT INTO scratchpad_meta (tab_id, position, created_at, last_modified) \
                 VALUES (?1, ?2, ?3, ?3)",
                params![tab_id, position, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove rows whose tab_id is not in the provided set (zombie cleanup
    /// after watcher emit).
    pub fn prune_scratchpad_meta(&self, keep: &[String]) -> Result<()> {
        if keep.is_empty() {
            self.conn.execute("DELETE FROM scratchpad_meta", [])?;
            return Ok(());
        }
        let placeholders = std::iter::repeat_n("?", keep.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM scratchpad_meta WHERE tab_id NOT IN ({placeholders})");
        let params: Vec<&dyn rusqlite::ToSql> =
            keep.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        self.conn.execute(&sql, params.as_slice())?;
        Ok(())
    }
}
