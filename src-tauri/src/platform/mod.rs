//! Per-user IPC runtime directory resolver and transport seam.
//!
//! WHY this module exists: Linux `/tmp` is world-writable (sticky-bit). A
//! foreign-uid process can (1) pre-create socket/FIFO paths under predictable
//! names (squat DoS), (2) connect in the create→`chmod 0600` window (TOCTOU),
//! or (3) write a forged `allow` into the plan-review FIFO (forged approval
//! gate). Moving every endpoint into a per-user `0700` directory whose ROOT is
//! un-writable by other uids closes all three vectors at once.
//!
//! macOS `temp_dir()` is already a per-user `0700` `/var/folders/…/T` whose
//! parent is user-owned, so no different-uid process can mkdir inside it. The
//! per-user resolver therefore harmonises both platforms on the stronger model
//! rather than just patching the Linux gap.
//!
//! # Security invariants (reviewer checklist)
//!
//! 1. `ipc_dir()` is keyed off `getuid()`, never an env var. The three
//!    processes that must agree on the path (GUI bind, MCP shim connect,
//!    hook-CLI connect) do NOT share an env (Codex sanitizes the shim env).
//! 2. On Linux the root is `/run/user/<uid>` — only systemd can mkdir there
//!    (not another user process). On macOS `temp_dir()` is already per-user.
//! 3. If the resolved dir is foreign-owned or over-permissive the server
//!    refuses to bind and logs (tripwire). This branch is unreachable by a
//!    different uid given the un-squattable root, but it guards misconfigured
//!    hosts and development mistakes.
//! 4. Stale endpoint removal is conditional on a liveness probe
//!    (`ECONNREFUSED` = provably dead, not "no reply" which fire-and-forget
//!    hook sockets never send).
//! 5. Peer uid is extracted via `SO_PEERCRED` (Linux) / `LOCAL_PEERCRED`
//!    (macOS) — never a peer PID (macOS exposes none).
//! 6. Rejection logging is rate-limited to prevent a hostile spammer from
//!    turning the audit log into a disk-fill DoS.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dashmap::DashMap;

// Windows named-pipe security helpers (client-SID peer auth, owner-only SD,
// owner verification). Win32-only; the Unix path uses peer-cred uids.
#[cfg(windows)]
mod win_sec;

// ── Platform constants ────────────────────────────────────────────────────────

/// Maximum AF_UNIX sun_path length on this platform (characters, incl. null).
/// macOS is tighter than Linux (104 vs 108 bytes total for the struct field).
#[cfg(target_os = "macos")]
const SUN_PATH_MAX: usize = 103; // 104 - 1 for null
#[cfg(not(target_os = "macos"))]
const SUN_PATH_MAX: usize = 107; // 108 - 1 for null

// ── IPC directory resolver ────────────────────────────────────────────────────

/// Resolve the per-user IPC runtime directory, creating it (`0700`) if absent.
///
/// On Linux: `/run/user/<getuid()>/nergal/` (systemd-managed root, un-squattable).
/// If that is missing or fails validation: `<getpwuid(uid)->pw_dir>/.local/share/nergal/ipc/`.
/// NEVER falls back to a guessable directory under shared `/tmp`.
///
/// On macOS: `temp_dir()/nergal/` (already per-user `0700`, parent user-owned).
///
/// WHY `getuid()` not `$XDG_RUNTIME_DIR`: Codex sanitizes the MCP shim env;
/// an env-keyed root diverges between GUI (bind) and shim (connect), silently
/// breaking MCP. `getuid()` is stable across all process-env configurations.
#[cfg(unix)]
pub fn ipc_dir() -> io::Result<PathBuf> {
    let uid = unsafe { libc::getuid() };

    // Linux primary: /run/user/<uid>/nergal/
    #[cfg(target_os = "linux")]
    {
        let run_user = PathBuf::from(format!("/run/user/{uid}"));
        if run_user.exists() {
            match validate_base_dir_owned_by(&run_user, uid) {
                Ok(()) => {
                    let dir = run_user.join("nergal");
                    return ensure_ipc_dir_0700(&dir, uid).map(|()| dir);
                }
                Err(e) => {
                    tracing::warn!(
                        ipc_event = "base_dir_validation_failed",
                        path = %run_user.display(),
                        error = %e,
                        "IPC base dir validation failed; falling back to home"
                    );
                }
            }
        }
        // Fallback: home from getpwuid (NOT $HOME — sudo/su leaves it stale)
        let home = home_from_getpwuid(uid)?;
        let dir = home.join(".local").join("share").join("nergal").join("ipc");
        // Validate that the home dir is owned by uid before using it
        validate_base_dir_owned_by(&home, uid)?;
        ensure_ipc_dir_0700(&dir, uid).map(|()| dir)
    }

    // macOS: temp_dir() is per-user 0700 /var/folders/…/T — safe as root
    #[cfg(target_os = "macos")]
    {
        let dir = std::env::temp_dir().join("nergal");
        return ensure_ipc_dir_0700(&dir, uid).map(|()| dir);
    }

    // Other unix (FreeBSD etc.) — home fallback
    #[cfg(all(unix, not(target_os = "linux"), not(target_os = "macos")))]
    {
        let home = home_from_getpwuid(uid)?;
        let dir = home.join(".local").join("share").join("nergal").join("ipc");
        validate_base_dir_owned_by(&home, uid)?;
        return ensure_ipc_dir_0700(&dir, uid).map(|()| dir);
    }
}

/// Validate that `dir` is owned by `uid`. Used to guard the base directory
/// before placing the per-user IPC dir inside it.
// macOS resolves via temp_dir() (inherently per-user) and never hits the
// getpwuid home fallback, so this base-dir guard is Linux/other-unix only.
#[cfg(all(unix, not(target_os = "macos")))]
fn validate_base_dir_owned_by(dir: &Path, uid: u32) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(dir)?;
    if meta.uid() != uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "IPC base dir {} is owned by uid {} not {}",
                dir.display(),
                meta.uid(),
                uid
            ),
        ));
    }
    Ok(())
}

