use super::*;

fn full_map() -> Map<String, Value> {
    let mut settings = Map::new();
    apply_setup(&mut settings).unwrap();
    settings
}

fn for_each_hook_command(settings: &mut Map<String, Value>, mut f: impl FnMut(&mut Value)) {
    let hooks = settings
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .unwrap();
    for entries in hooks.values_mut() {
        for entry in entries.as_array_mut().unwrap() {
            for hook in entry["hooks"].as_array_mut().unwrap() {
                f(hook);
            }
        }
    }
}

#[test]
fn apply_setup_on_empty_map_reports_changes() {
    let mut settings = Map::new();
    assert!(!apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_is_idempotent_once_everything_is_present() {
    let mut settings = full_map();
    assert!(apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_detects_missing_session_start_entry() {
    let mut settings = full_map();
    settings
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .unwrap()
        .remove("SessionStart")
        .unwrap();
    assert!(!apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_detects_stale_tool_done_matcher() {
    let mut settings = full_map();
    let hooks = settings
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .unwrap();
    let mut touched = false;
    for entries in hooks.values_mut() {
        for entry in entries.as_array_mut().unwrap() {
            let is_tool_done = entry["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["command"] == "nergal hook send tool-done");
            if is_tool_done {
                entry["matcher"] = json!("Write|Edit");
                touched = true;
            }
        }
    }
    assert!(touched, "fixture must contain the tool-done entry");
    assert!(!apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_detects_obsolete_hook_entry() {
    let mut settings = full_map();
    let obs = &OBSOLETE_HOOKS[0];
    let hooks = settings
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .unwrap();
    let entries = hooks
        .entry(obs.event)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .unwrap();
    entries.push(json!({"hooks": [{"type": "command", "command": obs.command}]}));
    assert!(!apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_detects_missing_statusline() {
    let mut settings = full_map();
    settings.remove("statusLine").unwrap();
    assert!(!apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_keeps_custom_statusline_and_stays_empty() {
    let mut settings = full_map();
    settings.insert(
        "statusLine".into(),
        json!({"type": "command", "command": "my-own-statusline"}),
    );
    assert!(apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_treats_wrapper_form_commands_as_present() {
    let mut settings = full_map();
    for_each_hook_command(&mut settings, |hook| {
        let cmd = hook["command"].as_str().unwrap().to_string();
        if let Some(args) = cmd.strip_prefix("nergal hook ") {
            hook["command"] = json!(format!(
                "/home/u/.claude/hooks/nergal-conditional.sh {args}"
            ));
        }
    });
    assert!(apply_setup(&mut settings).unwrap().is_empty());
}

#[test]
fn apply_setup_errors_naming_event_when_hooks_entry_is_not_an_array() {
    let mut settings = Map::new();
    settings.insert("hooks".into(), json!({"SessionStart": {}}));
    let err = apply_setup(&mut settings).unwrap_err();
    assert!(err.to_string().contains("SessionStart"), "got: {err}");
}

#[test]
fn hooks_status_at_missing_file_reports_unregistered_without_creating_anything() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join("missing")
        .join(".claude")
        .join("settings.json");
    let status = hooks_status_at(&path).unwrap();
    assert!(!status.registered);
    assert_eq!(status.settings_path, path.to_string_lossy());
    assert!(!path.exists());
    assert!(!dir.path().join("missing").exists());
}

#[test]
fn hooks_status_at_invalid_json_errors_and_leaves_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, b"{not json").unwrap();
    assert!(hooks_status_at(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
}

#[test]
fn run_at_creates_absent_settings_and_second_run_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".claude").join("settings.json");
    run_at(&path).unwrap();
    assert!(path.exists());

    let mut parsed: Map<String, Value> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(apply_setup(&mut parsed).unwrap().is_empty());

    let first = std::fs::read(&path).unwrap();
    run_at(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), first);
}

#[test]
fn apply_setup_leaves_null_event_untouched_when_no_nergal_hook_targets_it() {
    let mut settings = Map::new();
    settings.insert("hooks".into(), json!({"MyEvent": null}));
    let changes = apply_setup(&mut settings).unwrap();
    assert!(!changes.is_empty());
    assert_eq!(settings["hooks"]["MyEvent"], Value::Null);
}

#[test]
fn run_at_errors_naming_event_and_leaves_file_and_backup_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let original = br#"{"hooks":{"SessionStart":{}}}"#;
    std::fs::write(&path, original).unwrap();

    let err = run_at(&path).unwrap_err();
    assert!(err.to_string().contains("SessionStart"), "got: {err}");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(!dir.path().join("settings.json.bak").exists());
}

#[test]
fn binary_usable_is_false_without_a_resolved_binary() {
    assert!(!binary_usable(None, None));
}

#[test]
fn binary_usable_accepts_installed_binary_under_an_appimage() {
    use std::path::Path;
    assert!(binary_usable(
        Some(Path::new("/usr/bin/nergal")),
        Some(Path::new("/tmp/.mount_x"))
    ));
}

#[test]
fn binary_usable_rejects_binary_inside_the_appimage_mount() {
    use std::path::Path;
    assert!(!binary_usable(
        Some(Path::new("/tmp/.mount_x/usr/bin/nergal")),
        Some(Path::new("/tmp/.mount_x"))
    ));
}

#[test]
fn binary_usable_accepts_binary_outside_any_appimage() {
    use std::path::Path;
    assert!(binary_usable(
        Some(Path::new("/home/u/.local/bin/nergal")),
        None
    ));
}

#[test]
fn apply_setup_adds_exit_plan_mode_to_existing_wrapper_post_tool_use_entry() {
    let mut settings = Map::new();
    settings.insert(
        "hooks".into(),
        json!({
            "PostToolUse": [{
                "hooks": [{
                    "type": "command",
                    "command": "/home/felipe/.claude/hooks/nergal-conditional.sh send tool-done",
                    "async": true,
                }],
                "matcher": "Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create|AskUserQuestion"
            }]
        }),
    );
    apply_setup(&mut settings).unwrap();
    let entries: Vec<&Value> = settings["hooks"]["PostToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["command"].as_str().unwrap().contains("send tool-done"))
        })
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "matcher upgrade must not add a second entry"
    );
    let matcher = entries[0]["matcher"].as_str().unwrap();
    assert!(matcher.split('|').any(|m| m == "ExitPlanMode"));
}
