//! One-time migration of legacy `cluihud` local state to `nergal`.
//!
//! The project was renamed cluihud → nergal (brand AND internal). Existing
//! installs carry machine-local state under the old names: the config dir
//! (DB + settings), Claude Code hook entries in `settings.json`, the activity
//! sentinel, the hook state file, the conditional-hook wrapper, and the
//! codex/opencode MCP registrations. This runs once at startup, BEFORE config
//! and DB load, and moves each to the new name so upgrading users lose nothing.
//!
//! Invariants:
//! - **idempotent**: a second run is a no-op (every step is guarded on the old
//!   name existing / the file still containing it).
//! - **non-destructive**: never clobbers existing new-name state — a dir/file
//!   move is skipped when the target already exists; moves are renames, not
//!   deletes, so a partial failure never loses data.
//! - **best-effort**: every step logs and continues; nothing here blocks
//!   startup.
//!
//! This module is the ONE place the literal `cluihud` legitimately survives in
//! live code — it must read the old names to migrate away from them.

use std::path::Path;

const OLD: &str = "cluihud";
const NEW: &str = "nergal";

pub fn run() {
    migrate_config_dir();
    if let Some(home) = dirs::home_dir() {
        migrate_claude_state(&home);
        migrate_conditional_wrapper(&home);
        remove_stale_sentinel(&home);
    }
    rewrite_known_configs();
}

/// `~/.config/cluihud` → `~/.config/nergal`. The data-critical move: the SQLite
/// DB (incl. its `-wal`/`-shm` siblings, which carry not-yet-checkpointed
/// writes), config.json, and session state live here. The new dir may already
/// exist with stray content (a test or a partial run), so we MERGE old into new
/// entry by entry rather than rename the dir wholesale, renaming the
/// `cluihud.db*` files to `nergal.db*` (the code now opens `nergal/nergal.db`).
/// `nergal/nergal.db` existing is the done-sentinel; never overwrite a file
/// already present in the new dir.
fn migrate_config_dir() {
    let Some(base) = dirs::config_dir() else {
        return;
    };
    let old = base.join(OLD);
    let new = base.join(NEW);
    if !old.is_dir() || new.join("nergal.db").exists() {
        return; // nothing to migrate / already migrated
    }
    if let Err(e) = std::fs::create_dir_all(&new) {
        tracing::warn!("could not create {} for migration: {e}", new.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(&old) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // `cluihud.db`, `cluihud.db-wal`, `cluihud.db-shm` → `nergal.db*`.
        let dest_name = if name.starts_with(OLD) {
            name.replacen(OLD, NEW, 1)
        } else {
            name.to_string()
        };
        let dest = new.join(&dest_name);
        if dest.exists() {
            continue; // never clobber existing new-dir state
        }
        if let Err(e) = std::fs::rename(entry.path(), &dest) {
            tracing::warn!("migrate {} -> {} failed: {e}", name, dest.display());
        }
    }
    // Drop the old dir if the moves emptied it (best-effort; leftover keeps it).
    let _ = std::fs::remove_dir(&old);
    tracing::info!(
        "migrated config dir contents {} -> {}",
        old.display(),
        new.display()
    );
}

/// `~/.claude/cluihud-state.json` → `~/.claude/nergal-state.json`.
fn migrate_claude_state(home: &Path) {
    let old = home.join(".claude").join("cluihud-state.json");
    let new = home.join(".claude").join("nergal-state.json");
    rename_if_absent(&old, &new);
}

/// `~/.claude/hooks/cluihud-conditional.sh` → `nergal-conditional.sh`. The
/// wrapper is user-authored (per docs/hooks.md), so its body references the old
/// binary/sentinel — rewrite the content as we move it. settings.json's pointer
/// to it is fixed up by `rewrite_known_configs`.
fn migrate_conditional_wrapper(home: &Path) {
    let dir = home.join(".claude").join("hooks");
    let old = dir.join("cluihud-conditional.sh");
    let new = dir.join("nergal-conditional.sh");
    if !old.is_file() || new.exists() {
        return;
    }
    let Ok(body) = std::fs::read_to_string(&old) else {
        tracing::warn!("could not read {} for migration", old.display());
        return;
    };
    let rewritten = body
        .replace(OLD, NEW)
        .replace(&OLD.to_uppercase(), &NEW.to_uppercase());
    if std::fs::write(&new, rewritten).is_err() {
        tracing::warn!("could not write {}", new.display());
        return;
    }
    // Preserve the executable bit the wrapper needs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&old) {
            let _ = std::fs::set_permissions(
                &new,
                std::fs::Permissions::from_mode(meta.permissions().mode()),
            );
        }
    }
    let _ = std::fs::remove_file(&old);
    tracing::info!("migrated hook wrapper -> {}", new.display());
}

/// Stale `~/.cluihud-active` sentinel from a prior run (the old app removed it
/// on Drop, but a crash could leave it). The new `SentinelGuard` writes
/// `~/.nergal-active`; just clear the old one.
fn remove_stale_sentinel(home: &Path) {
    let old = home.join(".cluihud-active");
    if old.exists() {
        let _ = std::fs::remove_file(&old);
    }
}

/// Files whose `cluihud` occurrences are all nergal-owned entries (hook command
/// strings, MCP server key + binary path), so a scoped string replace is both
/// safe and complete — no foreign content matches the literal.
fn rewrite_known_configs() {
    let mut targets = Vec::new();
    if let Some(home) = dirs::home_dir() {
        targets.push(home.join(".claude").join("settings.json"));
        targets.push(home.join(".codex").join("config.toml"));
    }
    if let Some(cfg) = dirs::config_dir() {
        targets.push(cfg.join("opencode").join("opencode.json"));
    }
    for path in targets {
        rewrite_file_in_place(&path);
    }
}

fn rewrite_file_in_place(path: &Path) {
    let Ok(body) = std::fs::read_to_string(path) else {
        return; // absent or unreadable — nothing to migrate
    };
    if !body.contains(OLD) && !body.contains(&OLD.to_uppercase()) {
        return;
    }
    let rewritten = body
        .replace(OLD, NEW)
        .replace(&OLD.to_uppercase(), &NEW.to_uppercase());
    match std::fs::write(path, rewritten) {
        Ok(()) => tracing::info!("rewrote legacy names in {}", path.display()),
        Err(e) => tracing::warn!("could not rewrite {} ({e})", path.display()),
    }
}

fn rename_if_absent(old: &Path, new: &Path) {
    if old.exists() && !new.exists() {
        match std::fs::rename(old, new) {
            Ok(()) => tracing::info!("migrated {} -> {}", old.display(), new.display()),
            Err(e) => tracing::warn!(
                "migration {} -> {} failed: {e}",
                old.display(),
                new.display()
            ),
        }
    }
}
