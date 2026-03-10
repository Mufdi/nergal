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
    },
    HookDef {
        event: "SessionEnd",
        matcher: None,
        command: "cluihud hook send session-end",
        is_async: true,
    },
    HookDef {
        event: "PreToolUse",
        matcher: Some("ExitPlanMode"),
        command: "cluihud hook send plan-ready",
        is_async: true,
    },
    HookDef {
        event: "PreToolUse",
        matcher: None,
        command: "cluihud hook send pre-tool",
        is_async: true,
    },
    HookDef {
        event: "PostToolUse",
        matcher: None,
        command: "cluihud hook send tool-done",
        is_async: true,
    },
    HookDef {
        event: "TaskCompleted",
        matcher: None,
        command: "cluihud hook send task-done",
        is_async: true,
    },
    HookDef {
        event: "Stop",
        matcher: None,
        command: "cluihud hook send stop",
        is_async: true,
    },
    HookDef {
        event: "UserPromptSubmit",
        matcher: None,
        command: "cluihud hook inject-edits",
        is_async: false,
    },
];

struct HookDef {
    event: &'static str,
    matcher: Option<&'static str>,
    command: &'static str,
    is_async: bool,
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

    let mut added = Vec::new();
    let mut skipped = Vec::new();

    for def in HOOKS {
        if merge_hook(hooks_map, def) {
            added.push(def.event);
        } else {
            skipped.push(def.event);
        }
    }

    if added.is_empty() {
        println!("All cluihud hooks already configured. Nothing to do.");
        return Ok(());
    }

    backup_settings(&settings_path)?;
    save_settings(&settings_path, &settings)?;

    println!("Configured cluihud hooks in {}", settings_path.display());
    println!();
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
