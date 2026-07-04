use tauri::State;

use crate::agents::AgentId;
use crate::agents::state::AgentRuntimeState;
use crate::config::Config;
use crate::db::SharedDb;

// -- Config commands --

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Ok(Config::load())
}

/// Fields the backend owns exclusively, written only via dedicated commands
/// (`summary_set_settings`, the ClickUp poller). The frontend `configAtom` is
/// hydrated from `get_config`, so it carries a **stale** copy of these — if
/// `save_config` applied them, a general settings save would roll the value
/// back (e.g. flip the AI-summary backend to `Off`). They are dropped from the
/// frontend payload so the on-disk value always wins.
const BACKEND_OWNED_CONFIG_KEYS: &[&str] = &[
    "summary",
    "cross_session",
    "agent_spawned_worktrees",
    "clickup_poll_interval_secs",
    "linear_poll_interval_secs",
    "linear_active_window_days",
];

/// Persist the settings the frontend manages. The frontend payload is merged
/// over the on-disk config at the JSON level, and [backend-owned
/// keys](BACKEND_OWNED_CONFIG_KEYS) in the payload are ignored so dedicated
/// commands remain the single source of truth for them.
#[tauri::command]
pub fn save_config(config: serde_json::Value) -> Result<(), String> {
    merge_config_over(Config::load(), &config)?
        .save()
        .map_err(|e| e.to_string())
}

/// Overlay a frontend config object onto `base`, skipping backend-owned keys so
/// a stale frontend copy of them can't clobber the authoritative on-disk value.
/// Pure (no I/O) so the invariant is unit-tested.
fn merge_config_over(base: Config, overlay: &serde_json::Value) -> Result<Config, String> {
    let mut merged = serde_json::to_value(base).map_err(|e| e.to_string())?;
    match (merged.as_object_mut(), overlay.as_object()) {
        (Some(base), Some(overlay)) => {
            for (k, v) in overlay {
                if BACKEND_OWNED_CONFIG_KEYS.contains(&k.as_str()) {
                    continue;
                }
                base.insert(k.clone(), v.clone());
            }
        }
        // Non-object payload is malformed; reject rather than overwrite.
        _ => return Err("save_config expects a config object".into()),
    }
    serde_json::from_value(merged).map_err(|e| e.to_string())
}

// -- Notification command --

#[tauri::command]
pub fn send_notification(app: tauri::AppHandle, title: String, body: String) {
    crate::notify::send(&app, &title, &body);
}

#[tauri::command]
pub fn drain_pending_deeplinks(
    state: tauri::State<'_, crate::PendingDeepLinks>,
) -> Result<Vec<String>, String> {
    let mut buf = state.0.lock().map_err(|e| e.to_string())?;
    Ok(std::mem::take(&mut *buf))
}

/// Wire string of the active adapter's context-injection tier for a session
/// (`append_system_prompt_file` | `prompt_preamble` | `unsupported`), so the
/// pin chip can phrase an honest tooltip per agent.
#[tauri::command]
pub fn get_context_injection_tier(
    agents: State<'_, AgentRuntimeState>,
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<String, String> {
    let agent_id = agents
        .resolve(&session_id)
        .or_else(|| {
            db.lock()
                .ok()
                .and_then(|g| g.find_session(&session_id).ok().flatten())
                .and_then(|s| AgentId::new(&s.agent_id).ok())
        })
        .unwrap_or_else(AgentId::claude_code);
    let tier = agents
        .registry
        .get(&agent_id)
        .map(|a| a.context_injection())
        .unwrap_or(crate::agents::ContextInjection::Unsupported);
    Ok(tier.as_wire().to_string())
}

#[cfg(test)]
mod config_merge_tests {
    use super::merge_config_over;
    use crate::commands::obsidian::is_allowed_scheme;
    use crate::config::{Config, SummaryBackend, SummaryConfig};

    #[test]
    fn frontend_save_ignores_stale_backend_owned_fields() {
        // On-disk has summaries enabled (set via summary_set_settings).
        let on_disk = Config {
            summary: SummaryConfig {
                backend: SummaryBackend::AgentCli,
                ..SummaryConfig::default()
            },
            clickup_poll_interval_secs: Some(30),
            ..Config::default()
        };
        // The frontend (hydrated from get_config at startup) carries a STALE
        // copy of the backend-owned fields plus a real change to a field it owns.
        let frontend = serde_json::json!({
            "theme_mode": "v1-light",
            "mcp_server_enabled": true,
            "summary": { "backend": "off", "agent_command": null,
                         "api_base_url": null, "api_model": null,
                         "disabled_projects": [] },
            "clickup_poll_interval_secs": null,
        });
        let merged = merge_config_over(on_disk, &frontend).unwrap();
        // Frontend-owned field applied…
        assert_eq!(merged.theme_mode, "v1-light");
        assert!(merged.mcp_server_enabled);
        // …stale backend-owned fields IGNORED — disk value wins.
        assert_eq!(merged.summary.backend, SummaryBackend::AgentCli);
        assert_eq!(merged.clickup_poll_interval_secs, Some(30));
    }

    #[test]
    fn non_object_payload_is_rejected() {
        assert!(merge_config_over(Config::default(), &serde_json::json!("nope")).is_err());
    }

    // Security boundary (Decision 7): the scheme allowlist is the sole guard
    // because Rust app.opener().open_url() bypasses the plugin ACL scope check.
    #[test]
    fn is_allowed_scheme_permits_obsidian_and_nergal() {
        assert!(is_allowed_scheme("obsidian://open?vault=MyVault"));
        assert!(is_allowed_scheme("nergal://session/abc123"));
    }

    #[test]
    fn is_allowed_scheme_rejects_other_schemes() {
        assert!(!is_allowed_scheme("https://evil.example.com"));
        assert!(!is_allowed_scheme("file:///etc/passwd"));
        assert!(!is_allowed_scheme("javascript:alert(1)"));
        assert!(!is_allowed_scheme(""));
    }
}
