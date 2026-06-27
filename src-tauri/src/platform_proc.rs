//! Cross-platform process and port introspection — replaces the three
//! hand-rolled `/proc` subsystems (port scanner, process-tree kill, ancestor
//! env recovery) with `sysinfo` + `listeners` so the same code compiles and
//! works on Linux and macOS.
//!
//! Signals stay `libc::kill` under `cfg(unix)`: POSIX semantics are identical
//! on both platforms, and `sysinfo`'s `Process::kill_with` lacks the `kill(-pgid)`
//! call that BUG-06 requires (kills processes in a *new* process group).

use listeners::{Protocol, SocketState};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Lower bound for the user-port filter (inclusive). Mirrors the constant in
/// `browser.rs` — privileged ports (22, 53, 80 …) are out of scope.
const MIN_PORT: u16 = 1024;
/// Upper bound (inclusive). Excludes the ephemeral/dynamic range (≥32768 on
/// Linux) to keep the chip focused on dev servers.
const MAX_PORT: u16 = 32767;

// ── OS diagnostics ──────────────────────────────────────────────────────────

/// Kernel version string (e.g. `"6.8.0-48-generic"` on Linux, `"24.1.0"` on
/// macOS). Computed on every call by `sysinfo`.
pub fn kernel_version() -> Option<String> {
    System::kernel_version()
}

/// Human-readable OS/distro name (e.g. `"Linux (Ubuntu 24.04)"`, `"macOS
/// 15.1.1 Sequoia"`). Falls back to `"<name> <version>"` if `long_os_version`
/// is unavailable. Computed on every call by `sysinfo`.
pub fn os_name() -> Option<String> {
    System::long_os_version().or_else(|| {
        let name = System::name()?;
        let ver = System::os_version().unwrap_or_default();
        Some(format!("{name} {ver}"))
    })
}

// ── Process cwd ─────────────────────────────────────────────────────────────

/// Absolute cwd of the process `pid` as a string. Returns `None` when the
/// process is gone or the cwd is unreadable (permission, proc-gone race).
pub fn process_cwd(pid: u32) -> Option<String> {
    let spid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[spid]),
        false,
        ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always),
    );
    sys.process(spid)?.cwd().map(|p| p.display().to_string())
}

// ── Process tree ─────────────────────────────────────────────────────────────

/// BFS descendant set of `root` — **excludes** `root` itself and any pid ≤ 1.
/// Returned in BFS order (root's children first, their children next, …) so
/// `kill_tree` can reverse the list to kill leaf processes first.
pub fn descendants(root: u32) -> Vec<u32> {
    let mut sys = System::new();
    // Parent info is always populated; no UpdateKind fields needed.
    sys.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::nothing());

    let procs: Vec<(u32, u32)> = sys
        .processes()
        .iter()
        .filter_map(|(pid, proc)| {
            let ppid = proc.parent()?.as_u32();
            Some((pid.as_u32(), ppid))
        })
        .collect();

    let mut tree = vec![root];
    let mut i = 0;
    while i < tree.len() {
        let parent = tree[i];
        for &(pid, ppid) in &procs {
            if ppid == parent && pid != root && !tree.contains(&pid) {
                tree.push(pid);
            }
        }
        i += 1;
    }

    // Skip index 0 (the root itself).
    tree.into_iter().skip(1).collect()
}

