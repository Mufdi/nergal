//! Live preview browser backend.
//!
//! Two responsibilities:
//!   1. URL scheme validation for `browser_validate_url` (defense-in-depth so
//!      a compromised frontend cannot navigate the iframe to file:// or
//!      javascript: URLs).
//!   2. Localhost listening-port scanner: polls `platform_proc::listening_ports`
//!      every 3s to find TCP LISTEN ports in the user range, applies hysteresis
//!      (1-scan add, 2-scan remove) to absorb transient flap, emits
//!      `localhost:ports-changed` events.
//!
//! Cross-platform: the system-wide LISTEN list comes from `/proc/net/tcp` on
//! Linux (so root-owned docker-proxy ports show up, which `listeners` misses
//! since it only sees the current user's sockets) and from `listeners` on
//! macOS. Per-process owner attribution uses `listeners` on both.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use url::Url;

use crate::platform_spawn::NoWindow;

/// Reserved shortcuts intercepted at the Tauri runtime level so they fire
/// regardless of cross-origin iframe focus. Pairs (key chord, frontend
/// event payload). Adding entries here makes them registerable via
/// `browser_register_shortcuts`.
const RESERVED_SHORTCUTS: &[(&str, &str)] = &[
    ("ctrl+t", "browser:new-tab"),
    ("ctrl+w", "browser:close-tab"),
    ("ctrl+shift+0", "browser:toggle-mode"),
    ("ctrl+tab", "browser:next-tab"),
    ("ctrl+shift+tab", "browser:prev-tab"),
    ("f5", "browser:reload"),
    ("ctrl+r", "browser:reload"),
    // NOT ctrl+shift+r: the OS-global for that 3-key chord failed to unregister
    // cleanly (tauri global-shortcut quirk), so it stayed grabbed from other
    // apps (e.g. Brave's hard-reload) until Nergal quit. Hard-reload remains on
    // the browser toolbar button.
];

const ALLOWED_SCHEMES: &[&str] = &["http", "https"];
const ABOUT_BLANK: &str = "about:blank";

/// Inclusive lower bound: skip privileged system ports (DNS 53, CUPS 631…).
const MIN_PORT: u16 = 1024;
/// Inclusive upper bound: cap at the registered/user range so we exclude
/// the dynamic/ephemeral range (32768-60999 on Linux). Keeps the chip set
/// focused on dev servers.
const MAX_PORT: u16 = 32767;

const SCAN_INTERVAL: Duration = Duration::from_secs(3);
/// A port disappears from the active set only after this many consecutive
/// inactive scans. One-scan flap (e.g. dev-server hot-restart) is absorbed.
const REMOVE_AFTER_INACTIVE_SCANS: u8 = 2;

/// Validate a URL string against the allowed-scheme list.
///
/// Allows: http://, https://, about:blank.
/// Rejects: file://, javascript:, data:, chrome://, about:other, ftp://, etc.
pub fn validate_url(raw: &str) -> Result<Url, String> {
    if raw == ABOUT_BLANK {
        return Url::parse(ABOUT_BLANK).map_err(|e| format!("about:blank: {e}"));
    }
    let parsed = Url::parse(raw).map_err(|e| format!("invalid url: {e}"))?;
    if ALLOWED_SCHEMES.contains(&parsed.scheme()) {
        Ok(parsed)
    } else {
        Err(format!("scheme not allowed: {}", parsed.scheme()))
    }
}

#[tauri::command]
pub async fn browser_validate_url(url: String) -> Result<String, String> {
    let parsed = validate_url(&url)?;
    tracing::info!("browser navigate ok: {parsed}");
    Ok(parsed.to_string())
}

/// Synchronous snapshot of currently-listening user ports. Used by the
/// frontend on mount to cover the startup race where the scanner's first
/// emit may fire before the JS listener is registered (Tauri events do not
/// buffer). Bypasses hysteresis — returns raw current state. Subsequent
/// updates flow through `localhost:ports-changed` events.
#[tauri::command]
pub async fn browser_get_listening_ports() -> Vec<u16> {
    crate::platform_proc::listening_ports()
}