/// Create `dir` with mode `0700` if absent; if it exists, validate it is owned
/// by `uid` and no broader than `0700`. A foreign-owned or over-permissive
/// directory triggers a refusal + log (tripwire for misconfigured hosts).
#[cfg(unix)]
fn ensure_ipc_dir_0700(dir: &Path, uid: u32) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    if dir.exists() {
        let meta = std::fs::metadata(dir)?;
        if meta.uid() != uid {
            tracing::warn!(
                ipc_event = "dir_validation_refused",
                path = %dir.display(),
                owned_by = meta.uid(),
                expected = uid,
                "IPC dir is foreign-owned; refusing to bind (tripwire)"
            );
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "IPC dir {} is owned by uid {}, expected {}",
                    dir.display(),
                    meta.uid(),
                    uid
                ),
            ));
        }
        let mode = meta.mode() & 0o777;
        if mode & 0o077 != 0 {
            tracing::warn!(
                ipc_event = "dir_validation_refused",
                path = %dir.display(),
                mode = format!("{mode:03o}"),
                "IPC dir is over-permissive; refusing to bind (tripwire)"
            );
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "IPC dir {} has mode {:03o} (expected ≤ 0700)",
                    dir.display(),
                    mode
                ),
            ));
        }
        return Ok(());
    }

    // Not found — create with 0700.
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| {
            tracing::warn!(
                ipc_event = "bind_failure",
                path = %dir.display(),
                error = %e,
                "Failed to create IPC dir"
            );
            e
        })
}

/// Retrieve the home directory for `uid` via `getpwuid_r` (NOT `$HOME` —
/// under `sudo`/`su` `$HOME` can point at the original user's directory).
// Used only by the Linux/other-unix home fallback; macOS uses temp_dir().
#[cfg(all(unix, not(target_os = "macos")))]
fn home_from_getpwuid(uid: u32) -> io::Result<PathBuf> {
    let mut buf = vec![0u8; 8192];
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pw,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };

    if ret != 0 || result.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("getpwuid_r failed for uid {uid} (ret={ret})"),
        ));
    }

    let home_ptr = unsafe { (*result).pw_dir };
    if home_ptr.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("getpwuid_r returned null pw_dir for uid {uid}"),
        ));
    }

    let home_cstr = unsafe { std::ffi::CStr::from_ptr(home_ptr) };
    let home_str = home_cstr.to_str().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("home dir for uid {uid} is not UTF-8"),
        )
    })?;

    Ok(PathBuf::from(home_str))
}

// ── Endpoint path helpers ─────────────────────────────────────────────────────

/// Derive an endpoint path inside `ipc_dir`, enforcing the `AF_UNIX` `sun_path`
/// length limit at derivation time. If the full path would overflow, a short
/// stable hash of `name` is used as the filename instead, so callers never
/// encounter a `bind` failure from path length alone.
///
/// WHY at derivation time not bind time: `sockaddr_un.sun_path` is a fixed
/// C array; a silent truncation at `bind` would produce a wrong path, not an
/// error. Checking here gives a clear error with the full path in the message.
fn endpoint_path_within(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    let candidate_str = candidate.to_str().unwrap_or_default();
    if candidate_str.len() <= SUN_PATH_MAX {
        return candidate;
    }
    // Path exceeds sun_path — use a short FNV-1a hash of the name as fallback.
    // The hash is stable across runs (same name → same hash) and collision-safe
    // for our small endpoint set (hook.sock, mcp.sock, plan-*.fifo).
    let hash = fnv1a_32(name.as_bytes());
    dir.join(format!("{hash:08x}.sock"))
}

fn fnv1a_32(data: &[u8]) -> u32 {
    let mut h: u32 = 2166136261;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Build a per-user named-pipe path `\\.\pipe\nergal-{user-SID}-{endpoint}`.
/// The SID makes the name unique per user, so two users on one machine never
/// collide in the machine-global `\\.\pipe` namespace (Decision 3). The SID
/// string contains only `S`, `-`, and ASCII digits — safe in a pipe name.
#[cfg(windows)]
fn win_pipe_name(endpoint: &str) -> io::Result<PathBuf> {
    let sid = win_sec::current_user_sid_string()?;
    Ok(PathBuf::from(format!(r"\\.\pipe\nergal-{sid}-{endpoint}")))
}

/// Hook event endpoint: a socket in the per-user IPC dir (Unix) or a per-user
/// named pipe (Windows).
pub fn hook_socket_path() -> io::Result<PathBuf> {
    #[cfg(unix)]
    {
        let dir = ipc_dir()?;
        Ok(endpoint_path_within(&dir, "hook.sock"))
    }
    #[cfg(windows)]
    {
        win_pipe_name("hook")
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(std::env::temp_dir().join("nergal.sock"))
    }
}

/// MCP daemon endpoint: a socket in the per-user IPC dir (Unix) or a per-user
/// named pipe (Windows).
pub fn mcp_socket_path() -> io::Result<PathBuf> {
    #[cfg(unix)]
    {
        let dir = ipc_dir()?;
        Ok(endpoint_path_within(&dir, "mcp.sock"))
    }
    #[cfg(windows)]
    {
        win_pipe_name("mcp")
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(std::env::temp_dir().join("nergal-mcp.sock"))
    }
}

/// Plan-review endpoint for the given process id: a FIFO in the per-user IPC
/// dir (Unix) or a per-user named pipe (Windows).
pub fn plan_review_fifo_path(pid: u32) -> io::Result<PathBuf> {
    #[cfg(unix)]
    {
        let dir = ipc_dir()?;
        // Not a socket, so sun_path limit does not apply; keep it readable.
        Ok(dir.join(format!("plan-{pid}.fifo")))
    }
    #[cfg(windows)]
    {
        win_pipe_name(&format!("plan-{pid}"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(std::env::temp_dir().join(format!("nergal-plan-{pid}.fifo")))
    }
}

/// Directory holding the `gui.pid` liveness token. Unix: the per-user IPC dir.
/// Windows: the per-user local-data dir `…/nergal` (default per-user ACL) —
/// `ipc_dir()` has no Windows analog, but the liveness token still anchors the
/// plan-review dead-GUI deny (Decision 6). Both the writer (GUI) and reader
/// (hook CLI) resolve through here so they agree on one location.
pub fn gui_pid_dir() -> io::Result<PathBuf> {
    #[cfg(unix)]
    {
        ipc_dir()
    }
    #[cfg(windows)]
    {
        let base = dirs::data_local_dir().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no local-data dir for gui.pid")
        })?;
        let dir = base.join("nergal");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(std::env::temp_dir())
    }
}

// ── IPC lockfile (concurrent same-uid instance guard) ────────────────────────

/// Exclusive `flock` on `<ipc_dir>/nergal.lock`. Held by the live GUI
/// instance for the lifetime of the process so that a second same-uid launch
/// can detect a live peer and defer rather than unlinking its socket.
///
/// WHY `flock` not advisory: `fs2::FileExt::try_lock_exclusive` is `flock(2)`
/// on Unix — kernel-held, released on process death, no stale-lock hazard.
pub struct IpcLockFile(std::fs::File, #[allow(dead_code)] PathBuf);

impl IpcLockFile {
    /// Try to acquire the lock. Returns `Err` with `WouldBlock` if another
    /// instance holds it; returns `Err` with other kinds for I/O failures.
    pub fn acquire(dir: &Path) -> io::Result<Self> {
        #[allow(unused_imports)]
        use fs2::FileExt;
        let path = dir.join("nergal.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)?;
        file.try_lock_exclusive().map_err(|e| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("IPC lock held by another instance: {e}"),
            )
        })?;
        Ok(Self(file, path))
    }
}

impl Drop for IpcLockFile {
    fn drop(&mut self) {
        #[allow(unused_imports)]
        use fs2::FileExt;
        let _ = self.0.unlock();
    }
}

// ── GUI liveness token (gui.pid) ─────────────────────────────────────────────

/// Write the GUI liveness token: `<pid>\n<start_secs_since_epoch>\n`.
///
/// The start time disambiguates a recycled pid (if the kernel reuses this pid
/// for a different process, its start time will differ). Written only by the
/// instance that holds the IPC flock so the CLI always reads the live server.
pub fn write_gui_pid(dir: &Path) -> io::Result<()> {
    let pid = std::process::id();
    let start = process_start_time(pid).unwrap_or(0);
    let token = format!("{pid}\n{start}\n");
    let path = dir.join("gui.pid");
    std::fs::write(&path, token.as_bytes())
}

/// Read the GUI liveness token. Returns `(pid, start_time)` or `None`.
pub fn read_gui_pid(dir: &Path) -> Option<(u32, u64)> {
    let content = std::fs::read_to_string(dir.join("gui.pid")).ok()?;
    let mut lines = content.lines();
    let pid: u32 = lines.next()?.parse().ok()?;
    let start: u64 = lines.next()?.parse().ok()?;
    Some((pid, start))
}

/// Check whether the GUI identified by `(pid, expected_start)` is still alive.
///
/// Returns `true` only if the process exists AND its start time matches the
/// recorded token (guarding against pid recycling). Returns `false` if the
/// process is gone (`ESRCH`) or the start time mismatches.
pub fn check_gui_liveness(pid: u32, expected_start: u64) -> bool {
    // Safety-check: a start time of 0 means we could not measure it;
    // fall back to a basic kill(0) existence check (better than hanging).
    let actual_start = process_start_time(pid);
    match actual_start {
        None => {
            // Process not found
            tracing::info!(
                ipc_event = "dead_peer_deny",
                gui_pid = pid,
                "GUI pid not found — resolving to safe deny"
            );
            false
        }
        Some(0) => {
            // Measured as 0 — can't disambiguate recycled pid; allow wait to continue
            // only if kill(pid, 0) succeeds (ESRCH = dead)
            #[cfg(unix)]
            {
                let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
                if !alive {
                    tracing::info!(
                        ipc_event = "dead_peer_deny",
                        gui_pid = pid,
                        "GUI pid not running — resolving to safe deny"
                    );
                }
                alive
            }
            #[cfg(not(unix))]
            false
        }
        Some(start) => {
            if start != expected_start {
                tracing::info!(
                    ipc_event = "dead_peer_deny",
                    gui_pid = pid,
                    expected_start,
                    actual_start = start,
                    "GUI pid start-time mismatch (pid recycled) — resolving to safe deny"
                );
                false
            } else {
                true
            }
        }
    }
}

/// Return the process start time in seconds since epoch, or `None` if the
/// process does not exist.
///
/// Uses `sysinfo` so the same code path works on both Linux and macOS.
/// `start_time()` is a core field always populated by sysinfo on process
/// refresh (no special `UpdateKind` flag needed, unlike cwd/exe).
fn process_start_time(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let spid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[spid]),
        false,
        // nothing() suffices: start_time is always populated when the process
        // is found regardless of which UpdateKind flags are set.
        ProcessRefreshKind::nothing(),
    );
    sys.process(spid).map(|p| p.start_time())
}

