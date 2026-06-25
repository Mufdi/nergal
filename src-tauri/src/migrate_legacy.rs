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
        migrate_cc_mcp_registration(&home);
    }
    rewrite_known_configs();
    clean_legacy_scheme_handler();
    remove_legacy_cache_dir();
}

/// Best-effort removal of the legacy `~/.cache/cluihud` log directory. nergal
/// writes its log to `~/.cache/nergal/nergal.log` (see `lib.rs`), so an upgraded
/// install leaves a sibling `~/.cache/cluihud/` behind. XDG cache is regenerable
/// diagnostic output, not user data — delete it rather than merge. The
/// `.cache/cluihud` RENAME_FRAGMENT only rewrites that string *inside* a config
/// file, but the log path is computed at runtime and never persisted, so the
/// stale directory needs this dedicated cleanup.
fn remove_legacy_cache_dir() {
    let Some(cache) = dirs::cache_dir() else {
        return;
    };
    let old = cache.join(OLD);
    if !old.is_dir() {
        return;
    }
    match std::fs::remove_dir_all(&old) {
        Ok(()) => tracing::info!("removed legacy cache dir {}", old.display()),
        Err(e) => tracing::warn!("could not remove legacy cache dir {} ({e})", old.display()),
    }
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
    let rewritten = apply_fragments(&body);
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

/// `~/.claude.json` carries Claude Code's entire state blob (command history,
/// projects, pasted contents), so a blanket fragment rewrite would corrupt user
/// data — a `cluihud hook` string in someone's history would be silently edited.
/// It is therefore NOT a `rewrite_known_configs` target. The one nergal-owned key
/// is the MCP server entry `mcpServers.cluihud`, written by `mcp::registration`.
/// Remove it surgically (touch only that key): `register()` runs moments later at
/// startup and recreates the `nergal` entry with the correct command when the
/// server is enabled; a disabled server correctly leaves no entry. Without this,
/// the stale `cluihud` entry (→ `/usr/bin/cluihud`, gone after upgrade) sits
/// beside the new `nergal` one and Claude Code fails to spawn it every session.
fn migrate_cc_mcp_registration(home: &Path) {
    let path = home.join(".claude.json");
    let Ok(body) = std::fs::read_to_string(&path) else {
        return; // absent — never registered the MCP server under the old name
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&body) else {
        tracing::warn!("could not parse {} for mcp migration", path.display());
        return;
    };
    if !remove_legacy_cc_entry(&mut root) {
        return; // no legacy entry — nothing to migrate
    }
    match serde_json::to_string_pretty(&root) {
        Ok(pretty) => match crate::atomic_write::write_atomic(&path, pretty) {
            Ok(()) => tracing::info!("removed legacy cluihud mcp entry from {}", path.display()),
            Err(e) => tracing::warn!("could not rewrite {} ({e:#})", path.display()),
        },
        Err(e) => tracing::warn!("mcp migration serialize failed: {e}"),
    }
}

/// Pure: drop `mcpServers.cluihud` from a parsed `~/.claude.json` root, leaving
/// every other server and top-level key intact. Returns true if it removed the
/// entry. Mirrors `mcp::registration::apply_cc_registration`'s deregister path.
fn remove_legacy_cc_entry(root: &mut serde_json::Value) -> bool {
    root.get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .map(|servers| servers.remove(OLD).is_some())
        .unwrap_or(false)
}

/// The legacy `x-scheme-handler/cluihud=` mimeapps key.
const LEGACY_SCHEME_LINE: &str = "x-scheme-handler/cluihud=";

/// Drop the orphaned `cluihud://` scheme association from the user's
/// `mimeapps.list`. After the rename nergal registers only `nergal://`; the old
/// key lingers (pointing at the now-renamed `Nergal.desktop`, so not even
/// broken) but nothing emits `cluihud://` anymore, so it is inert. Remove it for
/// hygiene. Both XDG locations are checked. Only system-level state (the old
/// `.deb`'s `/usr/bin/cluihud` + `/usr/share/applications/*.desktop`, if a
/// renamed package left one behind) is out of scope — that is the package
/// manager's job (`apt remove`), not something an unprivileged process touches.
fn clean_legacy_scheme_handler() {
    let mut paths = Vec::new();
    if let Some(cfg) = dirs::config_dir() {
        paths.push(cfg.join("mimeapps.list"));
    }
    if let Some(data) = dirs::data_dir() {
        paths.push(data.join("applications").join("mimeapps.list"));
    }
    for path in paths {
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue; // absent — never registered the scheme there
        };
        let Some(cleaned) = strip_legacy_scheme(&body) else {
            continue; // no legacy key present
        };
        match crate::atomic_write::write_atomic(&path, cleaned) {
            Ok(()) => tracing::info!(
                "removed legacy cluihud scheme handler from {}",
                path.display()
            ),
            Err(e) => tracing::warn!("could not clean {} ({e:#})", path.display()),
        }
    }
}