/// Register the reserved browser shortcuts at the OS level. Called when
/// the browser panel becomes visible. The handlers each emit a frontend
/// event the React side dispatches to the corresponding atom action.
///
/// We register globals (not webview-scoped) because Tauri 2's public API
/// does not expose pre-iframe key interception at the webview layer; OS
/// globals are the only way to bypass the cross-origin iframe focus trap
/// without breaking iframe form input typing.
#[tauri::command]
pub async fn browser_register_shortcuts(app: AppHandle) -> Result<(), String> {
    let manager = app.global_shortcut();
    for (chord, event_id) in RESERVED_SHORTCUTS {
        let shortcut: Shortcut = chord
            .parse()
            .map_err(|e| format!("invalid shortcut {chord}: {e}"))?;
        // Unregister first so a re-entrant register (focus regained while the
        // panel stayed open) replaces rather than stacks a stale handler — a
        // stacked registration is why a later unregister failed to release the
        // chord back to other apps (e.g. Ctrl+Shift+R staying grabbed).
        let _ = manager.unregister(shortcut);
        let app_clone = app.clone();
        let event_id = (*event_id).to_string();
        if let Err(e) = manager.on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed
                && let Err(emit_err) = app_clone.emit("browser:intercepted-shortcut", &event_id)
            {
                tracing::warn!("emit browser:intercepted-shortcut failed: {emit_err}");
            }
        }) {
            tracing::warn!("register {chord} failed: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_unregister_shortcuts(app: AppHandle) -> Result<(), String> {
    let manager = app.global_shortcut();
    for (chord, _) in RESERVED_SHORTCUTS {
        if let Ok(shortcut) = chord.parse::<Shortcut>()
            && let Err(e) = manager.unregister(shortcut)
        {
            tracing::warn!("unregister {chord} failed: {e}");
        }
    }
    Ok(())
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Drop a script extension so a cmdline arg reads as a tool name (`vite.js` →
/// `vite`).
fn strip_script_ext(s: &str) -> &str {
    for ext in [".js", ".mjs", ".cjs", ".ts"] {
        if let Some(stripped) = s.strip_suffix(ext) {
            return stripped;
        }
    }
    s
}

const INTERPRETERS: &[&str] = &[
    "node", "python", "python3", "python2", "ruby", "deno", "bun", "php", "sh", "bash",
];

/// Pure label resolution from a process's parts, so the heuristic is testable.
/// `exe_base` is the basename of the process executable path; `comm` is the
/// short kernel name. Returns `None` only when there is no arg0 to work from.
fn resolve_label(args: &[String], exe_base: Option<&str>, comm: Option<&str>) -> Option<String> {
    let arg0 = args.first()?;
    let base = basename(arg0);
    if INTERPRETERS.contains(&base) {
        // Skip flags and `-m`; the first plain arg is the script/module.
        for a in args.iter().skip(1) {
            if a.starts_with('-') {
                continue;
            }
            return Some(strip_script_ext(basename(a)).to_string());
        }
    }
    // Chromium/Electron apps (Discord, Slack, VSCode, Chrome) re-exec through
    // their sandbox helper with arg0 = `/proc/self/exe`, whose basename is the
    // literal "exe". Resolve the real binary via `/proc/<pid>/exe`, then `comm`.
    if base == "exe" {
        if let Some(e) = exe_base.filter(|e| !e.is_empty() && *e != "exe") {
            return Some(strip_script_ext(e).to_string());
        }
        if let Some(c) = comm.filter(|c| !c.is_empty()) {
            return Some(c.to_string());
        }
    }
    Some(strip_script_ext(base).to_string())
}

/// Best-effort Docker attribution for a published port (the listener on the
/// host is root-owned `docker-proxy`, so /proc can't see it). No-op when the
/// `docker` CLI is absent or the user lacks access.
fn docker_container_for_port(port: u16) -> Option<PortProcess> {
    let out = std::process::Command::new("docker")
        .no_window()
        .args(["ps", "--format", "{{.Names}}\t{{.Image}}\t{{.Ports}}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let needle = format!(":{port}->");
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        let [name, image, ports] = cols.as_slice() else {
            continue;
        };
        if ports.contains(&needle) {
            return Some(PortProcess {
                label: name.to_string(),
                project: Some((*image).to_string()),
                kind: "docker".into(),
                pid: None,
            });
        }
    }
    None
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortProcess {
    /// Tool/process name (`vite`) or container name.
    pub label: String,
    /// Project folder (process cwd) or docker image.
    pub project: Option<String>,
    /// "process" (owned, killable) | "docker" (external).
    pub kind: String,
    /// Owning PID — present only for `process` kind (used by kill_port).
    pub pid: Option<u32>,
}

/// Identify what's listening on `port`: an owned process (label from cmdline +
/// project from cwd basename) when a user-visible process can be attributed via
/// `platform_proc::port_owner`, else a best-effort Docker container, else None.
#[tauri::command]
pub fn port_process_info(port: u16) -> Option<PortProcess> {
    if let Some(owner) = crate::platform_proc::port_owner(port) {
        let label = resolve_label(
            &owner.args,
            owner.exe_base.as_deref(),
            owner.comm.as_deref(),
        )
        .unwrap_or_else(|| "unknown".into());
        return Some(PortProcess {
            label,
            project: owner.cwd_basename,
            kind: "process".into(),
            pid: Some(owner.pid),
        });
    }
    docker_container_for_port(port)
}

/// SIGTERM the process listening on `port` to free it. Resolves the owning PID
/// the same way `port_process_info` does, then lets the dev server shut down.
#[tauri::command]
pub fn kill_port(port: u16) -> Result<(), String> {
    // Owned process → terminate. A Docker-published port has no user-visible
    // owner (the listener is root `docker-proxy`), so fall back to stopping
    // the owning container by name.
    if let Some(owner) = crate::platform_proc::port_owner(port) {
        return crate::platform_proc::kill_pid(owner.pid)
            .map_err(|e| format!("kill({}) failed: {e}", owner.pid));
    }
    if let Some(dp) = docker_container_for_port(port) {
        let out = std::process::Command::new("docker")
            .no_window()
            .args(["stop", &dp.label])
            .output()
            .map_err(|e| format!("docker stop {}: {e}", dp.label))?;
        if !out.status.success() {
            return Err(format!(
                "docker stop {} failed: {}",
                dp.label,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        return Ok(());
    }
    Err("could not resolve the owning process or container".into())
}

/// Apply hysteresis to a raw scan result against the previous active set.
///
/// `inactive_streaks` tracks how many consecutive scans each previously-active
/// port has been missing for. A port:
///   - appears immediately on the first scan it's seen.
///   - disappears only after `REMOVE_AFTER_INACTIVE_SCANS` missed scans.
fn apply_hysteresis(
    raw_active: &[u16],
    previous_active: &[u16],
    inactive_streaks: &mut BTreeMap<u16, u8>,
) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();

    for port in raw_active {
        out.push(*port);
        inactive_streaks.remove(port);
    }

    for port in previous_active {
        if raw_active.contains(port) {
            continue;
        }
        let entry = inactive_streaks.entry(*port).or_insert(0);
        *entry += 1;
        if *entry < REMOVE_AFTER_INACTIVE_SCANS {
            out.push(*port);
        } else {
            inactive_streaks.remove(port);
        }
    }

    out.sort_unstable();
    out.dedup();
    out
}

/// Long-lived background task. Spawn once from the Tauri setup block.
pub async fn run_port_scanner(app: AppHandle) {
    let mut previous: Vec<u16> = Vec::new();
    let mut inactive_streaks: BTreeMap<u16, u8> = BTreeMap::new();
    let mut interval = tokio::time::interval(SCAN_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tracing::info!(
        "browser port scanner started: scanning every {:?} (range {MIN_PORT}-{MAX_PORT})",
        SCAN_INTERVAL,
    );

    loop {
        interval.tick().await;
        let raw = crate::platform_proc::listening_ports();
        tracing::debug!("browser scan tick: raw={raw:?}");
        let next = apply_hysteresis(&raw, &previous, &mut inactive_streaks);
        if next != previous {
            tracing::info!("browser ports changed: {previous:?} -> {next:?}");
            #[cfg(not(target_os = "linux"))]
            for line in crate::platform_proc::diagnose_listeners() {
                tracing::info!("port-diag: {line}");
            }
            if let Err(e) = app.emit("localhost:ports-changed", &next) {
                tracing::warn!("emit localhost:ports-changed failed: {e}");
            }
            previous = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https() {
        assert!(validate_url("http://localhost:5173").is_ok());
        assert!(validate_url("https://example.com/path?q=1").is_ok());
    }

    #[test]
    fn accepts_about_blank() {
        assert!(validate_url("about:blank").is_ok());
    }

    #[test]
    fn rejects_disallowed_schemes() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<h1>hi</h1>",
            "chrome://settings",
            "about:newtab",
            "ftp://example.com",
        ] {
            assert!(validate_url(url).is_err(), "expected {url} to be rejected");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(validate_url("").is_err());
        assert!(validate_url("not a url").is_err());
    }

    #[test]
    fn skips_privileged_ports_in_filter() {
        // Verify the port-range constants match platform_proc's filter window.
        let mut all = vec![22u16, 53, 80, 443, 631, 1024, 5173, 32767, 40000];
        all.retain(|&p| (MIN_PORT..=MAX_PORT).contains(&p));
        assert_eq!(all, vec![1024, 5173, 32767]);
    }

    #[test]
    fn hysteresis_adds_immediately() {
        let mut streaks = BTreeMap::new();
        let out = apply_hysteresis(&[3000, 5173], &[], &mut streaks);
        assert_eq!(out, vec![3000, 5173]);
        assert!(streaks.is_empty());
    }

    #[test]
    fn hysteresis_keeps_through_one_miss() {
        let mut streaks = BTreeMap::new();
        let out1 = apply_hysteresis(&[3000, 5173], &[], &mut streaks);
        assert_eq!(out1, vec![3000, 5173]);

        let out2 = apply_hysteresis(&[3000], &out1, &mut streaks);
        assert_eq!(out2, vec![3000, 5173]);
        assert_eq!(streaks.get(&5173), Some(&1));
    }

    #[test]
    fn hysteresis_removes_after_two_misses() {
        let mut streaks = BTreeMap::new();
        let out1 = apply_hysteresis(&[5173], &[], &mut streaks);
        let out2 = apply_hysteresis(&[], &out1, &mut streaks);
        assert_eq!(out2, vec![5173]);
        let out3 = apply_hysteresis(&[], &out2, &mut streaks);
        assert!(out3.is_empty());
        assert!(streaks.is_empty());
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn label_uses_arg0_basename_for_plain_binary() {
        let args = argv(&["/usr/bin/postgres", "-D", "/var/lib/pg"]);
        assert_eq!(
            resolve_label(&args, None, None).as_deref(),
            Some("postgres")
        );
    }

    #[test]
    fn label_resolves_interpreter_script() {
        let args = argv(&["/usr/bin/node", "/app/node_modules/.bin/vite.js"]);
        assert_eq!(resolve_label(&args, None, None).as_deref(), Some("vite"));
    }

    #[test]
    fn label_resolves_chromium_exe_via_readlink() {
        // Discord/Electron: arg0 = /proc/self/exe → basename "exe".
        let args = argv(&["/proc/self/exe", "--type=renderer"]);
        let exe_base = Some("Discord");
        assert_eq!(
            resolve_label(&args, exe_base, Some("Discord")).as_deref(),
            Some("Discord")
        );
    }

    #[test]
    fn label_falls_back_to_comm_when_exe_unresolvable() {
        // exe readlink failed (None) or itself reads "exe" — use comm.
        let args = argv(&["/proc/self/exe", "--type=gpu-process"]);
        assert_eq!(
            resolve_label(&args, None, Some("slack")).as_deref(),
            Some("slack")
        );
        assert_eq!(
            resolve_label(&args, Some("exe"), Some("slack")).as_deref(),
            Some("slack")
        );
    }

    #[test]
    fn label_none_without_arg0() {
        assert_eq!(resolve_label(&[], Some("x"), Some("y")), None);
    }

    #[test]
    fn hysteresis_resets_streak_on_recovery() {
        let mut streaks = BTreeMap::new();
        let out1 = apply_hysteresis(&[5173], &[], &mut streaks);
        let out2 = apply_hysteresis(&[], &out1, &mut streaks);
        assert_eq!(streaks.get(&5173), Some(&1));
        let out3 = apply_hysteresis(&[5173], &out2, &mut streaks);
        assert_eq!(out3, vec![5173]);
        assert!(streaks.is_empty());
    }
}
