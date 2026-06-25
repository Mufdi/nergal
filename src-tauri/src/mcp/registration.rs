//! Idempotent registration of the `nergal mcp` shim into agent MCP configs.
//!
//! Design Decision 7: the registered command pins the **installed absolute
//! path** `/usr/bin/nergal` (not a `$PATH` lookup, which would bake in the
//! `~/.cargo/bin/nergal` shadow CLAUDE.md documents). Registration is
//! idempotent (no duplicate entries); deregistration is best-effort at disable
//! time. An orphaned entry after uninstall degrades to a structured error when
//! the agent tries to spawn the missing binary — not a hard failure.
//!
//! Wires every agent whose MCP config schema was verified against a live
//! install (2026-06-21): Claude Code (`~/.claude.json` `mcpServers`, JSON),
//! Codex (`~/.codex/config.toml` `[mcp_servers.<name>]`, TOML via `toml_edit`
//! to preserve the user's formatting/comments), and OpenCode
//! (`~/.config/opencode/opencode.json` `mcp.<name>` `{type:"local",command,
//! enabled}`, JSON). **Pi** is intentionally NOT registered: its CLI exposes no
//! MCP-server mechanism (no `pi mcp` subcommand; settings.json has no MCP key —
//! it uses `pi install` extensions), so there is nothing to wire. Each agent is
//! best-effort: a failure for one (or a missing/corrupt config) is logged and
//! does not block the others or the toggle.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// The MCP server key nergal owns inside an agent config.
const SERVER_KEY: &str = "nergal";

/// Absolute command the agent spawns for the shim. Pins `/usr/bin/nergal`
/// (the `.deb`/`.rpm` location) when present; falls back to the running
/// executable so a dev build still registers a working path.
fn shim_command() -> String {
    let installed = PathBuf::from("/usr/bin/nergal");
    if installed.exists() {
        return installed.to_string_lossy().into_owned();
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "nergal".to_string())
}

fn cc_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude.json"))
}

/// Mutate a parsed agent-config root in place: add or remove the `nergal`
/// entry under `mcpServers`. Returns true if the document changed. Pure (no
/// I/O) so it is unit-tested directly.
fn apply_cc_registration(root: &mut Value, command: &str, register: bool) -> bool {
    // Coerce a non-object (or absent) root into an object so a corrupt
    // `~/.claude.json` can't make us panic; `as_object_mut` is then infallible
    // but we still avoid `expect` (project rule) via a graceful bail.
    if !root.is_object() {
        *root = json!({});
    }
    let Some(obj) = root.as_object_mut() else {
        return false;
    };

    if register {
        let desired = json!({ "command": command, "args": ["mcp"] });
        let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
        if !servers.is_object() {
            *servers = json!({});
        }
        let Some(servers) = servers.as_object_mut() else {
            return false;
        };
        if servers.get(SERVER_KEY) == Some(&desired) {
            return false; // already exactly registered — idempotent no-op
        }
        servers.insert(SERVER_KEY.to_string(), desired);
        true
    } else {
        let Some(servers) = obj.get_mut("mcpServers").and_then(|v| v.as_object_mut()) else {
            return false;
        };
        servers.remove(SERVER_KEY).is_some()
    }
}

fn rewrite_cc_config(register: bool) -> Result<()> {
    let Some(path) = cc_config_path() else {
        return Ok(()); // no home dir — nothing to do
    };
    // A missing config on deregister is a no-op; on register we create it.
    let mut root: Value = match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => {
            serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display()))?
        }
        _ => {
            if !register {
                return Ok(());
            }
            json!({})
        }
    };
    let changed = apply_cc_registration(&mut root, &shim_command(), register);
    if changed {
        let pretty = serde_json::to_string_pretty(&root)?;
        crate::atomic_write::write_atomic(&path, pretty)
            .with_context(|| format!("writing {}", path.display()))?;
        tracing::info!(
            "mcp: {} nergal in {}",
            if register {
                "registered"
            } else {
                "deregistered"
            },
            path.display()
        );
    }
    Ok(())
}

// ── Codex (`~/.codex/config.toml`, TOML) ──────────────────────────────────

fn codex_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("config.toml"))
}

/// Add/remove `[mcp_servers.nergal]` in a parsed Codex TOML doc, preserving
/// every other table, key, comment and whitespace. Returns true if it changed.
fn apply_codex_registration(
    doc: &mut toml_edit::DocumentMut,
    command: &str,
    register: bool,
) -> bool {
    use toml_edit::{Array, Item, Table, value};
    if register {
        // Idempotent: bail if the entry already pins this exact command + args.
        if let Some(existing) = doc
            .get("mcp_servers")
            .and_then(|i| i.as_table())
            .and_then(|t| t.get(SERVER_KEY))
            .and_then(|i| i.as_table())
        {
            let cmd_ok = existing.get("command").and_then(|i| i.as_str()) == Some(command);
            let args_ok = existing
                .get("args")
                .and_then(|i| i.as_array())
                .map(|a| a.len() == 1 && a.get(0).and_then(|v| v.as_str()) == Some("mcp"))
                .unwrap_or(false);
            if cmd_ok && args_ok {
                return false;
            }
        }
        let servers = doc
            .entry("mcp_servers")
            .or_insert(Item::Table(Table::new()));
        let Some(servers) = servers.as_table_mut() else {
            return false;
        };
        let mut entry = Table::new();
        entry["command"] = value(command);
        let mut args = Array::new();
        args.push("mcp");
        entry["args"] = value(args);
        servers.insert(SERVER_KEY, Item::Table(entry));
        true
    } else {
        let Some(servers) = doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) else {
            return false;
        };
        servers.remove(SERVER_KEY).is_some()
    }
}

