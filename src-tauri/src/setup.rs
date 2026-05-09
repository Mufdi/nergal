use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

/// Hook definitions for Claude Code integration.
const HOOKS: &[HookDef] = &[
    HookDef {
        event: "SessionStart",
        matcher: None,
        command: "cluihud hook send session-start",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "SessionEnd",
        matcher: None,
        command: "cluihud hook send session-end",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "PermissionRequest",
        matcher: Some("ExitPlanMode"),
        command: "cluihud hook plan-review",
        is_async: false,
        timeout: Some(86400),
        if_condition: None,
    },
    HookDef {
        event: "PreToolUse",
        matcher: None,
        command: "cluihud hook send pre-tool",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "PostToolUse",
        matcher: Some(
            "Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create",
        ),
        command: "cluihud hook send tool-done",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "TaskCompleted",
        matcher: None,
        command: "cluihud hook send task-done",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "TaskCreated",
        matcher: None,
        command: "cluihud hook send task-created",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "PreToolUse",
        matcher: Some("AskUserQuestion"),
        command: "cluihud hook ask-user",
        is_async: false,
        timeout: Some(86400),
        if_condition: None,
    },
    HookDef {
        event: "CwdChanged",
        matcher: None,
        command: "cluihud hook send cwd-changed",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "FileChanged",
        matcher: None,
        command: "cluihud hook send file-changed",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "PermissionDenied",
        matcher: None,
        command: "cluihud hook send permission-denied",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "Stop",
        matcher: None,
        command: "cluihud hook send stop",
        is_async: true,
        timeout: None,
        if_condition: None,
    },
    HookDef {
        event: "UserPromptSubmit",
        matcher: None,
        command: "cluihud hook inject-edits",
        is_async: false,
        timeout: None,
        if_condition: None,
    },
];

struct HookDef {
    event: &'static str,
    matcher: Option<&'static str>,
    command: &'static str,
    is_async: bool,
    timeout: Option<u64>,
    if_condition: Option<&'static str>,
}

/// Hooks that should be removed during setup (superseded by new definitions).
const OBSOLETE_HOOKS: &[ObsoleteHook] = &[
    ObsoleteHook {
        event: "PreToolUse",
        command: "cluihud hook send plan-ready",
        only_without_matcher: false,
    },
    ObsoleteHook {
        event: "UserPromptSubmit",
        command: "cluihud hook inject-edits",
        only_without_matcher: false,
    },
    // PostToolUse without matcher replaced by filtered version
    ObsoleteHook {
        event: "PostToolUse",
        command: "cluihud hook send tool-done",
        only_without_matcher: true,
    },
];

struct ObsoleteHook {
    event: &'static str,
    command: &'static str,
    /// Only remove if the entry has no `matcher` field.
    only_without_matcher: bool,
}

/// Run the setup command: configure Claude Code hooks in ~/.claude/settings.json.
pub fn run() -> Result<()> {
    let settings_path = settings_path()?;

    let mut settings = load_settings(&settings_path)?;
    let hooks_obj = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));

    let Value::Object(hooks_map) = hooks_obj else {
        anyhow::bail!("~/.claude/settings.json: 'hooks' is not an object");
    };

    let mut removed = Vec::new();
    for obs in OBSOLETE_HOOKS {
        if remove_hook(hooks_map, obs) {
            removed.push(obs.command);
        }
    }

    let mut added = Vec::new();
    let mut skipped = Vec::new();

    for def in HOOKS {
        if merge_hook(hooks_map, def) {
            added.push(def.event);
        } else {
            skipped.push(def.event);
        }
    }

    if added.is_empty() && removed.is_empty() {
        println!("All cluihud hooks already configured. Nothing to do.");
        return Ok(());
    }

    backup_settings(&settings_path)?;
    save_settings(&settings_path, &settings)?;

    println!("Configured cluihud hooks in {}", settings_path.display());
    println!();
    for cmd in &removed {
        println!("  - {cmd} (obsolete, removed)");
    }
    for event in &added {
        println!("  + {event}");
    }
    for event in &skipped {
        println!("  ~ {event} (already present, kept existing)");
    }
    println!();
    println!("Backup saved as {}.bak", settings_path.display());

    Ok(())
}

/// Remove an obsolete hook entry. Returns true if removed.
fn remove_hook(hooks_map: &mut Map<String, Value>, obs: &ObsoleteHook) -> bool {
    let Some(Value::Array(arr)) = hooks_map.get_mut(obs.event) else {
        return false;
    };

    let before = arr.len();
    arr.retain(|entry| {
        if obs.only_without_matcher && entry.get("matcher").is_some() {
            return true;
        }
        let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) else {
            return true;
        };
        !hooks.iter().any(|h| {
            h.get("command")
                .and_then(|c| c.as_str())
                .is_some_and(|c| c == obs.command)
        })
    });

    let removed = arr.len() < before;

    if arr.is_empty() {
        hooks_map.remove(obs.event);
    }

    removed
}

/// Merge a single hook into the hooks map. Returns true if added, false if already present.
fn merge_hook(hooks_map: &mut Map<String, Value>, def: &HookDef) -> bool {
    let entries = hooks_map
        .entry(def.event)
        .or_insert_with(|| Value::Array(Vec::new()));

    let Value::Array(arr) = entries else {
        return false;
    };

    let already_exists = arr.iter().any(|entry| {
        // Match on both matcher and command to allow multiple hooks per event
        let entry_matcher = entry.get("matcher").and_then(|m| m.as_str());
        if entry_matcher != def.matcher {
            return false;
        }
        let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) else {
            return false;
        };
        hooks.iter().any(|h| {
            h.get("command")
                .and_then(|c| c.as_str())
                .is_some_and(|c| c == def.command)
        })
    });

    if already_exists {
        return false;
    }

    let mut hook = json!({
        "type": "command",
        "command": def.command,
    });
    if def.is_async {
        hook.as_object_mut()
            .expect("just created")
            .insert("async".into(), Value::Bool(true));
    }
    if let Some(timeout) = def.timeout {
        hook.as_object_mut()
            .expect("just created")
            .insert("timeout".into(), Value::Number(timeout.into()));
    }
    if let Some(condition) = def.if_condition {
        hook.as_object_mut()
            .expect("just created")
            .insert("if".into(), Value::String(condition.into()));
    }

    let mut entry = json!({
        "hooks": [hook],
    });
    if let Some(matcher) = def.matcher {
        entry
            .as_object_mut()
            .expect("just created")
            .insert("matcher".into(), Value::String(matcher.into()));
    }
    arr.push(entry);
    true
}

fn settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".claude").join("settings.json"))
}

fn load_settings(path: &PathBuf) -> Result<Map<String, Value>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let val: Value =
                serde_json::from_str(&contents).context("invalid JSON in settings.json")?;
            let Value::Object(map) = val else {
                anyhow::bail!("settings.json root is not an object");
            };
            Ok(map)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Ok(Map::new())
        }
        Err(e) => Err(e).context("reading settings.json"),
    }
}

fn backup_settings(path: &PathBuf) -> Result<()> {
    if path.exists() {
        let backup = path.with_extension("json.bak");
        std::fs::copy(path, &backup).context("creating settings backup")?;
    }
    Ok(())
}

fn save_settings(path: &PathBuf, settings: &Map<String, Value>) -> Result<()> {
    let json = serde_json::to_string_pretty(&Value::Object(settings.clone()))?;
    std::fs::write(path, json).context("writing settings.json")?;
    Ok(())
}
