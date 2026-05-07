//! Live preview browser backend.
//!
//! Two responsibilities:
//!   1. URL scheme validation for `browser_validate_url` (defense-in-depth so
//!      a compromised frontend cannot navigate the iframe to file:// or
//!      javascript: URLs).
//!   2. Localhost listening-port scanner: reads `/proc/net/tcp` and
//!      `/proc/net/tcp6` every 3s to find ports in LISTEN state on the user
//!      port range, applies hysteresis (1-scan add, 2-scan remove) to
//!      absorb transient flap, emits `localhost:ports-changed` events.
//!
//! Linux-only: cluihud targets Linux per CLAUDE.md. The /proc approach is
//! native, fast (sub-ms), and accurate — same source `ss`/`netstat`/`lsof`
//! consult. No hardcoded port list, no TCP probes.

use std::collections::BTreeMap;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use url::Url;

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
    ("ctrl+shift+r", "browser:hard-reload"),
];

const ALLOWED_SCHEMES: &[&str] = &["http", "https"];
const ABOUT_BLANK: &str = "about:blank";

/// Linux TCP socket state value for LISTEN. See `include/net/tcp_states.h`.
const TCP_LISTEN: &str = "0A";

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

const PROC_NET_TCP: &str = "/proc/net/tcp";
const PROC_NET_TCP6: &str = "/proc/net/tcp6";

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
    read_listening_user_ports()
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
        let app_clone = app.clone();
        let event_id = (*event_id).to_string();
        if let Err(e) = manager.on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed
                && let Err(emit_err) =
                    app_clone.emit("browser:intercepted-shortcut", &event_id)
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
        if let Ok(shortcut) = chord.parse::<Shortcut>() {
            let _ = manager.unregister(shortcut);
        }
    }
    Ok(())
}

/// Parse listening TCP ports from a `/proc/net/tcp{,6}` content blob.
///
/// Format (skip-1-header, then one socket per line):
///   sl  local_address rem_address   st ...
///    0: 0100007F:0277 00000000:0000 0A ...
///
/// `local_address` is `<hex_ip>:<hex_port>`. The port is the last 4 hex
/// digits regardless of IPv4 (8-hex IP) or IPv6 (32-hex IP). State `0A`
/// means LISTEN.
fn parse_listening_ports(content: &str) -> Vec<u16> {
    let mut ports = Vec::new();
    for line in content.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let local = match fields.nth(1) {
            Some(s) => s,
            None => continue,
        };
        // After the `nth(1)` consumed `sl` and `local_address`, `next()` is
        // `rem_address`; one more `next()` is `st`.
        let _rem = fields.next();
        let state = match fields.next() {
            Some(s) => s,
            None => continue,
        };
        if state != TCP_LISTEN {
            continue;
        }
        let port_hex = match local.rsplit(':').next() {
            Some(p) => p,
            None => continue,
        };
        let port = match u16::from_str_radix(port_hex, 16) {
            Ok(p) => p,
            Err(_) => continue,
        };
        ports.push(port);
    }
    ports
}

/// Read both /proc/net/tcp and tcp6, dedup, filter to user-port range,
/// return sorted ascending. Reads are best-effort: failure on either file
/// just yields an empty contribution (the other still works).
fn read_listening_user_ports() -> Vec<u16> {
    let mut all = Vec::new();
    if let Ok(c) = std::fs::read_to_string(PROC_NET_TCP) {
        all.extend(parse_listening_ports(&c));
    }
    if let Ok(c) = std::fs::read_to_string(PROC_NET_TCP6) {
        all.extend(parse_listening_ports(&c));
    }
    all.sort_unstable();
    all.dedup();
    all.retain(|&p| p >= MIN_PORT && p <= MAX_PORT);
    all
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
        "browser port scanner started: reading /proc/net/tcp{{,6}} every {:?} (range {}-{})",
        SCAN_INTERVAL,
        MIN_PORT,
        MAX_PORT
    );

    loop {
        interval.tick().await;
        let raw = read_listening_user_ports();
        tracing::debug!("browser scan tick: raw={raw:?}");
        let next = apply_hysteresis(&raw, &previous, &mut inactive_streaks);
        if next != previous {
            tracing::info!("browser ports changed: {previous:?} -> {next:?}");
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
    fn parses_ipv4_listen_socket() {
        // 127.0.0.1:5173 — port 5173 = 0x1435
        let content = "  sl  local_address rem_address   st\n   0: 0100007F:1435 00000000:0000 0A 00000000:00000000 00:00000000 00000000   1000        0 1\n";
        assert_eq!(parse_listening_ports(content), vec![5173]);
    }

    #[test]
    fn parses_ipv6_listen_socket() {
        // ::1:1420 — port 1420 = 0x058C, IPv6 IP is 32 hex chars
        let content = "  sl  local_address rem_address   st\n   0: 00000000000000000000000001000000:058C 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000   1000        0 1\n";
        assert_eq!(parse_listening_ports(content), vec![1420]);
    }

    #[test]
    fn ignores_non_listen_states() {
        // ESTABLISHED (01), TIME_WAIT (06), etc. are all != "0A"
        let content = "  sl  local_address rem_address   st\n   0: 0100007F:1F90 0100007F:1F91 01 00000000:00000000 00:00000000 00000000   1000        0 1\n";
        assert!(parse_listening_ports(content).is_empty());
    }

    #[test]
    fn skips_privileged_ports_in_filter() {
        // Constructed via read_listening_user_ports filter, simulated:
        let mut all = vec![22u16, 53, 80, 443, 631, 1024, 5173, 32767, 40000];
        all.retain(|&p| p >= MIN_PORT && p <= MAX_PORT);
        assert_eq!(all, vec![1024, 5173, 32767]);
    }

    #[test]
    fn handles_empty_proc_file() {
        assert!(parse_listening_ports("").is_empty());
        assert!(parse_listening_ports("  sl  local_address rem_address   st\n").is_empty());
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