fn rewrite_codex_config(register: bool) -> Result<()> {
    let Some(path) = codex_config_path() else {
        return Ok(());
    };
    let mut doc: toml_edit::DocumentMut = match std::fs::read_to_string(&path) {
        Ok(s) => s
            .parse()
            .with_context(|| format!("parsing {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if !register {
                return Ok(());
            }
            toml_edit::DocumentMut::new()
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if apply_codex_registration(&mut doc, &shim_command(), register) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        crate::atomic_write::write_atomic(&path, doc.to_string())
            .with_context(|| format!("writing {}", path.display()))?;
        tracing::info!(
            "mcp: {} nergal in {}",
            if register {
                "registered"
            } else {
                "deregistered"
            },
            path.display()
        );
    }
    Ok(())
}

// ── OpenCode (`~/.config/opencode/opencode.json`, JSON) ───────────────────

fn opencode_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("opencode").join("opencode.json"))
}

/// Add/remove the `nergal` entry under `mcp` in a parsed OpenCode config.
/// OpenCode's local-server shape (verified live): `{type:"local", command:[cmd,
/// args...], enabled:true}` with `command` a single array. Returns true if changed.
fn apply_opencode_registration(root: &mut Value, command: &str, register: bool) -> bool {
    if !root.is_object() {
        *root = json!({});
    }
    let Some(obj) = root.as_object_mut() else {
        return false;
    };
    if register {
        let desired = json!({ "type": "local", "command": [command, "mcp"], "enabled": true });
        let mcp = obj.entry("mcp").or_insert_with(|| json!({}));
        if !mcp.is_object() {
            *mcp = json!({});
        }
        let Some(mcp) = mcp.as_object_mut() else {
            return false;
        };
        if mcp.get(SERVER_KEY) == Some(&desired) {
            return false;
        }
        mcp.insert(SERVER_KEY.to_string(), desired);
        true
    } else {
        let Some(mcp) = obj.get_mut("mcp").and_then(|v| v.as_object_mut()) else {
            return false;
        };
        mcp.remove(SERVER_KEY).is_some()
    }
}

fn rewrite_opencode_config(register: bool) -> Result<()> {
    let Some(path) = opencode_config_path() else {
        return Ok(());
    };
    let mut root: Value = match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => {
            serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display()))?
        }
        Ok(_) => json!({ "$schema": "https://opencode.ai/config.json" }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if !register {
                return Ok(());
            }
            json!({ "$schema": "https://opencode.ai/config.json" })
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if apply_opencode_registration(&mut root, &shim_command(), register) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let pretty = serde_json::to_string_pretty(&root)?;
        crate::atomic_write::write_atomic(&path, pretty)
            .with_context(|| format!("writing {}", path.display()))?;
        tracing::info!(
            "mcp: {} nergal in {}",
            if register {
                "registered"
            } else {
                "deregistered"
            },
            path.display()
        );
    }
    Ok(())
}

/// Register the shim into every supported agent config. Claude is authoritative
/// (its result propagates); Codex + OpenCode are best-effort so a problem with
/// one agent's config never blocks enabling the server for the others.
pub fn register() -> Result<()> {
    if let Err(e) = rewrite_codex_config(true) {
        tracing::warn!("mcp: codex registration failed (best-effort): {e:#}");
    }
    if let Err(e) = rewrite_opencode_config(true) {
        tracing::warn!("mcp: opencode registration failed (best-effort): {e:#}");
    }
    rewrite_cc_config(true)
}

