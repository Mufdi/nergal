//! Idempotent registration of the `cluihud mcp` shim into agent MCP configs.
//!
//! Design Decision 7: the registered command pins the **installed absolute
//! path** `/usr/bin/cluihud` (not a `$PATH` lookup, which would bake in the
//! `~/.cargo/bin/cluihud` shadow CLAUDE.md documents). Registration is
//! idempotent (no duplicate entries); deregistration is best-effort at disable
//! time. An orphaned entry after uninstall degrades to a structured error when
//! the agent tries to spawn the missing binary — not a hard failure.
//!
//! v1 wires Claude Code (`~/.claude.json` `mcpServers`). Codex/Pi/OpenCode use
//! different config schemas; their registrars land once each format is verified
//! against a live install (tracked in the change), so we don't write guessed
//! config that could corrupt an agent's setup.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// The MCP server key cluihud owns inside an agent config.
const SERVER_KEY: &str = "cluihud";

/// Absolute command the agent spawns for the shim. Pins `/usr/bin/cluihud`
/// (the `.deb`/`.rpm` location) when present; falls back to the running
/// executable so a dev build still registers a working path.
fn shim_command() -> String {
    let installed = PathBuf::from("/usr/bin/cluihud");
    if installed.exists() {
        return installed.to_string_lossy().into_owned();
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "cluihud".to_string())
}

fn cc_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude.json"))
}

/// Mutate a parsed agent-config root in place: add or remove the `cluihud`
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
        std::fs::write(&path, pretty).with_context(|| format!("writing {}", path.display()))?;
        tracing::info!(
            "mcp: {} cluihud in {}",
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

/// Register the shim into every supported agent config (best-effort per agent).
pub fn register() -> Result<()> {
    rewrite_cc_config(true)
}

/// Remove the shim from every supported agent config (best-effort).
pub fn deregister() -> Result<()> {
    rewrite_cc_config(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_adds_entry() {
        let mut root = json!({});
        assert!(apply_cc_registration(&mut root, "/usr/bin/cluihud", true));
        assert_eq!(root["mcpServers"]["cluihud"]["command"], "/usr/bin/cluihud");
        assert_eq!(root["mcpServers"]["cluihud"]["args"][0], "mcp");
    }

    #[test]
    fn register_is_idempotent() {
        let mut root = json!({});
        apply_cc_registration(&mut root, "/usr/bin/cluihud", true);
        // Second identical registration reports no change.
        assert!(!apply_cc_registration(&mut root, "/usr/bin/cluihud", true));
    }

    #[test]
    fn register_preserves_other_servers() {
        let mut root = json!({ "mcpServers": { "other": { "command": "x" } }, "foo": 1 });
        apply_cc_registration(&mut root, "/usr/bin/cluihud", true);
        assert_eq!(root["mcpServers"]["other"]["command"], "x");
        assert_eq!(root["foo"], 1);
        assert!(root["mcpServers"]["cluihud"].is_object());
    }

    #[test]
    fn deregister_removes_only_cluihud() {
        let mut root =
            json!({ "mcpServers": { "cluihud": { "command": "c" }, "other": { "command": "x" } } });
        assert!(apply_cc_registration(&mut root, "/usr/bin/cluihud", false));
        assert!(root["mcpServers"].get("cluihud").is_none());
        assert_eq!(root["mcpServers"]["other"]["command"], "x");
    }

    #[test]
    fn deregister_missing_is_noop() {
        let mut root = json!({ "mcpServers": { "other": {} } });
        assert!(!apply_cc_registration(&mut root, "/usr/bin/cluihud", false));
    }

    #[test]
    fn register_into_non_object_root_resets() {
        let mut root = json!("garbage");
        assert!(apply_cc_registration(&mut root, "/usr/bin/cluihud", true));
        assert!(root["mcpServers"]["cluihud"].is_object());
    }
}
