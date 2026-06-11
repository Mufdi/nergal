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
            "Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create|AskUserQuestion",
        ),
        command: "cluihud hook send tool-done",
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
        event: "Notification",
        matcher: Some("permission_prompt"),
        command: "cluihud hook notification",
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
    // PostToolUse without matcher replaced by filtered version
    ObsoleteHook {
        event: "PostToolUse",
        command: "cluihud hook send tool-done",
        only_without_matcher: true,
    },
    // CC never shipped these hook events; CC v2.1.138+ rejects the keys as invalid.
    ObsoleteHook {
        event: "CwdChanged",
        command: "cluihud hook send cwd-changed",
        only_without_matcher: false,
    },
    ObsoleteHook {
        event: "FileChanged",
        command: "cluihud hook send file-changed",
        only_without_matcher: false,
    },
    ObsoleteHook {
        event: "PermissionDenied",
        command: "cluihud hook send permission-denied",
        only_without_matcher: false,
    },
    ObsoleteHook {
        event: "TaskCompleted",
        command: "cluihud hook send task-done",
        only_without_matcher: false,
    },
    ObsoleteHook {
        event: "TaskCreated",
        command: "cluihud hook send task-created",
        only_without_matcher: false,
    },
    // Send-gate Running edge, removed with the guard (2026-06-11): CC queues
    // mid-turn prompts natively, and the hook invaded the user's global
    // settings while only working for Claude Code.
    ObsoleteHook {
        event: "UserPromptSubmit",
        command: "cluihud hook send user-prompt",
        only_without_matcher: false,
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

    let wrapper_prefix = detect_wrapper_prefix(hooks_map);

    for def in HOOKS {
        if merge_hook(hooks_map, def, wrapper_prefix.as_deref()) {
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

/// Match a configured hook command against a target `cluihud hook …` command,
/// accepting both the bare binary form and the user-installed
/// `cluihud-conditional.sh` wrapper form (see `docs/hooks.md`). Without this,
/// settings.json entries that route through the wrapper are invisible to
/// `cluihud hook setup` cleanup and merge logic.
fn matches_hook_command(cmd: &str, target: &str) -> bool {
    if cmd == target {
        return true;
    }
    if let Some(args) = target.strip_prefix("cluihud hook ")
        && cmd.contains(WRAPPER_MARKER)
    {
        return cmd.ends_with(args);
    }
    false
}

const WRAPPER_MARKER: &str = "/cluihud-conditional.sh ";

/// Wrapper prefix (full script path + trailing space) of one configured
/// command, if it routes through `cluihud-conditional.sh`.
fn wrapper_prefix_of(cmd: &str) -> Option<String> {
    let pos = cmd.find(WRAPPER_MARKER)?;
    Some(cmd[..pos + WRAPPER_MARKER.len()].to_string())
}

/// Scan every configured hook command for the `cluihud-conditional.sh`
/// wrapper. Deterministic rule for new entries: any existing wrapper-form
/// cluihud entry ⇒ install new entries wrapped (a bare entry on a wrapper
/// machine would spawn-and-fail on every prompt of every CC session outside
/// cluihud); none ⇒ install bare.
fn detect_wrapper_prefix(hooks_map: &Map<String, Value>) -> Option<String> {
    for entries in hooks_map.values() {
        let Some(arr) = entries.as_array() else {
            continue;
        };
        for entry in arr {
            let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) else {
                continue;
            };
            for h in hooks {
                if let Some(prefix) = h
                    .get("command")
                    .and_then(|c| c.as_str())
                    .and_then(wrapper_prefix_of)
                {
                    return Some(prefix);
                }
            }
        }
    }
    None
}

/// Synthesize the on-disk command for a new entry: wrapper form when the
/// settings already route cluihud hooks through the wrapper, bare otherwise.
fn synthesize_command(def_command: &str, wrapper_prefix: Option<&str>) -> String {
    match (wrapper_prefix, def_command.strip_prefix("cluihud hook ")) {
        (Some(prefix), Some(args)) => format!("{prefix}{args}"),
        _ => def_command.to_string(),
    }
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
                .is_some_and(|c| matches_hook_command(c, obs.command))
        })
    });

    let removed = arr.len() < before;

    if arr.is_empty() {
        hooks_map.remove(obs.event);
    }

    removed
}