/// Remove the shim from every supported agent config (best-effort per agent).
pub fn deregister() -> Result<()> {
    if let Err(e) = rewrite_codex_config(false) {
        tracing::warn!("mcp: codex deregistration failed (best-effort): {e:#}");
    }
    if let Err(e) = rewrite_opencode_config(false) {
        tracing::warn!("mcp: opencode deregistration failed (best-effort): {e:#}");
    }
    rewrite_cc_config(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_adds_entry() {
        let mut root = json!({});
        assert!(apply_cc_registration(&mut root, "/usr/bin/nergal", true));
        assert_eq!(root["mcpServers"]["nergal"]["command"], "/usr/bin/nergal");
        assert_eq!(root["mcpServers"]["nergal"]["args"][0], "mcp");
    }

    #[test]
    fn register_is_idempotent() {
        let mut root = json!({});
        apply_cc_registration(&mut root, "/usr/bin/nergal", true);
        // Second identical registration reports no change.
        assert!(!apply_cc_registration(&mut root, "/usr/bin/nergal", true));
    }

    #[test]
    fn register_preserves_other_servers() {
        let mut root = json!({ "mcpServers": { "other": { "command": "x" } }, "foo": 1 });
        apply_cc_registration(&mut root, "/usr/bin/nergal", true);
        assert_eq!(root["mcpServers"]["other"]["command"], "x");
        assert_eq!(root["foo"], 1);
        assert!(root["mcpServers"]["nergal"].is_object());
    }

    #[test]
    fn deregister_removes_only_nergal() {
        let mut root =
            json!({ "mcpServers": { "nergal": { "command": "c" }, "other": { "command": "x" } } });
        assert!(apply_cc_registration(&mut root, "/usr/bin/nergal", false));
        assert!(root["mcpServers"].get("nergal").is_none());
        assert_eq!(root["mcpServers"]["other"]["command"], "x");
    }

    #[test]
    fn deregister_missing_is_noop() {
        let mut root = json!({ "mcpServers": { "other": {} } });
        assert!(!apply_cc_registration(&mut root, "/usr/bin/nergal", false));
    }

    #[test]
    fn register_into_non_object_root_resets() {
        let mut root = json!("garbage");
        assert!(apply_cc_registration(&mut root, "/usr/bin/nergal", true));
        assert!(root["mcpServers"]["nergal"].is_object());
    }

    // ── Codex (TOML) ──

    #[test]
    fn codex_register_adds_table_and_preserves_others() {
        let mut doc: toml_edit::DocumentMut =
            "[mcp_servers.other]\ncommand = \"x\"\nargs = [\"mcp\"]\n\n[tui]\ntheme = \"mono\"\n"
                .parse()
                .unwrap();
        assert!(apply_codex_registration(&mut doc, "/usr/bin/nergal", true));
        assert_eq!(
            doc["mcp_servers"]["nergal"]["command"].as_str(),
            Some("/usr/bin/nergal")
        );
        assert_eq!(
            doc["mcp_servers"]["nergal"]["args"][0].as_str(),
            Some("mcp")
        );
        // Untouched: the other server + the unrelated [tui] table.
        assert_eq!(doc["mcp_servers"]["other"]["command"].as_str(), Some("x"));
        assert_eq!(doc["tui"]["theme"].as_str(), Some("mono"));
    }

    #[test]
    fn codex_register_is_idempotent() {
        let mut doc: toml_edit::DocumentMut = String::new().parse().unwrap();
        assert!(apply_codex_registration(&mut doc, "/usr/bin/nergal", true));
        assert!(!apply_codex_registration(&mut doc, "/usr/bin/nergal", true));
    }

    #[test]
    fn codex_deregister_removes_only_nergal() {
        let mut doc: toml_edit::DocumentMut =
            "[mcp_servers.nergal]\ncommand = \"c\"\nargs = [\"mcp\"]\n\n[mcp_servers.other]\ncommand = \"x\"\n"
                .parse()
                .unwrap();
        assert!(apply_codex_registration(&mut doc, "/usr/bin/nergal", false));
        assert!(doc["mcp_servers"].get("nergal").is_none());
        assert_eq!(doc["mcp_servers"]["other"]["command"].as_str(), Some("x"));
    }

    #[test]
    fn codex_deregister_missing_is_noop() {
        let mut doc: toml_edit::DocumentMut = "[tui]\ntheme = \"m\"\n".parse().unwrap();
        assert!(!apply_codex_registration(
            &mut doc,
            "/usr/bin/nergal",
            false
        ));
    }

    // ── OpenCode (JSON) ──

    #[test]
    fn opencode_register_adds_local_entry() {
        let mut root = json!({ "$schema": "https://opencode.ai/config.json", "mcp": {} });
        assert!(apply_opencode_registration(
            &mut root,
            "/usr/bin/nergal",
            true
        ));
        assert_eq!(root["mcp"]["nergal"]["type"], "local");
        assert_eq!(root["mcp"]["nergal"]["command"][0], "/usr/bin/nergal");
        assert_eq!(root["mcp"]["nergal"]["command"][1], "mcp");
        assert_eq!(root["mcp"]["nergal"]["enabled"], true);
    }

    #[test]
    fn opencode_register_is_idempotent_and_preserves() {
        let mut root = json!({ "mcp": { "other": { "type": "local", "command": ["x"] } } });
        assert!(apply_opencode_registration(
            &mut root,
            "/usr/bin/nergal",
            true
        ));
        assert!(!apply_opencode_registration(
            &mut root,
            "/usr/bin/nergal",
            true
        ));
        assert_eq!(root["mcp"]["other"]["command"][0], "x");
    }

    #[test]
    fn opencode_deregister_removes_only_nergal() {
        let mut root =
            json!({ "mcp": { "nergal": { "type": "local" }, "other": { "type": "local" } } });
        assert!(apply_opencode_registration(
            &mut root,
            "/usr/bin/nergal",
            false
        ));
        assert!(root["mcp"].get("nergal").is_none());
        assert!(root["mcp"]["other"].is_object());
    }
}