// ── Stale endpoint probe (concurrent-instance guard) ─────────────────────────

/// Check whether an existing socket file at `path` has a live listener.
///
/// WHY connect-probe not "no reply": the hook socket is fire-and-forget and
/// never sends a response, so "connected but no reply" means the socket IS
/// live, not dead. `ECONNREFUSED` on an existing socket file is the only
/// signal that the path is provably dead (kernel cleans the socket on exit
/// only if the peer called `close()`; a crash leaves the file but refuses
/// new connections).
#[cfg(unix)]
pub fn probe_socket_alive(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    // Attempt a non-blocking connect; success = live, ECONNREFUSED = dead.
    use std::os::unix::net::UnixStream;
    match UnixStream::connect(path) {
        Ok(_) => true,
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => false,
        // PermissionDenied (another uid's socket), etc. → assume live (defer)
        Err(_) => true,
    }
}

// ── Peer uid (for hook server accept loop) ───────────────────────────────────

/// Extract the peer process uid from a connected `tokio::net::UnixStream`.
///
/// On Linux: `SO_PEERCRED`. On macOS: `LOCAL_PEERCRED` / `getpeereid`.
/// Abstracted by `tokio`'s `peer_cred()` so no platform-specific code here.
/// WHY never peer PID: macOS `LOCAL_PEERCRED` does not expose a peer PID.
#[cfg(unix)]
pub fn peer_uid_of(stream: &tokio::net::UnixStream) -> io::Result<u32> {
    let cred = stream.peer_cred()?;
    Ok(cred.uid())
}

// ── Peer identity (cross-platform principal) ─────────────────────────────────

/// Opaque peer identity from an accepted IPC connection. The seam returns this
/// instead of a raw uid so the Unix path (a uid via `SO_PEERCRED`) and the
/// Windows named-pipe path (a variable-length client SID) share one
/// comparison + audit surface. The only enforced access control is
/// "same principal as this process?" — there is no notion of a privileged
/// peer.
#[derive(Clone)]
pub enum PeerIdentity {
    #[cfg(unix)]
    Uid(u32),
    #[cfg(windows)]
    Sid(Box<[u8]>),
}

impl PeerIdentity {
    /// True when the peer is the same principal as the current process. Unix:
    /// `uid == getuid()`. Windows: the client SID `EqualSid` the process owner
    /// SID. A `false` here is the rejection signal at every accept loop.
    pub fn matches_current_process(&self) -> bool {
        match self {
            #[cfg(unix)]
            PeerIdentity::Uid(uid) => *uid == unsafe { libc::getuid() },
            #[cfg(windows)]
            PeerIdentity::Sid(sid) => win_sec::sid_matches_current_user(sid),
        }
    }

    /// A loggable principal string for the audit line (the uid, or the SID
    /// string `S-1-5-…`). Never the raw SID bytes.
    pub fn display(&self) -> String {
        match self {
            #[cfg(unix)]
            PeerIdentity::Uid(uid) => uid.to_string(),
            #[cfg(windows)]
            PeerIdentity::Sid(sid) => {
                win_sec::sid_bytes_to_string(sid).unwrap_or_else(|_| "<unresolved-sid>".to_string())
            }
        }
    }
}

