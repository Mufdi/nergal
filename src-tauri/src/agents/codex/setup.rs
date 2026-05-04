//! Conservative merge of cluihud's hook entries into `~/.codex/hooks.json`.
//!
//! Mirrors the CC `cluihud setup` flow but targets Codex's hook config file.
//! Behavior:
//! 1. Read existing `~/.codex/hooks.json` (or start blank).
//! 2. For each event Codex supports, ensure cluihud's `command` entry
//!    exists in the array, identified by a marker substring (`cluihud hook`).
//! 3. Drop any obsolete cluihud entries (matchers we no longer use) so old
//!    installs don't accumulate stale rows.
//! 4. Preserve every non-cluihud entry verbatim so user customisations
//!    remain untouched.
//!
//! Atomic write: write to a temp file in the same directory, then rename.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{Value, json};

const CLUIHUD_MARKER: &str = "cluihud hook";

/// Run the setup. Idempotent.
pub async fn run_codex_setup() -> Result<()> {
    let home = dirs::home_dir().context("home dir")?;
    let path = home.join(".codex/hooks.json");
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    let existing = match tokio::fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str::<Value>(&s).unwrap_or(json!({})),
        Err(_) => json!({}),
    };

    let merged = merge_cluihud_entries(existing);
    let body = serde_json::to_string_pretty(&merged)?;
    write_atomic(&path, &body).await?;
    Ok(())
}

fn merge_cluihud_entries(existing: Value) -> Value {
    // Normalise to an object before mutating so we don't double-borrow.
    let mut root = match existing {
        Value::Object(map) => Value::Object(map),
        _ => json!({}),
    };
    let hooks_obj = root.as_object_mut().expect("normalised to object above");

    let hooks_inner = hooks_obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap();

    for (event_name, cluihud_command) in cluihud_event_commands() {
        let event_array = hooks_inner
            .entry(event_name.to_string())
            .or_insert_with(|| Value::Array(vec![]));
        let arr = match event_array.as_array_mut() {
            Some(a) => a,
            None => {
                *event_array = Value::Array(vec![]);
                event_array.as_array_mut().unwrap()
            }
        };

        // 1) Drop obsolete cluihud entries (any cluihud-marked matcher that
        //    isn't the current shape). For now the rule is simple: keep the
        //    canonical entry shape, drop any other cluihud entry.
        arr.retain(|entry| {
            !is_cluihud_entry(entry) || matches_canonical_shape(entry, cluihud_command)
        });

        // 2) Insert canonical entry if missing.
        let already_present = arr
            .iter()
            .any(|entry| matches_canonical_shape(entry, cluihud_command));
        if !already_present {
            arr.push(canonical_entry(cluihud_command));
        }
    }

    root
}

fn cluihud_event_commands() -> Vec<(&'static str, &'static str)> {
    // Codex's hook event names line up with CC's. Map each to the
    // corresponding `cluihud hook ...` invocation.
    vec![
        ("SessionStart", "cluihud hook send session-start"),
        ("SessionEnd", "cluihud hook send session-end"),
        ("PreToolUse", "cluihud hook send pre-tool"),
        ("PostToolUse", "cluihud hook send post-tool"),
        ("Stop", "cluihud hook send stop"),
        ("UserPromptSubmit", "cluihud hook inject-edits"),
    ]
}

fn canonical_entry(command: &str) -> Value {
    json!({
        "matcher": "*",
        "hooks": [{
            "type": "command",
            "command": command,
            "async": true
        }]
    })
}

fn is_cluihud_entry(entry: &Value) -> bool {
    let hooks = match entry.get("hooks").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return false,
    };
    hooks.iter().any(|h| {
        h.get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|c| c.contains(CLUIHUD_MARKER))
    })
}

fn matches_canonical_shape(entry: &Value, expected_command: &str) -> bool {
    let hooks = match entry.get("hooks").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return false,
    };
    hooks.iter().any(|h| {
        h.get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|c| c == expected_command)
    })
}

async fn write_atomic(path: &PathBuf, body: &str) -> Result<()> {
    let dir = path.parent().context("hooks.json has no parent")?;
    let tmp = dir.join(format!("hooks.json.tmp-{}", std::process::id()));
    tokio::fs::write(&tmp, body).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_into_empty_object_creates_canonical_entries() {
        let merged = merge_cluihud_entries(json!({}));
        let entries = merged
            .pointer("/hooks/PreToolUse")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches_canonical_shape(
            &entries[0],
            "cluihud hook send pre-tool"
        ));
    }

    #[test]
    fn merge_preserves_user_entries() {
        let user_entry = json!({
            "matcher": "Bash",
            "hooks": [{ "type": "command", "command": "echo bash" }]
        });
        let existing = json!({ "hooks": { "PreToolUse": [user_entry.clone()] } });
        let merged = merge_cluihud_entries(existing);
        let entries = merged
            .pointer("/hooks/PreToolUse")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(entries.len(), 2, "user entry preserved + cluihud added");
        assert_eq!(&entries[0], &user_entry, "user entry first, untouched");
    }

    #[test]
    fn merge_is_idempotent() {
        let once = merge_cluihud_entries(json!({}));
        let twice = merge_cluihud_entries(once.clone());
        assert_eq!(once, twice);
    }

    #[test]
    fn merge_drops_obsolete_cluihud_entries() {
        // An old shape that no longer matches the canonical list should be
        // pruned on next setup run.
        let stale = json!({
            "matcher": "*",
            "hooks": [{ "type": "command", "command": "cluihud hook send obsolete-event" }]
        });
        let existing = json!({ "hooks": { "PreToolUse": [stale] } });
        let merged = merge_cluihud_entries(existing);
        let entries = merged
            .pointer("/hooks/PreToolUse")
            .and_then(|v| v.as_array())
            .unwrap();
        // Only the canonical "pre-tool" entry remains.
        assert_eq!(entries.len(), 1);
        assert!(matches_canonical_shape(
            &entries[0],
            "cluihud hook send pre-tool"
        ));
    }
}
