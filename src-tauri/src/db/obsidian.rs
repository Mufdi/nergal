use super::*;

impl Database {
    // ── Obsidian Config (obsidian-bridge change, migration 008) ──

    pub fn get_obsidian_config(
        &self,
        workspace_id: &str,
    ) -> Result<Option<crate::obsidian::config::ObsidianConfig>> {
        let result = self.conn.query_row(
            "SELECT vault_root, vault_name, session_log_path, quick_capture_path, \
                    moc_path, templates_path, backlinks_enabled, render_wikilinks, \
                    search_subdir \
             FROM obsidian_config WHERE workspace_id = ?1",
            [workspace_id],
            |r| {
                Ok(crate::obsidian::config::ObsidianConfig {
                    vault_root: r.get::<_, Option<String>>(0)?,
                    vault_name: r.get::<_, Option<String>>(1)?,
                    session_log_path: r.get::<_, Option<String>>(2)?,
                    quick_capture_path: r.get::<_, Option<String>>(3)?,
                    moc_path: r.get::<_, Option<String>>(4)?,
                    templates_path: r.get::<_, Option<String>>(5)?,
                    backlinks_enabled: r.get::<_, i64>(6)? != 0,
                    render_wikilinks: r.get::<_, i64>(7)? != 0,
                    search_subdir: r.get::<_, Option<String>>(8)?,
                })
            },
        );
        match result {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_obsidian_config(
        &self,
        workspace_id: &str,
        cfg: &crate::obsidian::config::ObsidianConfig,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO obsidian_config (workspace_id, vault_root, vault_name, \
                session_log_path, quick_capture_path, moc_path, templates_path, \
                backlinks_enabled, render_wikilinks, search_subdir, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
             ON CONFLICT(workspace_id) DO UPDATE SET \
                vault_root=?2, vault_name=?3, session_log_path=?4, \
                quick_capture_path=?5, moc_path=?6, templates_path=?7, \
                backlinks_enabled=?8, render_wikilinks=?9, search_subdir=?10, \
                updated_at=?11",
            params![
                workspace_id,
                cfg.vault_root,
                cfg.vault_name,
                cfg.session_log_path,
                cfg.quick_capture_path,
                cfg.moc_path,
                cfg.templates_path,
                cfg.backlinks_enabled as i64,
                cfg.render_wikilinks as i64,
                cfg.search_subdir,
                now_secs(),
            ],
        )?;
        Ok(())
    }

    pub fn delete_obsidian_config(&self, workspace_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM obsidian_config WHERE workspace_id = ?1",
            [workspace_id],
        )?;
        Ok(())
    }
}