// ── Rate-limited rejection logger ────────────────────────────────────────────

/// Coalesces repeated foreign-principal rejection log lines so a spammer cannot
/// turn audit logging into a disk-fill DoS.
///
/// Emits at most one coalesced line per `WINDOW` per (principal, socket) pair,
/// reporting the count and first/last timestamps. `principal` is the uid string
/// on Unix and the client SID string on Windows (the `PeerIdentity::display()`).
pub struct RejectionRateLimit {
    // key: (principal, socket_name), value: (count, first_seen, last_seen)
    windows: DashMap<(String, &'static str), (u64, Instant, Instant)>,
}

const REJECTION_WINDOW: Duration = Duration::from_secs(60);

impl RejectionRateLimit {
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
        }
    }

    /// Record a rejection from `principal` on `socket` (a static label like
    /// `"hook"` or `"mcp"`). `principal` is `PeerIdentity::display()` (uid on
    /// Unix, SID string on Windows). Logs at most once per `REJECTION_WINDOW`
    /// per (principal, socket) pair; bursts within the window are coalesced.
    pub fn report(&self, principal: &str, socket: &'static str) {
        let now = Instant::now();
        let mut entry = self
            .windows
            .entry((principal.to_string(), socket))
            .or_insert((0, now, now));
        let (count, first_seen, last_seen) = entry.value_mut();
        *count += 1;
        if now.duration_since(*last_seen) >= REJECTION_WINDOW {
            // Window expired: emit the coalesced summary and reset
            tracing::warn!(
                ipc_event = "peer_rejected",
                principal,
                socket,
                burst_count = *count,
                window_secs = REJECTION_WINDOW.as_secs(),
                "rejected {count} connection(s) from {principal} on {socket} in the last {}s",
                REJECTION_WINDOW.as_secs()
            );
            *count = 0;
            *first_seen = now;
        } else if *count == 1 {
            // First rejection in a new window — emit immediately
            tracing::warn!(
                ipc_event = "peer_rejected",
                principal,
                socket,
                "rejected connection from {principal} on {socket} (further rejections coalesced)"
            );
        }
        *last_seen = now;
    }

    /// Flush any pending window (called on graceful shutdown so the last
    /// burst is not silently dropped).
    pub fn flush(&self) {
        for entry in self.windows.iter() {
            let ((principal, socket), (count, _, _)) = entry.pair();
            if *count > 0 {
                tracing::warn!(
                    ipc_event = "peer_rejected",
                    principal,
                    socket,
                    burst_count = count,
                    "flushing coalesced rejections on shutdown"
                );
            }
        }
    }
}

impl Default for RejectionRateLimit {
    fn default() -> Self {
        Self::new()
    }
}

// ── PlatformListener / PlatformStream (async) ────────────────────────────────
//
// A thin seam over the Unix implementation so call sites reference neither
// `tokio::net::UnixListener` nor `tokio::net::UnixStream` directly.  The
// Windows iteration drops in a named-pipe body here without touching the hook
// server or MCP daemon.

/// Async IPC listener.  Binds a path, removes any stale endpoint, and accepts
/// connections as `(PlatformStream, peer_uid)`.
#[cfg(unix)]
pub struct PlatformListener {
    listener: tokio::net::UnixListener,
    path: PathBuf,
}

