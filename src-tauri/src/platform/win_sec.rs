//! Windows named-pipe security helpers (Win32-only). Mirrors the Unix
//! peer-cred + `0700`-dir model: an owner-only security descriptor on the pipe
//! and a client-SID peer check on accept. All functions are synchronous Win32
//! — the impersonation→token→revert sequence MUST NOT straddle an `.await`
//! (Decision 5), so callers run `client_sid_of` after `connect().await`
//! returns, never across an await point.
//!
//! Signatures verified against `windows` 0.62.2 (see
//! `handoff/windows-namedpipe-research.md`). UNVERIFIED runtime behaviour
//! (`EqualSid` Err=not-equal, `OWNER_SECURITY_INFORMATION` module) is walked on
//! the user's Windows machine.

use std::ffi::c_void;
use std::io;

use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{
    EqualSid, GetLengthSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    PSID, RevertToSelf, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Memory::LocalFree;
use windows::Win32::System::Pipes::ImpersonateNamedPipeClient;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
};
use windows::core::{PCWSTR, PWSTR};

/// SDDL revision passed to `ConvertStringSecurityDescriptorToSecurityDescriptorW`.
const SDDL_REVISION_1: u32 = 1;

fn winerr(ctx: &str, e: windows::core::Error) -> io::Error {
    io::Error::other(format!("{ctx}: {e}"))
}

// ── RAII guards ──────────────────────────────────────────────────────────────

/// Reverts the calling thread out of client impersonation on EVERY exit path
/// (the Decision 5 invariant). Constructed immediately after a successful
/// `ImpersonateNamedPipeClient`, before any fallible call, so no `?` can skip it.
struct RevertGuard;
impl Drop for RevertGuard {
    fn drop(&mut self) {
        // SAFETY: only constructed while the thread is impersonating; reverting
        // is always valid there. A failure here is unrecoverable but logged.
        unsafe {
            if RevertToSelf().is_err() {
                tracing::error!(
                    ipc_event = "revert_failed",
                    "RevertToSelf failed — thread may still be impersonating"
                );
            }
        }
    }
}

/// Closes an owned `HANDLE` on drop.
struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a real handle this guard owns.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// `LocalFree`s a `LocalAlloc`-backed buffer (security descriptor / SID string)
/// on drop.
struct LocalGuard(HLOCAL);
impl Drop for LocalGuard {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a `LocalAlloc`-family call (SDDL convert /
        // ConvertSidToStringSidW), freed exactly once here.
        unsafe {
            let _ = LocalFree(Some(self.0));
        }
    }
}

// ── SID extraction ───────────────────────────────────────────────────────────

/// Copy the `TokenUser` SID out of an open token as owned bytes. The bytes are
/// a self-contained SID usable as a `PSID` (`bytes.as_ptr()`) for later
/// `EqualSid`/`ConvertSidToStringSidW`.
///
/// SAFETY: `token` must be a valid token handle open for `TOKEN_QUERY`.
unsafe fn token_user_sid(token: HANDLE) -> io::Result<Box<[u8]>> {
    // Two-call pattern: first call (null buffer) yields the required size.
    let mut needed = 0u32;
    let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
    if needed == 0 {
        return Err(io::Error::other("GetTokenInformation returned zero size"));
    }
    let mut buf = vec![0u8; needed as usize];
    GetTokenInformation(
        token,
        TokenUser,
        Some(buf.as_mut_ptr() as *mut c_void),
        needed,
        &mut needed,
    )
    .map_err(|e| winerr("GetTokenInformation(TokenUser)", e))?;

    let token_user = &*(buf.as_ptr() as *const TOKEN_USER);
    let psid = token_user.User.Sid;
    let len = GetLengthSid(psid) as usize;
    if len == 0 {
        return Err(io::Error::other("GetLengthSid returned zero"));
    }
    let bytes = std::slice::from_raw_parts(psid.0 as *const u8, len).to_vec();
    Ok(bytes.into_boxed_slice())
}

/// The current process owner SID (= the current user) as owned bytes.
pub fn process_owner_sid() -> io::Result<Box<[u8]>> {
    // SAFETY: standard OpenProcessToken→GetTokenInformation flow; the token is
    // closed by the guard.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| winerr("OpenProcessToken", e))?;
        let _guard = HandleGuard(token);
        token_user_sid(token)
    }
}