/// Pure: strip every line whose key is exactly `x-scheme-handler/cluihud`,
/// leaving all other associations and section headers intact. Returns the new
/// body if anything changed, else None. The trailing newline is preserved.
fn strip_legacy_scheme(body: &str) -> Option<String> {
    if !body.contains(LEGACY_SCHEME_LINE) {
        return None;
    }
    let mut out = body
        .lines()
        .filter(|line| !line.trim_start().starts_with(LEGACY_SCHEME_LINE))
        .collect::<Vec<_>>()
        .join("\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// nergal-OWNED substrings only. A blanket `cluihud`→`nergal` would corrupt
/// legitimate user data that happens to contain the word — e.g. a workspace at
/// `~/Projects/cluihud` or a Claude Code permission entry referencing it. Each
/// fragment is anchored to a path/key/command nergal owns, so it never matches
/// such a path (`/Projects/cluihud` has no `.config/`, `/usr/bin/`, `hook `,
/// etc. prefix). Order is irrelevant — the fragments don't overlap.
const RENAME_FRAGMENTS: &[(&str, &str)] = &[
    ("cluihud hook ", "nergal hook "),
    ("cluihud-conditional.sh", "nergal-conditional.sh"),
    ("cluihud-state.json", "nergal-state.json"),
    ("cluihud-mcp.sock", "nergal-mcp.sock"),
    ("cluihud.sock", "nergal.sock"),
    ("cluihud-plan-", "nergal-plan-"),
    ("cluihud-ask-", "nergal-ask-"),
    ("/usr/bin/cluihud", "/usr/bin/nergal"),
    (".config/cluihud", ".config/nergal"),
    (".cache/cluihud", ".cache/nergal"),
    (".cluihud-active", ".nergal-active"),
    ("[mcp_servers.cluihud]", "[mcp_servers.nergal]"),
    ("mcp_servers.cluihud.", "mcp_servers.nergal."),
    ("\"cluihud\":", "\"nergal\":"),
    ("CLUIHUD_SESSION_ID", "NERGAL_SESSION_ID"),
    ("CLUIHUD_AGENT_ID", "NERGAL_AGENT_ID"),
];

fn apply_fragments(body: &str) -> String {
    let mut out = body.to_string();
    for (from, to) in RENAME_FRAGMENTS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

/// Rewrite nergal-owned legacy names in place across the config files that
/// reference them: Claude Code `settings.json` (hook commands + wrapper path),
/// the Claude Code statusline script (user-authored, pipes its JSON snapshot to
/// `cluihud hook send agent-status` — left stale, the CC status bar goes blank),
/// the codex/opencode MCP registrations, and nergal's own `config.json` (which
/// persists absolute `hook_socket_path` / `scratchpad_path` etc. that pointed
/// into the old config dir). Targeted fragments only — never a blanket replace.
fn rewrite_known_configs() {
    let mut targets = Vec::new();
    if let Some(home) = dirs::home_dir() {
        targets.push(home.join(".claude").join("settings.json"));
        targets.push(home.join(".claude").join("statusline-command.sh"));
        targets.push(home.join(".codex").join("config.toml"));
    }
    if let Some(cfg) = dirs::config_dir() {
        targets.push(cfg.join("opencode").join("opencode.json"));
        targets.push(cfg.join("nergal").join("config.json"));
    }
    for path in targets {
        rewrite_file_in_place(&path);
    }
}

fn rewrite_file_in_place(path: &Path) {
    let Ok(body) = std::fs::read_to_string(path) else {
        return; // absent or unreadable — nothing to migrate
    };
    let rewritten = apply_fragments(&body);
    if rewritten == body {
        return; // no nergal-owned legacy fragment present
    }
    match crate::atomic_write::write_atomic(path, rewritten) {
        Ok(()) => tracing::info!("rewrote legacy names in {}", path.display()),
        Err(e) => tracing::warn!("could not rewrite {} ({e:#})", path.display()),
    }
}

/// Copy OS-keyring secrets from the legacy `cluihud` service to `nergal`,
/// non-destructively. ClickUp + Linear tokens live in the keyring (service was
/// renamed by the sweep), so after upgrading the app looks under `nergal` and
/// finds nothing. `linear_org_ids` are the per-workspace key accounts
/// (`linear-token::<org>`) read from the mirror; the bare `linear-token` covers
/// the legacy single-key store.
///
/// MUST run off the main thread, AFTER startup — keyring access blocks on
/// D-Bus, and a synchronous stall during boot reproduces the journald
/// ghost-window class of bug. Idempotent: skips any account already present
/// under `nergal`.
/// Returns true if at least one secret was recovered, so the caller can restart
/// the pollers (which already parked on "no token" at boot before this ran).
pub fn migrate_keyring(linear_org_ids: &[String]) -> bool {
    let mut accounts = vec!["clickup-token".to_string(), "linear-token".to_string()];
    for org in linear_org_ids {
        accounts.push(format!("linear-token::{org}"));
    }
    let mut recovered = false;
    for account in &accounts {
        recovered |= copy_keyring_secret(account);
    }
    recovered
}

fn copy_keyring_secret(account: &str) -> bool {
    let (Ok(old), Ok(new)) = (
        keyring::Entry::new(OLD, account),
        keyring::Entry::new(NEW, account),
    ) else {
        return false;
    };
    // Non-destructive: never overwrite a secret the user already set under the
    // new service (e.g. a token re-entered after the rename).
    if new.get_password().is_ok() {
        return false;
    }
    // NoEntry (nothing to migrate) or a transient keyring error → leave it.
    if let Ok(secret) = old.get_password() {
        match new.set_password(&secret) {
            Ok(()) => {
                tracing::info!("recovered legacy keyring secret for {account}");
                return true;
            }
            Err(e) => tracing::warn!("keyring migrate {account} write failed: {e}"),
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_statusline_hook_command() {
        // The CC statusline script pipes its AgentStatus JSON to the nergal CLI.
        // The legacy `cluihud hook send` must become `nergal hook send` so the
        // status bar keeps receiving snapshots after the rename.
        let body = "}' 2>/dev/null | cluihud hook send agent-status 2>/dev/null &";
        let out = apply_fragments(body);
        assert_eq!(
            out,
            "}' 2>/dev/null | nergal hook send agent-status 2>/dev/null &"
        );
    }

    #[test]
    fn apply_fragments_is_idempotent() {
        let once = apply_fragments("cluihud hook send agent-status");
        assert_eq!(apply_fragments(&once), once);
    }

    #[test]
    fn leaves_bare_cluihud_paths_untouched() {
        // A workspace path that merely contains the word must NOT be rewritten —
        // only nergal-owned anchored fragments are.
        let body = "cd /home/felipe/Projects/cluihud && ls";
        assert_eq!(apply_fragments(body), body);
    }

    #[test]
    fn removes_legacy_cc_mcp_entry_only() {
        // The orphan `cluihud` MCP entry must go; every other server and the
        // surrounding state blob must survive untouched.
        let mut root = serde_json::json!({
            "mcpServers": {
                "cluihud": { "command": "/usr/bin/cluihud", "args": ["mcp"] },
                "other": { "command": "x" }
            },
            "projects": { "/home/felipe/Projects/cluihud": { "history": ["cluihud hook send"] } }
        });
        assert!(remove_legacy_cc_entry(&mut root));
        assert!(root["mcpServers"].get("cluihud").is_none());
        assert_eq!(root["mcpServers"]["other"]["command"], "x");
        // History inside the state blob is data, not config — must be preserved verbatim.
        assert_eq!(
            root["projects"]["/home/felipe/Projects/cluihud"]["history"][0],
            "cluihud hook send"
        );
    }

    #[test]
    fn cc_mcp_migration_noop_when_absent() {
        let mut root = serde_json::json!({ "mcpServers": { "nergal": { "command": "c" } } });
        assert!(!remove_legacy_cc_entry(&mut root));
        let mut no_servers = serde_json::json!({ "foo": 1 });
        assert!(!remove_legacy_cc_entry(&mut no_servers));
    }

    #[test]
    fn strips_only_legacy_scheme_line() {
        let body = "[Added Associations]\n\
                    x-scheme-handler/cluihud=Nergal.desktop;\n\
                    x-scheme-handler/nergal=Nergal.desktop;\n\
                    text/html=firefox.desktop;\n\n\
                    [Default Applications]\n\
                    x-scheme-handler/cluihud=Nergal.desktop\n";
        let out = strip_legacy_scheme(body).expect("changed");
        assert!(!out.contains("cluihud"));
        // The live scheme and an unrelated association survive, headers intact.
        assert!(out.contains("x-scheme-handler/nergal=Nergal.desktop;"));
        assert!(out.contains("text/html=firefox.desktop;"));
        assert!(out.contains("[Added Associations]"));
        assert!(out.contains("[Default Applications]"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn scheme_strip_noop_when_absent() {
        assert!(
            strip_legacy_scheme("[Default Applications]\nx-scheme-handler/nergal=Nergal.desktop\n")
                .is_none()
        );
    }
}