#[cfg(unix)]
impl PlatformListener {
    /// Bind the listener at `path`.  If a socket file already exists a
    /// liveness probe determines whether to remove it (ECONNREFUSED → stale)
    /// or refuse (live instance).
    pub fn bind(path: &Path) -> io::Result<Self> {
        if path.exists() {
            if probe_socket_alive(path) {
                tracing::info!(
                    ipc_event = "bind_deferred",
                    path = %path.display(),
                    "live socket already bound; deferring"
                );
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("live socket already exists at {}", path.display()),
                ));
            }
            // Provably dead — safe to unlink
            std::fs::remove_file(path)?;
        }

        // Enforce sun_path limit before trying to bind (a silent truncation
        // would produce a bind on the wrong path, not an error).
        let path_str = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "socket path is not UTF-8")
        })?;
        if path_str.len() > SUN_PATH_MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "socket path length {} exceeds sun_path limit {} on this platform: {}",
                    path_str.len(),
                    SUN_PATH_MAX,
                    path.display()
                ),
            ));
        }

        let listener = tokio::net::UnixListener::bind(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// Accept one connection, returning the stream and the peer identity.
    pub async fn accept(&self) -> io::Result<(PlatformStream, PeerIdentity)> {
        let (stream, _addr) = self.listener.accept().await?;
        let uid = peer_uid_of(&stream)?;
        Ok((PlatformStream(stream), PeerIdentity::Uid(uid)))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for PlatformListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Async IPC stream (accepted side).  Implements `AsyncRead + AsyncWrite`
/// so `tokio::io::BufReader::new(stream)` and the length-framing helpers in
/// `mcp/transport.rs` work without modification.
#[cfg(unix)]
pub struct PlatformStream(tokio::net::UnixStream);

#[cfg(unix)]
impl PlatformStream {
    /// Connect to a bound listener (shim / CLI async side).
    pub async fn connect(path: &Path) -> io::Result<Self> {
        tokio::net::UnixStream::connect(path).await.map(Self)
    }

    /// Access the inner stream (needed by the MCP transport's `peer_uid`).
    pub fn inner(&self) -> &tokio::net::UnixStream {
        &self.0
    }
}

// SAFETY: tokio::net::UnixStream is Unpin (it wraps a raw fd behind a mutex).
#[cfg(unix)]
impl Unpin for PlatformStream {}

#[cfg(unix)]
impl tokio::io::AsyncRead for PlatformStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

#[cfg(unix)]
impl tokio::io::AsyncWrite for PlatformStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

// ── Windows named-pipe transport ─────────────────────────────────────────────
//
// The Unix model ported to Windows: an owner-only security descriptor on the
// pipe (the ACL analog of 0600 + the 0700 dir) + a client-SID peer check on
// accept (the analog of SO_PEERCRED). A named-pipe server is a LOOP — each
// accepted connection consumes one instance, so `accept` creates the next
// instance (with the SAME owner-only SD — Decision 4) before handing the
// current one to the caller.

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, HANDLE};

/// Create one server instance of the named pipe with the owner-only security
/// descriptor. `first` toggles `FILE_FLAG_FIRST_PIPE_INSTANCE`: anti-squat on
/// the first instance, and MUST be false on instances 2..N (it would make
/// `create` fail `ERROR_ACCESS_DENIED`). The SD is identical on every instance
/// (Decision 4 — a plain `create()` on later instances would grant the default
/// SD, re-opening the foreign-open gap).
#[cfg(windows)]
fn make_pipe_instance(name: &str, sid: &str, first: bool) -> io::Result<NamedPipeServer> {
    win_sec::with_owner_only_security_attributes(sid, |sa| {
        let mut opts = ServerOptions::new();
        opts.first_pipe_instance(first).reject_remote_clients(true);
        // SAFETY: `sa` points to a valid SECURITY_ATTRIBUTES for the duration
        // of this call (the closure outlives it); the SD is freed afterwards.
        unsafe { opts.create_with_security_attributes_raw(name, sa) }
    })
}

#[cfg(windows)]
pub struct PlatformListener {
    name: String,
    sid: String,
    // The not-yet-connected server instance ready for the next accept. Swapped
    // out (the connected one returned) and a fresh one swapped in per accept.
    current: std::sync::Mutex<Option<NamedPipeServer>>,
}

#[cfg(windows)]
impl PlatformListener {
    /// Bind the first pipe instance. `first_pipe_instance(true)` makes `create`
    /// fail `PermissionDenied` if any instance of this name already exists — a
    /// squatter pre-created it; refuse to start the surface (Decision 3).
    pub fn bind(path: &Path) -> io::Result<Self> {
        let name = path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "pipe name is not UTF-8"))?
            .to_string();
        let sid = win_sec::current_user_sid_string()?;
        let first = make_pipe_instance(&name, &sid, true).map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                tracing::error!(
                    ipc_event = "squat_detected",
                    path = %name,
                    "named pipe already exists at bind — refusing to start (possible squat)"
                );
            }
            e
        })?;
        Ok(Self {
            name,
            sid,
            current: std::sync::Mutex::new(Some(first)),
        })
    }

    /// Accept one connection. Waits on the current instance, then creates the
    /// NEXT instance (same owner-only SD) BEFORE handing this one off, so the
    /// pipe is continuously available (Decision 9 — shrink the ERROR_PIPE_BUSY
    /// window). The client SID is extracted synchronously (Decision 5).
    pub async fn accept(&self) -> io::Result<(PlatformStream, PeerIdentity)> {
        // Take the ready instance (recreate if somehow absent).
        let server = {
            let mut guard = self.current.lock().unwrap();
            guard.take()
        };
        let server = match server {
            Some(s) => s,
            None => make_pipe_instance(&self.name, &self.sid, false)?,
        };

        server.connect().await?;

        // Make the next instance available before serving this one. If it fails
        // we leave `current` empty; the next accept recreates it (None branch).
        match make_pipe_instance(&self.name, &self.sid, false) {
            Ok(next) => {
                *self.current.lock().unwrap() = Some(next);
            }
            Err(e) => {
                tracing::warn!(
                    ipc_event = "pipe_instance_create_failed",
                    error = %e,
                    "could not pre-create next pipe instance; will retry on next accept"
                );
            }
        }

        // Synchronous SID extraction — NO await between connect() and here, and
        // none inside client_sid_of (Decision 5: an await in the impersonation
        // window could migrate the task and strand the impersonation).
        let handle = HANDLE(server.as_raw_handle());
        let sid = win_sec::client_sid_of(handle)?;
        Ok((PlatformStream::Server(server), PeerIdentity::Sid(sid)))
    }

    pub fn path(&self) -> &Path {
        Path::new(&self.name)
    }
}

/// Accepted (`Server`) or connected (`Client`) named-pipe stream. Both ends
/// impl `AsyncRead + AsyncWrite`, so the framing helpers + `BufReader` work
/// unchanged.
#[cfg(windows)]
pub enum PlatformStream {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

#[cfg(windows)]
impl PlatformStream {
    /// Connect to a bound pipe (shim / async client side). Retries on
    /// `ERROR_PIPE_BUSY` (server momentarily between instances — Decision 9),
    /// then verifies the server's owner SID is the current user before the
    /// caller sends any payload (Decision 3, client side).
    pub async fn connect(path: &Path) -> io::Result<Self> {
        let name = path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "pipe name is not UTF-8"))?;
        const MAX_BUSY_RETRIES: u32 = 50; // ~1s at 20ms
        let mut tries = 0u32;
        let client = loop {
            match ClientOptions::new().open(name) {
                Ok(c) => break c,
                Err(e)
                    if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32)
                        && tries < MAX_BUSY_RETRIES =>
                {
                    tries += 1;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => return Err(e),
            }
        };
        // Anti-impersonation: confirm the pipe is owned by us before trusting it.
        win_sec::verify_pipe_owner_is_current_user(HANDLE(client.as_raw_handle()))?;
        Ok(PlatformStream::Client(client))
    }
}

#[cfg(windows)]
impl tokio::io::AsyncRead for PlatformStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            PlatformStream::Client(c) => std::pin::Pin::new(c).poll_read(cx, buf),
        }
    }
}

#[cfg(windows)]
impl tokio::io::AsyncWrite for PlatformStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        match self.get_mut() {
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            PlatformStream::Client(c) => std::pin::Pin::new(c).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_flush(cx),
            PlatformStream::Client(c) => std::pin::Pin::new(c).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            PlatformStream::Client(c) => std::pin::Pin::new(c).poll_shutdown(cx),
        }
    }
}

// ── Sync connect (for hook CLI blocking paths) ────────────────────────────────

/// Synchronous connect to a bound listener.  Used by the hook CLI which runs
/// blocking I/O (synchronous `std::fs::read_to_string` etc.) and cannot use
/// tokio streams.
///
/// WHY separate from `PlatformStream::connect`: the async and sync worlds must
/// not be mixed.  The hook CLI is a standalone blocking binary; forcing it onto
/// a tokio runtime just to send a newline-framed message adds unnecessary
/// complexity.
#[cfg(unix)]
pub fn sync_connect(path: &Path) -> io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(path)
}

/// Synchronous named-pipe client connect (blocking). Opening `\\.\pipe\name`
/// with `OpenOptions` is `CreateFile(OPEN_EXISTING)` under the hood; the
/// returned `File` impls `Read + Write` like the Unix `UnixStream`. Retries on
/// `ERROR_PIPE_BUSY` / `ERROR_FILE_NOT_FOUND` (server between instances / pipe
/// not yet created — Decision 9), then verifies the pipe owner is the current
/// user (Decision 3, client side) before returning.
#[cfg(windows)]
pub fn sync_connect(path: &Path) -> io::Result<std::fs::File> {
    const MAX_RETRIES: u32 = 50; // ~1s at 20ms
    let mut tries = 0u32;
    let file = loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            Ok(f) => break f,
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(c) if c == ERROR_PIPE_BUSY.0 as i32 || c == ERROR_FILE_NOT_FOUND.0 as i32
                ) && tries < MAX_RETRIES =>
            {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    };
    win_sec::verify_pipe_owner_is_current_user(HANDLE(file.as_raw_handle()))?;
    Ok(file)
}