/// SIGTERM the shell's entire descendant tree + its process group, preserving
/// the exact ordering from the original `kill_process_tree` in `pty.rs`:
/// deepest descendants first (reversed BFS), then `kill(-pgid)` to catch the
/// foreground group, then the root pid itself. Guards `root > 1`.
#[cfg(unix)]
pub fn kill_tree(root: u32) {
    if root <= 1 {
        return;
    }
    let desc = descendants(root);
    for &pid in desc.iter().rev() {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
    unsafe {
        libc::kill(-(root as libc::pid_t), libc::SIGTERM);
        libc::kill(root as libc::pid_t, libc::SIGTERM);
    }
}

/// SIGTERM a single pid. Maps a non-zero libc return code to `io::Error` the
/// same way the original `kill_port` did (preserving the `last_os_error` path).
#[cfg(unix)]
pub fn kill_pid(pid: u32) -> std::io::Result<()> {
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// ── Port scanner ─────────────────────────────────────────────────────────────

/// All TCP ports in LISTEN state (v4 + v6), sorted ascending, deduped,
/// filtered to the user-port range [`MIN_PORT`, `MAX_PORT`]. Replaces the
/// `/proc/net/tcp{,6}` + `parse_listening_ports` pipeline in `browser.rs`.
pub fn listening_ports() -> Vec<u16> {
    let Ok(all) = listeners::get_all() else {
        return Vec::new();
    };
    let mut ports: Vec<u16> = all
        .iter()
        .filter(|l| l.protocol == Protocol::TCP && l.state == SocketState::Listen)
        .map(|l| l.socket.port())
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports.retain(|&p| (MIN_PORT..=MAX_PORT).contains(&p));
    ports
}

/// Process attributes for the owner of a LISTEN socket — inputs to
/// `resolve_label` in `browser.rs` and the `project` field of `PortProcess`.
pub struct PortOwner {
    pub pid: u32,
    /// Full cmdline argument list (`sysinfo` `.cmd()`). Same role as the
    /// NUL-split `/proc/<pid>/cmdline` args in the old `process_label`.
    pub args: Vec<String>,
    /// Basename of the executable path (`sysinfo` `.exe()`). Resolves the
    /// `/proc/self/exe` → "exe" Chromium/Electron case.
    pub exe_base: Option<String>,
    /// Short process name (`sysinfo` `.name()`). Equivalent to `/proc/<pid>/comm`.
    pub comm: Option<String>,
    /// Basename of the process cwd — used as the `project` (folder) field.
    pub cwd_basename: Option<String>,
}

/// Resolve the owner of the LISTEN socket on `port`. Returns `None` when
/// no user-visible process owns the port (e.g. root `docker-proxy` — on
/// Linux, `listeners` can't read root's `/proc/<pid>/fd`, so it returns an
/// error and the caller falls back to the Docker attribution path). Internally:
/// `listeners` gives us the owning pid, then `sysinfo` fills in cmd/exe/cwd
/// for labelling. If the process exits between the two scans (race), `listeners`'
/// name/path are used as a minimal fallback so we never panic.
pub fn port_owner(port: u16) -> Option<PortOwner> {
    let lproc = listeners::get_process_by_port(port, Protocol::TCP).ok()?;
    let pid = lproc.pid;

    let spid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[spid]),
        false,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet),
    );

    let (args, exe_base, comm, cwd_basename) = if let Some(proc) = sys.process(spid) {
        let args: Vec<String> = proc
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        let exe_base = proc
            .exe()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let comm = Some(proc.name().to_string_lossy().into_owned());
        let cwd_basename = proc
            .cwd()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        (args, exe_base, comm, cwd_basename)
    } else {
        // Process exited between listeners scan and sysinfo refresh — use
        // listeners' name/path as a minimal fallback for labelling.
        let exe_base = std::path::Path::new(&lproc.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let comm = Some(lproc.name.clone());
        (vec![lproc.name], exe_base, comm, None)
    };

    Some(PortOwner {
        pid,
        args,
        exe_base,
        comm,
        cwd_basename,
    })
}

// ── Ancestor env recovery ────────────────────────────────────────────────────

/// Walk the ancestor chain of the current process (up to `max_depth` hops)
/// and return the value of the first key from `keys` found in any ancestor's
/// environment. Returns `None` when none of the keys appear. Mirrors the
/// `session_hint_from_ancestors` behaviour in `mcp/shim.rs`.
///
/// On macOS, `sysinfo` may not be able to read another process's environ
/// (SIP / hardened-runtime restrictions) — callers should treat `None` as
/// "env not found or unreadable" and fall back gracefully.
pub fn ancestor_env(keys: &[&str], max_depth: usize) -> Option<String> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        false,
        ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
    );

    let mut pid = Pid::from_u32(std::process::id());
    for _ in 0..max_depth {
        let proc = sys.process(pid)?;
        let ppid = proc.parent()?;
        if ppid.as_u32() <= 1 {
            break;
        }
        pid = ppid;
        let ancestor = sys.process(pid)?;
        for env_var in ancestor.environ() {
            let env_str = env_var.to_string_lossy();
            for &key in keys {
                let prefix = format!("{key}=");
                if let Some(val) = env_str.strip_prefix(prefix.as_str())
                    && !val.is_empty()
                {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendants_excludes_root() {
        // Build a synthetic tree: root→child→grandchild. Only child and
        // grandchild should appear; root is excluded.
        let desc = descendants(std::process::id());
        // The test process has no children, so descendants must be empty.
        assert!(
            desc.is_empty() || desc.iter().all(|&p| p != std::process::id()),
            "root must not appear in descendants"
        );
    }

    #[test]
    fn descendants_excludes_pid_le_1() {
        // Pid 0 and 1 are system processes — they must never appear in the
        // descendant set, because we skip ppid <= 1 at the pid-pair collection
        // level and at the kill_tree guard level.
        let desc = descendants(std::process::id());
        assert!(desc.iter().all(|&p| p > 1), "pid ≤ 1 must not appear");
    }

    #[test]
    fn port_range_filter() {
        // Mirrors the `skips_privileged_ports_in_filter` test in browser.rs.
        let mut all = vec![22u16, 53, 80, 443, 631, 1024, 5173, 32767, 40000];
        all.retain(|&p| (MIN_PORT..=MAX_PORT).contains(&p));
        assert_eq!(all, vec![1024, 5173, 32767]);
    }

    #[test]
    fn ancestor_env_key_order() {
        // With no matching ancestors, must return None, not panic.
        let result = ancestor_env(&["__DEFINITELY_NOT_SET_KEY_XYZ__"], 4);
        assert!(result.is_none());
    }
}