/// The connected client's SID, read by briefly impersonating the pipe client.
/// FULLY SYNCHRONOUS — never call across an `.await` (Decision 5): an await
/// inside the impersonation window could migrate the task to another worker
/// thread, leaving a thread impersonating the client.
///
/// `server_handle` is the SERVER end of the pipe (research gotcha 4).
pub fn client_sid_of(server_handle: HANDLE) -> io::Result<Box<[u8]>> {
    // SAFETY: impersonate→revert is bracketed by the RAII guard; the token is
    // closed by its guard; the SID is copied out before either drops.
    unsafe {
        ImpersonateNamedPipeClient(server_handle)
            .map_err(|e| winerr("ImpersonateNamedPipeClient", e))?;
        // CRITICAL: revert on every exit path. Constructed before any `?`.
        let _revert = RevertGuard;

        let mut token = HANDLE::default();
        // openasself=true: open the token in the server's own context, not the
        // impersonated (possibly lower-privilege) client's (research gotcha 6).
        OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token)
            .map_err(|e| winerr("OpenThreadToken", e))?;
        let _token_guard = HandleGuard(token);

        token_user_sid(token)
    }
}

// ── SID comparison + formatting ──────────────────────────────────────────────

/// `EqualSid`: `Ok(())` = equal, `Err` = not equal (synthetic, not an OS error
/// — research §4). Compare via `.is_ok()`.
pub fn sid_eq(a: &[u8], b: &[u8]) -> bool {
    // SAFETY: both slices are valid self-contained SIDs (from token_user_sid /
    // well-known constants).
    unsafe {
        EqualSid(
            PSID(a.as_ptr() as *mut c_void),
            PSID(b.as_ptr() as *mut c_void),
        )
        .is_ok()
    }
}

/// True when `sid` is the current process owner (the same-principal wall).
pub fn sid_matches_current_user(sid: &[u8]) -> bool {
    match process_owner_sid() {
        Ok(owner) => sid_eq(sid, &owner),
        Err(_) => false,
    }
}

/// Render a SID as its `S-1-5-…` string (for pipe names + audit logs).
pub fn sid_bytes_to_string(sid: &[u8]) -> io::Result<String> {
    // SAFETY: `sid` is a valid SID; the returned wide string is LocalFree'd by
    // the guard after it is copied into an owned `String`.
    unsafe {
        let mut pwstr = PWSTR::null();
        ConvertSidToStringSidW(PSID(sid.as_ptr() as *mut c_void), &mut pwstr)
            .map_err(|e| winerr("ConvertSidToStringSidW", e))?;
        let _guard = LocalGuard(HLOCAL(pwstr.0 as *mut c_void));
        pwstr
            .to_string()
            .map_err(|e| io::Error::other(format!("SID string utf16: {e}")))
    }
}

/// The current user's SID string, embedded in per-user pipe names.
pub fn current_user_sid_string() -> io::Result<String> {
    let sid = process_owner_sid()?;
    sid_bytes_to_string(&sid)
}

// ── Owner-only security descriptor ───────────────────────────────────────────

/// Build the owner-only `SECURITY_ATTRIBUTES` (SDDL `D:P(A;;GA;;;<sid>)` — a
/// protected DACL granting GENERIC_ALL to the current user only) and run `f`
/// with a pointer to it. The descriptor is freed after `f` returns: its
/// lifetime need only span the `CreateNamedPipe*` call (research gotcha 2).
pub fn with_owner_only_security_attributes<F, R>(sid_str: &str, f: F) -> io::Result<R>
where
    F: FnOnce(*mut c_void) -> io::Result<R>,
{
    let sddl = format!("D:P(A;;GA;;;{sid_str})");
    let sddl_w: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the SD pointer is valid for the duration of `f` (freed by the
    // guard afterwards); `sa` lives on the stack across the `f` call.
    unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl_w.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .map_err(|e| winerr("ConvertStringSecurityDescriptorToSecurityDescriptorW", e))?;
        let _sd_guard = LocalGuard(HLOCAL(sd.0));

        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        f(&mut sa as *mut SECURITY_ATTRIBUTES as *mut c_void)
    }
}

// ── Client-side pipe owner verification (anti-impersonation) ─────────────────

/// Verify the connected pipe's owner SID is the current user before sending any
/// payload (Decision 3, client side). Closes the different-user-same-name lure
/// on a multi-session host even though the owner-only SD already blocks foreign
/// opens.
///
/// `GetSecurityInfo` returns `WIN32_ERROR`, NOT `Result<()>` (research §5) —
/// success is `== ERROR_SUCCESS`.
pub fn verify_pipe_owner_is_current_user(handle: HANDLE) -> io::Result<()> {
    // SAFETY: `owner` points into `sd`, valid until the guard frees it; the
    // current-user SID is owned bytes.
    unsafe {
        let mut owner = PSID::default();
        let mut sd = PSECURITY_DESCRIPTOR::default();
        let rc = GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            Some(&mut sd),
        );
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc.0 as i32));
        }
        let _guard = LocalGuard(HLOCAL(sd.0));

        let me = process_owner_sid()?;
        let owner_len = GetLengthSid(owner) as usize;
        if owner_len == 0 {
            return Err(io::Error::other("pipe owner SID has zero length"));
        }
        let owner_bytes = std::slice::from_raw_parts(owner.0 as *const u8, owner_len);
        if sid_eq(owner_bytes, &me) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "pipe owner SID is not the current user",
            ))
        }
    }
}