// Fallback stub for any target that is neither unix nor windows. The hook-CLI
// blocking callers drive the returned value with synchronous `Read`/`Write`, so
// the `Ok` type must implement both even though it is never constructed.
#[cfg(not(any(unix, windows)))]
fn unsupported_sync() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "synchronous IPC is not implemented on this platform",
    )
}

#[cfg(not(any(unix, windows)))]
pub struct UnsupportedSyncStream(());

#[cfg(not(any(unix, windows)))]
impl io::Read for UnsupportedSyncStream {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(unsupported_sync())
    }
}

#[cfg(not(any(unix, windows)))]
impl io::Write for UnsupportedSyncStream {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(unsupported_sync())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(unsupported_sync())
    }
}

#[cfg(not(any(unix, windows)))]
pub fn sync_connect(_path: &Path) -> io::Result<UnsupportedSyncStream> {
    Err(unsupported_sync())
}

// ── Windows synchronous plan-review gate server (overlapped accept) ──────────
//
// The plan-review CLI is a blocking binary with no tokio runtime: it HOSTS the
// approval endpoint and reads the decision the GUI writes (mirroring the Unix
// mkfifo+poll model — Decision 6). The named-pipe analog of FIFO
// `O_NONBLOCK`+`poll(timeout)` is an OVERLAPPED `ConnectNamedPipe` armed once,
// then `WaitForSingleObject(event, tick)` per tick so the CLI can run the
// `gui.pid` liveness check between waits (research §7).

#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES, ReadFile,
    WriteFile,
};
#[cfg(windows)]
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
#[cfg(windows)]
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, NAMED_PIPE_MODE, PIPE_ACCESS_DUPLEX,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    CreateEventW, INFINITE, ResetEvent, SetEvent, WAIT_OBJECT_0, WAIT_TIMEOUT, WaitForSingleObject,
};

#[cfg(windows)]
fn coreerr(e: windows::core::Error) -> io::Error {
    io::Error::other(e)
}

/// A blocking, owner-only named-pipe server for the plan-review approval gate.
/// Single instance (one gate at a time). `accept_with_timeout` polls so the CLI
/// can interleave liveness checks; the connected stream reads the decision.
#[cfg(windows)]
pub struct SyncPipeServer {
    handle: Option<HANDLE>,
    event: HANDLE,
    overlapped: Box<OVERLAPPED>,
    armed: bool,
}

/// Create the plan-review gate pipe with the owner-only SD + overlapped mode.
/// `FILE_FLAG_OVERLAPPED` is REQUIRED or `ConnectNamedPipe` blocks with no
/// timeout (research §7.2). MUST complete before the PlanReview notification is
/// sent (the GUI then connects — Decision 6 ordering invariant).
#[cfg(windows)]
pub fn sync_listen(path: &Path) -> io::Result<SyncPipeServer> {
    let name = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "pipe name is not UTF-8"))?;
    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let sid = win_sec::current_user_sid_string()?;

    let open_mode = FILE_FLAGS_AND_ATTRIBUTES(
        PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_OVERLAPPED.0 | FILE_FLAG_FIRST_PIPE_INSTANCE.0,
    );
    let pipe_mode = NAMED_PIPE_MODE(PIPE_TYPE_BYTE.0 | PIPE_WAIT.0 | PIPE_REJECT_REMOTE_CLIENTS.0);

    let handle = win_sec::with_owner_only_security_attributes(&sid, |sa| {
        // SAFETY: `name_w` is NUL-terminated and `sa` is a valid
        // SECURITY_ATTRIBUTES for the call; the result is checked for validity.
        let h = unsafe {
            CreateNamedPipeW(
                windows::core::PCWSTR(name_w.as_ptr()),
                open_mode,
                pipe_mode,
                1,    // nMaxInstances: one gate at a time
                4096, // out buffer
                4096, // in buffer
                0,    // default timeout (unused; we drive ConnectNamedPipe)
                Some(sa as *const windows::Win32::Security::SECURITY_ATTRIBUTES),
            )
        };
        if h.is_invalid() {
            return Err(io::Error::last_os_error());
        }
        Ok(h)
    })?;

    // SAFETY: standard manual-reset event creation.
    let event = unsafe { CreateEventW(None, true, false, None) }.map_err(coreerr)?;
    let mut overlapped = Box::new(OVERLAPPED::default());
    overlapped.hEvent = event;

    Ok(SyncPipeServer {
        handle: Some(handle),
        event,
        overlapped,
        armed: false,
    })
}

#[cfg(windows)]
impl SyncPipeServer {
    /// Wait up to `tick_ms` for a client. `Ok(None)` = timeout (caller runs the
    /// liveness check and calls again — the SAME pending op keeps waiting;
    /// `ConnectNamedPipe` is NOT re-issued, research §7.4). `Ok(Some(..))` =
    /// connected, with the verified peer identity.
    pub fn accept_with_timeout(
        &mut self,
        tick_ms: u32,
    ) -> io::Result<Option<(SyncPlatformStream, PeerIdentity)>> {
        let handle = self
            .handle
            .ok_or_else(|| io::Error::other("sync pipe server already consumed"))?;

        // Arm ConnectNamedPipe exactly once (research §7.1/§7.4).
        if !self.armed {
            // SAFETY: `handle` is a valid overlapped pipe; `overlapped` has a
            // stable boxed address and a manual-reset event.
            let res = unsafe { ConnectNamedPipe(handle, Some(&mut *self.overlapped)) };
            match res {
                // TRUE is unexpected for an overlapped pipe (research §7.3).
                Ok(()) => {
                    return Err(io::Error::other(
                        "ConnectNamedPipe returned TRUE on an overlapped pipe",
                    ));
                }
                Err(e) if e.code() == windows::Win32::Foundation::ERROR_IO_PENDING.to_hresult() => {
                    // Normal: pending.
                }
                Err(e)
                    if e.code()
                        == windows::Win32::Foundation::ERROR_PIPE_CONNECTED.to_hresult() =>
                {
                    // Client connected before the call — signal the event so the
                    // wait loop completes on the next tick (research §7.3).
                    // SAFETY: `self.event` is a valid manual-reset event.
                    unsafe { SetEvent(self.event) }.map_err(coreerr)?;
                }
                Err(e) => return Err(coreerr(e)),
            }
            self.armed = true;
        }

        // SAFETY: `self.event` is valid; WaitForSingleObject is always safe.
        let status = unsafe { WaitForSingleObject(self.event, tick_ms) };
        if status == WAIT_TIMEOUT {
            return Ok(None);
        }
        if status != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }

        // Completed: confirm and extract bytes-transferred (bwait=false: done).
        let mut transferred = 0u32;
        // SAFETY: the op is complete (event signaled); overlapped is the same
        // boxed struct passed to ConnectNamedPipe.
        unsafe { GetOverlappedResult(handle, &*self.overlapped, &mut transferred, false) }
            .map_err(coreerr)?;

        // Synchronous SID extraction — no await anywhere in the CLI (Decision 5).
        let sid = win_sec::client_sid_of(handle)?;
        let peer = PeerIdentity::Sid(sid);

        // Hand the connected pipe to the stream; the server keeps only the event
        // (closed on drop). The connect op is done, so the boxed OVERLAPPED is
        // no longer referenced by the kernel.
        let pipe = self.handle.take().expect("handle present");
        self.armed = false; // op consumed
        // SAFETY: fresh manual-reset event for the stream's overlapped reads.
        let read_event = unsafe { CreateEventW(None, true, false, None) }.map_err(coreerr)?;
        Ok(Some((
            SyncPlatformStream {
                handle: pipe,
                event: read_event,
            },
            peer,
        )))
    }
}

#[cfg(windows)]
impl Drop for SyncPipeServer {
    fn drop(&mut self) {
        // SAFETY: best-effort teardown of owned handles. If a connect op is
        // still armed, cancel it and drain the completion before closing
        // (research §7.4: the kernel holds a pointer to the OVERLAPPED).
        unsafe {
            if let Some(h) = self.handle {
                if self.armed {
                    let _ = CancelIoEx(h, Some(&*self.overlapped));
                    let _ = WaitForSingleObject(self.event, INFINITE);
                }
                let _ = DisconnectNamedPipe(h);
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
            let _ = windows::Win32::Foundation::CloseHandle(self.event);
        }
    }
}

/// The connected plan-review pipe (server side). `Read`/`Write` over the
/// overlapped handle via a per-call OVERLAPPED + blocking `GetOverlappedResult`.
#[cfg(windows)]
pub struct SyncPlatformStream {
    handle: HANDLE,
    event: HANDLE,
}

#[cfg(windows)]
impl io::Read for SyncPlatformStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: overlapped read on a valid handle with the stream's event;
        // GetOverlappedResult(bwait=true) blocks until done.
        unsafe {
            let mut ol = OVERLAPPED::default();
            ol.hEvent = self.event;
            let _ = ResetEvent(self.event);
            let pending_or_ok = ReadFile(self.handle, Some(buf), None, Some(&mut ol));
            match pending_or_ok {
                Ok(()) => {}
                Err(e) if e.code() == windows::Win32::Foundation::ERROR_IO_PENDING.to_hresult() => {
                }
                Err(e)
                    if e.code() == windows::Win32::Foundation::ERROR_BROKEN_PIPE.to_hresult() =>
                {
                    return Ok(0); // peer closed → EOF
                }
                Err(e) => return Err(coreerr(e)),
            }
            let mut n = 0u32;
            match GetOverlappedResult(self.handle, &ol, &mut n, true) {
                Ok(()) => Ok(n as usize),
                Err(e)
                    if e.code() == windows::Win32::Foundation::ERROR_BROKEN_PIPE.to_hresult() =>
                {
                    Ok(0)
                }
                Err(e) => Err(coreerr(e)),
            }
        }
    }
}