/// Merge a single hook into the hooks map. Returns true if the map was modified
/// (added new entry OR upgraded matcher of existing entry), false if already
/// correct. Matcher upgrade keeps the same command/wrapper choice intact —
/// only the matcher pattern changes. New entries synthesize the wrapper form
/// when `wrapper_prefix` is set (see [`detect_wrapper_prefix`]).
fn merge_hook(
    hooks_map: &mut Map<String, Value>,
    def: &HookDef,
    wrapper_prefix: Option<&str>,
) -> bool {
    let entries = hooks_map
        .entry(def.event)
        .or_insert_with(|| Value::Array(Vec::new()));

    let Value::Array(arr) = entries else {
        return false;
    };

    if let Some(idx) = arr.iter().position(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|hooks| {
                hooks.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| matches_hook_command(c, def.command))
                })
            })
    }) {
        let entry_matcher = arr[idx].get("matcher").and_then(|m| m.as_str());
        if entry_matcher == def.matcher {
            return false;
        }
        let Some(obj) = arr[idx].as_object_mut() else {
            return false;
        };
        match def.matcher {
            Some(m) => {
                obj.insert("matcher".into(), Value::String(m.into()));
            }
            None => {
                obj.remove("matcher");
            }
        }
        return true;
    }

    let mut hook = json!({
        "type": "command",
        "command": synthesize_command(def.command, wrapper_prefix),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_accepts_bare_form() {
        assert!(matches_hook_command(
            "cluihud hook send tool-done",
            "cluihud hook send tool-done",
        ));
    }

    #[test]
    fn matcher_accepts_wrapper_form() {
        assert!(matches_hook_command(
            "/home/felipe/.claude/hooks/cluihud-conditional.sh send tool-done",
            "cluihud hook send tool-done",
        ));
    }

    #[test]
    fn matcher_rejects_unrelated_command() {
        assert!(!matches_hook_command(
            "echo send tool-done",
            "cluihud hook send tool-done",
        ));
    }

    #[test]
    fn matcher_rejects_wrong_args_via_wrapper() {
        assert!(!matches_hook_command(
            "/home/felipe/.claude/hooks/cluihud-conditional.sh send cwd-changed",
            "cluihud hook send tool-done",
        ));
    }

    #[test]
    fn matcher_rejects_lookalike_script_name() {
        assert!(!matches_hook_command(
            "/usr/bin/not-cluihud-conditional.sh send tool-done",
            "cluihud hook send tool-done",
        ));
    }

    #[test]
    fn remove_hook_clears_wrapper_entry_and_drops_empty_event() {
        let mut hooks_map = Map::new();
        hooks_map.insert(
            "CwdChanged".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "/home/felipe/.claude/hooks/cluihud-conditional.sh send cwd-changed",
                    "async": true,
                }]
            }]),
        );

        let obs = ObsoleteHook {
            event: "CwdChanged",
            command: "cluihud hook send cwd-changed",
            only_without_matcher: false,
        };

        assert!(remove_hook(&mut hooks_map, &obs));
        assert!(!hooks_map.contains_key("CwdChanged"));
    }

    #[test]
    fn merge_hook_upgrades_matcher_in_place_for_wrapper_entry() {
        let mut hooks_map = Map::new();
        hooks_map.insert(
            "PostToolUse".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "/home/felipe/.claude/hooks/cluihud-conditional.sh send tool-done",
                    "async": true,
                }],
                "matcher": "Write|Edit"
            }]),
        );

        let def = HookDef {
            event: "PostToolUse",
            matcher: Some("Write|Edit|Bash|AskUserQuestion"),
            command: "cluihud hook send tool-done",
            is_async: true,
            timeout: None,
            if_condition: None,
        };

        assert!(merge_hook(&mut hooks_map, &def, None));
        let arr = hooks_map["PostToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1, "matcher upgrade must not duplicate");
        assert_eq!(
            arr[0].get("matcher").and_then(|m| m.as_str()),
            Some("Write|Edit|Bash|AskUserQuestion"),
            "matcher should be upgraded"
        );
        let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            cmd.contains("/cluihud-conditional.sh "),
            "wrapper command must be preserved through matcher upgrade"
        );
    }

    #[test]
    fn merge_hook_treats_wrapper_form_as_present() {
        let mut hooks_map = Map::new();
        hooks_map.insert(
            "SessionStart".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "/home/felipe/.claude/hooks/cluihud-conditional.sh send session-start",
                    "async": true,
                }]
            }]),
        );

        let def = HookDef {
            event: "SessionStart",
            matcher: None,
            command: "cluihud hook send session-start",
            is_async: true,
            timeout: None,
            if_condition: None,
        };

        assert!(!merge_hook(&mut hooks_map, &def, None));
        let arr = hooks_map["SessionStart"].as_array().unwrap();
        assert_eq!(
            arr.len(),
            1,
            "should not duplicate when wrapper form already present"
        );
    }

    // Generic async fixture for merge/wrapper-synthesis tests; the command is
    // hypothetical on purpose — the logic under test is command-agnostic.
    const ASYNC_TEST_COMMAND: &str = "cluihud hook send user-prompt";

    fn send_user_prompt_def() -> HookDef {
        HookDef {
            event: "UserPromptSubmit",
            matcher: None,
            command: ASYNC_TEST_COMMAND,
            is_async: true,
            timeout: None,
            if_condition: None,
        }
    }

    #[test]
    fn insertion_synthesizes_wrapper_form_when_existing_entries_are_wrapped() {
        let mut hooks_map = Map::new();
        hooks_map.insert(
            "Stop".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "/home/felipe/.claude/hooks/cluihud-conditional.sh send stop",
                    "async": true,
                }]
            }]),
        );

        let prefix = detect_wrapper_prefix(&hooks_map);
        assert_eq!(
            prefix.as_deref(),
            Some("/home/felipe/.claude/hooks/cluihud-conditional.sh ")
        );

        let def = send_user_prompt_def();
        assert!(merge_hook(&mut hooks_map, &def, prefix.as_deref()));
        let arr = hooks_map["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(
            arr[0]["hooks"][0]["command"].as_str().unwrap(),
            "/home/felipe/.claude/hooks/cluihud-conditional.sh send user-prompt",
        );
        assert_eq!(arr[0]["hooks"][0]["async"], Value::Bool(true));
    }

    #[test]
    fn insertion_synthesizes_wrapper_form_with_mixed_existing_forms() {
        // One bare + one wrapper entry: ANY wrapper entry wins (a bare entry
        // on a wrapper machine would spawn-and-fail outside cluihud).
        let mut hooks_map = Map::new();
        hooks_map.insert(
            "SessionStart".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "cluihud hook send session-start",
                    "async": true,
                }]
            }]),
        );
        hooks_map.insert(
            "Stop".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "/home/felipe/.claude/hooks/cluihud-conditional.sh send stop",
                    "async": true,
                }]
            }]),
        );

        let prefix = detect_wrapper_prefix(&hooks_map);
        let def = send_user_prompt_def();
        assert!(merge_hook(&mut hooks_map, &def, prefix.as_deref()));
        assert_eq!(
            hooks_map["UserPromptSubmit"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap(),
            "/home/felipe/.claude/hooks/cluihud-conditional.sh send user-prompt",
        );
    }

    #[test]
    fn insertion_stays_bare_with_no_prior_entries() {
        let mut hooks_map = Map::new();
        let prefix = detect_wrapper_prefix(&hooks_map);
        assert!(prefix.is_none());

        let def = send_user_prompt_def();
        assert!(merge_hook(&mut hooks_map, &def, prefix.as_deref()));
        assert_eq!(
            hooks_map["UserPromptSubmit"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap(),
            ASYNC_TEST_COMMAND,
        );
    }

    #[test]
    fn insertion_is_idempotent_across_reruns_in_both_forms() {
        // Wrapper machine: second run must find the wrapped entry and skip.
        let mut wrapped = Map::new();
        wrapped.insert(
            "Stop".into(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": "/home/x/.claude/hooks/cluihud-conditional.sh send stop",
                    "async": true,
                }]
            }]),
        );
        let def = send_user_prompt_def();
        let prefix = detect_wrapper_prefix(&wrapped);
        assert!(merge_hook(&mut wrapped, &def, prefix.as_deref()));
        let prefix = detect_wrapper_prefix(&wrapped);
        assert!(!merge_hook(&mut wrapped, &def, prefix.as_deref()));
        assert_eq!(wrapped["UserPromptSubmit"].as_array().unwrap().len(), 1);

        // Bare machine: same invariant.
        let mut bare = Map::new();
        assert!(merge_hook(&mut bare, &def, None));
        assert!(!merge_hook(&mut bare, &def, None));
        assert_eq!(bare["UserPromptSubmit"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn send_entry_coexists_with_inject_edits_under_same_event() {
        let mut hooks_map = Map::new();
        let inject = HookDef {
            event: "UserPromptSubmit",
            matcher: None,
            command: "cluihud hook inject-edits",
            is_async: false,
            timeout: None,
            if_condition: None,
        };
        assert!(merge_hook(&mut hooks_map, &inject, None));
        assert!(merge_hook(&mut hooks_map, &send_user_prompt_def(), None));
        let arr = hooks_map["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(arr.len(), 2, "second entry must push, not replace");
        // Re-run touches neither.
        assert!(!merge_hook(&mut hooks_map, &inject, None));
        assert!(!merge_hook(&mut hooks_map, &send_user_prompt_def(), None));
        assert_eq!(hooks_map["UserPromptSubmit"].as_array().unwrap().len(), 2);
    }
}
