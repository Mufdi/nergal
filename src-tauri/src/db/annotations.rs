use super::*;

impl Database {
    // ── Annotations ──

    #[allow(clippy::too_many_arguments)] // Mirrors the column list 1:1; collapsing to a struct adds noise.
    pub fn save_annotation(
        &self,
        id: &str,
        session_id: &str,
        ann_type: &str,
        target: &str,
        content: &str,
        start_meta: &str,
        end_meta: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO annotations (id, session_id, type, target, content, start_meta, end_meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, session_id, ann_type, target, content, start_meta, end_meta],
        )?;
        Ok(())
    }

    pub fn get_annotations(&self, session_id: &str) -> Result<Vec<AnnotationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, type, target, content, start_meta, end_meta, created_at FROM annotations WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([session_id], |row| {
                Ok(AnnotationRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    ann_type: row.get(2)?,
                    target: row.get(3)?,
                    content: row.get(4)?,
                    start_meta: row.get(5)?,
                    end_meta: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn delete_annotation(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM annotations WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_annotations(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM annotations WHERE session_id = ?1",
            [session_id],
        )?;
        Ok(())
    }

    // ── Spec annotations ──

    #[allow(clippy::too_many_arguments)] // Mirrors the column list 1:1; collapsing to a struct adds noise.
    pub fn save_spec_annotation(
        &self,
        id: &str,
        spec_key: &str,
        ann_type: &str,
        target: &str,
        content: &str,
        start_meta: &str,
        end_meta: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO spec_annotations (id, spec_key, type, target, content, start_meta, end_meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, spec_key, ann_type, target, content, start_meta, end_meta],
        )?;
        Ok(())
    }

    pub fn get_spec_annotations(&self, spec_key: &str) -> Result<Vec<SpecAnnotationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, spec_key, type, target, content, start_meta, end_meta, created_at FROM spec_annotations WHERE spec_key = ?1 ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([spec_key], |row| {
                Ok(SpecAnnotationRow {
                    id: row.get(0)?,
                    spec_key: row.get(1)?,
                    ann_type: row.get(2)?,
                    target: row.get(3)?,
                    content: row.get(4)?,
                    start_meta: row.get(5)?,
                    end_meta: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn delete_spec_annotation(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM spec_annotations WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_spec_annotations(&self, spec_key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM spec_annotations WHERE spec_key = ?1",
            [spec_key],
        )?;
        Ok(())
    }

    /// Group annotation counts by spec_key for a given LIKE prefix (e.g. "my-change/%").
    pub fn count_spec_annotations_by_prefix(
        &self,
        prefix_like: &str,
    ) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT spec_key, COUNT(*) FROM spec_annotations WHERE spec_key LIKE ?1 GROUP BY spec_key",
        )?;
        let rows = stmt
            .query_map([prefix_like], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}