#[cfg(windows)]
impl io::Write for SyncPlatformStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // SAFETY: overlapped write mirroring `read`.
        unsafe {
            let mut ol = OVERLAPPED::default();
            ol.hEvent = self.event;
            let _ = ResetEvent(self.event);
            match WriteFile(self.handle, Some(buf), None, Some(&mut ol)) {
                Ok(()) => {}
                Err(e) if e.code() == windows::Win32::Foundation::ERROR_IO_PENDING.to_hresult() => {
                }
                Err(e) => return Err(coreerr(e)),
            }
            let mut n = 0u32;
            GetOverlappedResult(self.handle, &ol, &mut n, true).map_err(coreerr)?;
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for SyncPlatformStream {
    fn drop(&mut self) {
        // SAFETY: owned handles closed once.
        unsafe {
            let _ = DisconnectNamedPipe(self.handle);
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
            let _ = windows::Win32::Foundation::CloseHandle(self.event);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 5.1 Comparison-branch test (CI) — exercises `PeerIdentity::
    // matches_current_process`, the only enforced access control at every
    // accept loop. The branch that silently disappears if a refactor drops
    // the peer check.
    //
    // IMPORTANT: this asserts the SELF-vs-FOREIGN comparison, NOT that the OS
    // delivers the real foreign principal. The real two-account harness
    // (`sudo -u nobody` on Unix / a second Windows user) is UNVERIFIED-pending
    // and cannot run in single-account CI.
    #[cfg(unix)]
    #[test]
    fn peer_identity_rejects_foreign_uid() {
        let me = unsafe { libc::getuid() };
        assert!(
            PeerIdentity::Uid(me).matches_current_process(),
            "self uid {me} must be accepted"
        );
        assert!(
            !PeerIdentity::Uid(me.wrapping_add(1)).matches_current_process(),
            "foreign uid must be rejected (comparison branch)"
        );
        // uid 0 (root) is foreign when the app runs non-root (skip if CI is root).
        assert!(
            me == 0 || !PeerIdentity::Uid(0).matches_current_process(),
            "uid 0 must be rejected when the app is non-root"
        );
    }

    #[cfg(windows)]
    #[test]
    fn peer_identity_rejects_foreign_sid() {
        // The current process owner SID must match itself.
        let me = win_sec::process_owner_sid().expect("current process owner SID");
        assert!(
            PeerIdentity::Sid(me).matches_current_process(),
            "self SID must be accepted"
        );
        // Well-known "Everyone" SID (S-1-1-0), a valid SID that is never the
        // interactive user — exercises the reject branch.
        let everyone: Box<[u8]> = Box::new([0x01, 0x01, 0, 0, 0, 0, 0, 0x01, 0, 0, 0, 0]);
        assert!(
            !PeerIdentity::Sid(everyone).matches_current_process(),
            "foreign (Everyone) SID must be rejected (comparison branch)"
        );
    }

    // 5.1b Path-location assertion
    //
    // Verifies that the resolved hook/mcp socket paths are inside the per-user
    // IPC dir and that the parent of that dir is NOT group/world-writable.
    // A future refactor that silently reverts endpoints to bare temp_dir()/tmp
    // (the squat-DoS/TOCTOU mitigation) fails this test.
    #[cfg(unix)]
    #[test]
    fn resolved_paths_are_inside_per_user_dir() {
        use std::os::unix::fs::MetadataExt;

        let dir = ipc_dir().expect("ipc_dir must succeed in test environment");
        let hook = hook_socket_path().expect("hook_socket_path");
        let mcp = mcp_socket_path().expect("mcp_socket_path");

        // Both paths must be children of the per-user IPC dir
        assert!(
            hook.starts_with(&dir),
            "hook socket {hook:?} must be inside ipc_dir {dir:?}"
        );
        assert!(
            mcp.starts_with(&dir),
            "mcp socket {mcp:?} must be inside ipc_dir {dir:?}"
        );

        // The PARENT of the IPC dir must not be group/world-writable.
        // On Linux the parent is /run/user/<uid> (0700) or ~ (0700/0755).
        // Shared sticky /tmp would be 0777 — this assertion catches a
        // regression where we silently fall back to /tmp.
        let parent = dir.parent().expect("ipc_dir must have a parent");
        if parent.exists() {
            let meta = std::fs::metadata(parent).expect("parent metadata");
            let mode = meta.mode() & 0o022; // group-write + other-write bits
            assert_eq!(
                mode,
                0,
                "IPC dir parent {} has group/world-write bits set ({:03o}); \
                 this means endpoints could be squatted by another uid — \
                 ipc_dir() must never place endpoints under shared /tmp",
                parent.display(),
                meta.mode() & 0o777
            );
        }
    }

    // 1.4 Seam unit tests — connect-error path and sync/async split
    //
    // Verifies that a connect to a non-existent path returns a clear error
    // (not a hang) and that the sync connect and async connect are distinct
    // code paths.
    #[cfg(unix)]
    #[test]
    fn sync_connect_to_missing_path_returns_error() {
        let result = sync_connect(Path::new("/tmp/nergal-test-does-not-exist.sock"));
        assert!(
            result.is_err(),
            "sync_connect to missing path must return Err"
        );
        let err = result.unwrap_err();
        // Must be a connection/notfound error, not a panic or unwrap
        assert!(
            err.kind() == io::ErrorKind::NotFound
                || err.kind() == io::ErrorKind::ConnectionRefused
                || err.kind() == io::ErrorKind::AddrNotAvailable,
            "unexpected error kind for missing path: {:?}",
            err.kind()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn async_connect_to_missing_path_returns_error() {
        let result =
            PlatformStream::connect(Path::new("/tmp/nergal-test-does-not-exist.sock")).await;
        assert!(
            result.is_err(),
            "async connect to missing path must return Err"
        );
    }

    // Confirm fnv1a hash is deterministic and produces short filenames
    #[test]
    fn endpoint_path_hash_is_deterministic() {
        let hash1 = fnv1a_32(b"hook.sock");
        let hash2 = fnv1a_32(b"hook.sock");
        assert_eq!(hash1, hash2, "FNV-1a hash must be deterministic");
        assert_ne!(
            fnv1a_32(b"hook.sock"),
            fnv1a_32(b"mcp.sock"),
            "different names must produce different hashes"
        );
    }

    // 6.3 Real foreign-uid extraction harness (committed but UNVERIFIED-pending)
    //
    // This test is committed as documentation of the harness design and is
    // gated behind a feature flag so it does not run in standard CI.
    //
    // To run manually (requires `sudo` and the `nobody` account):
    //
    //   cargo test --features ipc-foreign-uid-harness -- \
    //     real_foreign_uid_extraction_harness --nocapture
    //
    // The harness:
    //   1. Binds a test socket in a deliberately-traversable (0711) parent dir.
    //   2. Stages a tiny connector binary in a world-execable location.
    //   3. Runs it as `nobody` via `sudo -u nobody`.
    //   4. Asserts that peer_cred().uid() returns nobody's real uid AND the
    //      boundary rejects it post-accept.
    //
    // IMPORTANT: the production socket is in the 0700 IPC dir which EACCES-
    // blocks a foreign uid at connect, making peer_cred unreachable. This test
    // MUST use a relaxed-perm parent — otherwise it only tests the directory
    // permissions (already known) and leaves the LOCAL_PEERCRED/SO_PEERCRED
    // extraction unverified while believing it verified.
    //
    // Status: UNVERIFIED-pending (cannot run sudo non-interactively in CI;
    // manual verification required on a machine with `sudo -u nobody` access).
    // Run with: cargo test -- real_foreign_uid_extraction_harness --ignored
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "UNVERIFIED-pending: requires `sudo -u nobody` access; run manually"]
    async fn real_foreign_uid_extraction_harness() {
        use std::os::unix::fs::PermissionsExt;

        // Bind a test socket in a 0711 dir (traversable by nobody)
        let test_dir = tempfile::tempdir().expect("tempdir");
        std::fs::set_permissions(test_dir.path(), std::fs::Permissions::from_mode(0o711))
            .expect("chmod test dir 0711");
        let sock_path = test_dir.path().join("test.sock");

        let listener = tokio::net::UnixListener::bind(&sock_path).expect("bind test socket");
        // Make the socket world-connectable (this is the test socket, not the production one)
        std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o777))
            .expect("chmod test socket 0777");

        // Stage connector in /tmp (world-execable)
        // The connector simply connects and exits.
        // On macOS, /Users/<dev> is 0700 so we cannot exec from target/; must stage in /tmp.
        let connector_src = r#"
use std::os::unix::net::UnixStream;
fn main() {
    let path = std::env::args().nth(1).expect("socket path");
    let _s = UnixStream::connect(&path).expect("connect");
    // connection established; peer_cred will be readable by the server
    std::thread::sleep(std::time::Duration::from_millis(200));
}
"#;
        // NOTE: Actual compilation of the connector binary requires `rustc` available.
        // In a real verification, compile the connector out of band and set the path here.
        // This test records the harness design; the actual run is UNVERIFIED-pending.
        let _ = (connector_src, &listener);

        // Expected outcome when run:
        // - peer reaches accept()
        // - peer_cred().uid() returns nobody's uid (typically 65534)
        // - the boundary check (uid != app_uid) rejects it
        // - EACCES-at-connect would mean the test socket is not traversable (harness invalid)
        //
        // Record as UNVERIFIED-pending: sudo -u nobody is not available in non-interactive CI.
        eprintln!("UNVERIFIED-pending: real foreign-uid extraction requires `sudo -u nobody`");
        eprintln!("To verify: run this test manually on Linux/macOS with `sudo -u nobody` access.");
        eprintln!("Test socket would be at: {}", sock_path.display());
    }
}
