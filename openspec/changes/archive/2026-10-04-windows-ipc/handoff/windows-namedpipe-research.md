# Windows Named Pipe IPC — Security Research Facts

> Research date: 2026-06-28. All API surfaces verified against docs.rs (tokio latest / 1.x) and
> microsoft.github.io/windows-docs-rs (windows crate 0.62.2 unless noted). Signatures from
> windows-sys may differ (raw extern "system" vs ergonomic wrappers). This document covers the
> `windows` crate (ergonomic wrappers, returns `Result<()>`) not `windows-sys` (raw BOOL/DWORD).

---

## 1. tokio Named Pipe API (`tokio::net::windows::named_pipe`)

Source: https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/

Requires tokio features: `["net"]`. AsyncRead/AsyncWrite on the server/client structs additionally
require the `io-util` feature (or `full`).

### `ServerOptions` — builder

All builder methods return `&mut Self`.

```rust
pub fn new() -> ServerOptions

// Pipe mode
pub fn pipe_mode(&mut self, pipe_mode: PipeMode) -> &mut Self
pub fn access_inbound(&mut self, allowed: bool) -> &mut Self
pub fn access_outbound(&mut self, allowed: bool) -> &mut Self
pub fn reject_remote_clients(&mut self, reject: bool) -> &mut Self
pub fn max_instances(&mut self, instances: usize) -> &mut Self
pub fn in_buffer_size(&mut self, buffer: u32) -> &mut Self
pub fn out_buffer_size(&mut self, buffer: u32) -> &mut Self

// Security-relevant flags
pub fn first_pipe_instance(&mut self, first: bool) -> &mut Self
pub fn write_dac(&mut self, requested: bool) -> &mut Self
pub fn write_owner(&mut self, requested: bool) -> &mut Self
pub fn access_system_security(&mut self, requested: bool) -> &mut Self

// Creation — standard path (no custom SECURITY_ATTRIBUTES)
pub fn create(&self, addr: impl AsRef<OsStr>) -> io::Result<NamedPipeServer>

// Creation — with raw SECURITY_ATTRIBUTES pointer (CONFIRMED)
pub unsafe fn create_with_security_attributes_raw(
    &self,
    addr: impl AsRef<OsStr>,
    attrs: *mut c_void,   // cast from *mut SECURITY_ATTRIBUTES; null = same as create()
) -> io::Result<NamedPipeServer>
```

`create_with_security_attributes_raw` passes `attrs` as the `lpSecurityAttributes` argument to
`CreateNamedPipeW`. The pointer must either be null or point to a valid `SECURITY_ATTRIBUTES`
for the duration of the call. The SD it points to may be freed immediately after this function
returns.

**CONFIRMED** — method name and signature.

### `NamedPipeServer`

```rust
// Wait for a client to connect (wraps ConnectNamedPipe); cancellation-safe
pub async fn connect(&self) -> io::Result<()>

// Disconnect without closing the handle (so the server instance can be reused)
pub fn disconnect(&self) -> io::Result<()>
```

Trait implementations (CONFIRMED):
- `AsyncRead` + `AsyncWrite` (via `poll_read` / `poll_write` / `poll_flush` / `poll_shutdown`)
- `AsRawHandle` → `fn as_raw_handle(&self) -> RawHandle`
- `AsHandle` → `fn as_handle(&self) -> BorrowedHandle<'_>`
- `Send + Sync + Unpin`

The `AsRawHandle`/`AsHandle` implementations give the underlying `HANDLE` needed for all Win32
peer-auth calls in sections 3–5 below.

### `ClientOptions` and `NamedPipeClient`

```rust
// ClientOptions builder
pub fn new() -> ClientOptions

pub fn open(&self, addr: impl AsRef<OsStr>) -> io::Result<NamedPipeClient>
// Wraps CreateFile with OPEN_EXISTING.
// Errors: NotFound (pipe does not exist), raw OS error ERROR_PIPE_BUSY (server not waiting).
```

`NamedPipeClient` trait implementations (CONFIRMED):
- `AsyncRead` + `AsyncWrite`
- `AsRawHandle` → `fn as_raw_handle(&self) -> RawHandle`
- `AsHandle` → `fn as_handle(&self) -> BorrowedHandle<'_>`

---

## 2. Owner-Only SECURITY_DESCRIPTOR / SECURITY_ATTRIBUTES

### Recommended approach: SDDL string (simplest and correct)

Cargo features required:
```toml
[dependencies.windows]
version = "0.59"           # or whatever version is already in tree; 0.59+ all fine
features = [
    "Win32_Foundation",
    "Win32_Security",
    "Win32_Security_Authorization",
    "Win32_System_Memory",   # for LocalFree
]
```

Relevant types and functions:

```
windows::Win32::Security::SECURITY_ATTRIBUTES        // struct
windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW
windows::Win32::System::Memory::LocalFree
```

**`ConvertStringSecurityDescriptorToSecurityDescriptorW` signature** (CONFIRMED, windows 0.62.2):

```rust
pub unsafe fn ConvertStringSecurityDescriptorToSecurityDescriptorW<P0>(
    stringsecuritydescriptor: P0,
    stringsdrevision: u32,
    securitydescriptor: *mut PSECURITY_DESCRIPTOR,
    securitydescriptorsize: Option<*mut u32>,
) -> Result<()>
where
    P0: Param<PCWSTR>,
```

The `stringsdrevision` must be `SDDL_REVISION_1` = `1u32`. The returned
`PSECURITY_DESCRIPTOR` is heap-allocated (via LocalAlloc) and **must** be freed with `LocalFree`
after use. Pass `None` for `securitydescriptorsize` unless you need the size.

**SDDL string for owner-only pipe (recommended pattern):**

```
D:P(A;;GA;;;{USER_SID})
```

- `D:` = DACL
- `P` = protected (do not inherit from parent; critical for named objects)
- `A;;GA;;;{USER_SID}` = Allow, no object/inherit flags, GENERIC_ALL, to the user SID

Obtain `{USER_SID}` via `ConvertSidToStringSidW` (see §4). Embed directly into the SDDL
format string before calling `ConvertStringSecurityDescriptorToSecurityDescriptorW`.

**Full sequence (pseudo-code):**

```rust
// 1. Get current user SID string (see §4)
let user_sid_string: String = get_current_user_sid_string()?;

// 2. Build SDDL
let sddl = format!("D:P(A;;GA;;;{})", user_sid_string);
let sddl_wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();

// 3. Convert to binary SD
let mut sd: PSECURITY_DESCRIPTOR = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
ConvertStringSecurityDescriptorToSecurityDescriptorW(
    PCWSTR(sddl_wide.as_ptr()),
    1u32,   // SDDL_REVISION_1
    &mut sd,
    None,
)?;

// 4. Build SECURITY_ATTRIBUTES on the stack
let mut sa = SECURITY_ATTRIBUTES {
    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
    lpSecurityDescriptor: sd.0,
    bInheritHandle: BOOL(0),
};

// 5. Create the pipe with these security attributes
let server = server_opts
    .create_with_security_attributes_raw(pipe_name, &mut sa as *mut _ as *mut c_void)?;

// 6. Free the SD (safe to do immediately after create_with_security_attributes_raw returns)
LocalFree(HLOCAL(sd.0));
```

**Alternative (manual DACL building):** `InitializeSecurityDescriptor` +
`InitializeAcl` + `AddAccessAllowedAce` + `SetSecurityDescriptorDacl` are all present in
`windows::Win32::Security`. These are more verbose but avoid string parsing overhead. SDDL is
recommended unless the manual approach is already in use in the codebase.

Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/Authorization/fn.ConvertStringSecurityDescriptorToSecurityDescriptorW.html
Source: https://learn.microsoft.com/en-us/windows/win32/api/sddl/nf-sddl-convertstringsecuritydescriptortosecuritydescriptorw

---

## 3. Client SID Extraction — Server-Side Peer Authentication

Goal: after `server.connect().await?`, read the connected client's user SID.

Cargo features required (additions to §2):
```toml
"Win32_System_Pipes",
"Win32_System_Threading",
```

Module paths (all CONFIRMED):
```
windows::Win32::System::Pipes::ImpersonateNamedPipeClient
windows::Win32::Security::RevertToSelf
windows::Win32::System::Threading::OpenThreadToken
windows::Win32::System::Threading::GetCurrentThread
windows::Win32::Security::GetTokenInformation
windows::Win32::Security::TOKEN_INFORMATION_CLASS  (enum variant: TokenUser = 1)
windows::Win32::Security::TOKEN_USER               (struct)
windows::Win32::Security::TOKEN_ACCESS_MASK        (use TOKEN_QUERY)
```

**Signatures (CONFIRMED):**

```rust
// windows::Win32::System::Pipes
pub unsafe fn ImpersonateNamedPipeClient(hnamedpipe: HANDLE) -> Result<()>

// windows::Win32::Security
pub unsafe fn RevertToSelf() -> Result<()>

// windows::Win32::System::Threading
pub unsafe fn OpenThreadToken(
    threadhandle: HANDLE,
    desiredaccess: TOKEN_ACCESS_MASK,
    openasself: bool,
    tokenhandle: *mut HANDLE,
) -> Result<()>

// windows::Win32::Security
pub unsafe fn GetTokenInformation(
    tokenhandle: HANDLE,
    tokeninformationclass: TOKEN_INFORMATION_CLASS,
    tokeninformation: Option<*mut c_void>,
    tokeninformationlength: u32,
    returnlength: *mut u32,
) -> Result<()>
```

`TOKEN_USER` struct:
```rust
pub struct TOKEN_USER {
    pub User: SID_AND_ATTRIBUTES,
}
pub struct SID_AND_ATTRIBUTES {
    pub Sid: PSID,
    pub Attributes: u32,
}
```

**Full sequence (pseudo-code):**

```rust
let handle: HANDLE = HANDLE(server.as_raw_handle() as isize);

ImpersonateNamedPipeClient(handle)?;

// CRITICAL: RevertToSelf MUST be called on every exit path (use a defer/guard pattern)
let _guard = scopeguard::defer(|| { let _ = RevertToSelf(); });

let mut token: HANDLE = HANDLE(0);
OpenThreadToken(
    GetCurrentThread(),
    TOKEN_QUERY,
    true,   // openasself=true: open token before impersonation checks (avoids deadlock)
    &mut token,
)?;
let _token_guard = scopeguard::defer(|| { let _ = CloseHandle(token); });

// Two-call pattern: first call gets required buffer size
let mut return_len: u32 = 0;
let _ = GetTokenInformation(token, TokenUser, None, 0, &mut return_len);

let mut buf = vec![0u8; return_len as usize];
GetTokenInformation(
    token,
    TokenUser,
    Some(buf.as_mut_ptr() as *mut c_void),
    return_len,
    &mut return_len,
)?;

let token_user = &*(buf.as_ptr() as *const TOKEN_USER);
let client_sid: PSID = token_user.User.Sid;
// client_sid is valid as long as buf is alive
```

**Security invariant:** `RevertToSelf()` must be called even on error paths. Use a RAII guard
(`scopeguard`, `defer!`, or a custom Drop impl). Failing to revert leaves the thread impersonating
the client for all subsequent operations — a security hole.

Sources:
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Pipes/fn.ImpersonateNamedPipeClient.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/fn.RevertToSelf.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Threading/fn.OpenThreadToken.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/fn.GetTokenInformation.html

---

## 4. Current Process Owner SID (for comparison + pipe name embedding)

### Get current user SID

```rust
// windows::Win32::System::Threading
pub unsafe fn OpenProcessToken(
    processhandle: HANDLE,
    desiredaccess: TOKEN_ACCESS_MASK,
    tokenhandle: *mut HANDLE,
) -> Result<()>
```

**CONFIRMED** (windows crate). Module: `windows::Win32::System::Threading`.

Pattern: `OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?` then
`GetTokenInformation(token, TokenUser, ...)` exactly as in §3 — yields a `PSID` pointing
into the local buffer.

### SID comparison — `EqualSid`

```rust
// windows::Win32::Security
pub unsafe fn EqualSid(psid1: PSID, psid2: PSID) -> Result<()>
```

**CONFIRMED** (windows crate 0.62.2). Module: `windows::Win32::Security`.

**IMPORTANT caveat — return value semantics:** Win32 `EqualSid` returns a `BOOL` where `TRUE`
means equal and `FALSE` means not equal (and does **not** set `GetLastError`). The windows crate
wraps BOOL-returning functions as `Result<()>`, mapping FALSE → `Err`. For `EqualSid` this means:
- `Ok(())` = SIDs are equal
- `Err(_)` = SIDs are NOT equal (this is not an OS error; the error value is synthetic)

Check equality with `.is_ok()`, not by matching the error variant.

UNCONFIRMED — implementer must verify the `Err` variant does not spuriously carry a meaningful
error code on a Windows machine. Alternative: use the raw `windows_sys` binding which returns
`BOOL` directly (`!= 0` for equal), avoiding any ambiguity.

Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/fn.EqualSid.html

### Convert SID to string (for embedding in pipe name)

```rust
// windows::Win32::Security::Authorization
pub unsafe fn ConvertSidToStringSidW(
    sid: PSID,
    stringsid: *mut PWSTR,
) -> Result<()>
```

**CONFIRMED**. The returned `PWSTR` points to a LocalAlloc'd wide string of the form
`S-1-5-21-...`. Free with `LocalFree(HLOCAL(stringsid.0 as *mut _))` after use. Convert to
a Rust `String` via `stringsid.to_string()` (windows crate `PWSTR::to_string`) or manually
with `from_wide_ptr`.

**Pipe name pattern for per-user isolation:**

```
\\.\pipe\nergal-{SID_STRING}
```

This prevents cross-user name collisions because each user's SID is unique. The SID string is
safe to embed in a pipe name (contains only `S`, `-`, and ASCII digits).

Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/Authorization/fn.ConvertSidToStringSidW.html

---

## 5. Anti-Squat / Anti-Impersonation

### Server side: `first_pipe_instance(true)`

**CONFIRMED** — `ServerOptions::first_pipe_instance(true)` sets
`FILE_FLAG_FIRST_PIPE_INSTANCE` (0x00080000) in the `dwOpenMode` passed to `CreateNamedPipeW`.

**Semantics (from Win32 docs, CONFIRMED):**

> "If you attempt to create multiple instances of a pipe with this flag, creation of the **first**
> instance succeeds, but creation of the **next** instance fails with `ERROR_ACCESS_DENIED`."

"Multiple instances" here is cross-process: the kernel tracks pipe instances by name globally.
Tokio maps this to `io::ErrorKind::PermissionDenied` on the failing call.

Practical defense:
1. **Squatter creates the pipe before the server:** the server's `create()` call fails with
   `PermissionDenied`. The server should treat this as a security alert and refuse to start.
2. **Squatter tries to create a second instance after the server:** the squatter's call fails
   with `ERROR_ACCESS_DENIED`.

Both directions are covered by setting this flag on the server's first instance.

Source: https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html
Source: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea

### Client side: verify pipe owner before trusting the connection

After `ClientOptions::new().open(pipe_name)` succeeds, the client should verify that the
server end of the pipe is owned by the expected user (the current user for intra-user IPC).

Use `GetSecurityInfo` to retrieve the owner SID from the connected pipe handle:

```rust
// windows::Win32::Security::Authorization
pub unsafe fn GetSecurityInfo(
    handle: HANDLE,
    objecttype: SE_OBJECT_TYPE,       // SE_KERNEL_OBJECT for named pipe handles
    securityinfo: OBJECT_SECURITY_INFORMATION,  // OWNER_SECURITY_INFORMATION = 0x1
    ppsidowner: Option<*mut PSID>,
    ppsidgroup: Option<*mut PSID>,
    ppdacl: Option<*mut *mut ACL>,
    ppsacl: Option<*mut *mut ACL>,
    ppsecuritydescriptor: Option<*mut PSECURITY_DESCRIPTOR>,
) -> WIN32_ERROR   // NOTE: returns WIN32_ERROR, NOT Result<()>
```

**CONFIRMED** signature. Module: `windows::Win32::Security::Authorization`.

**Critical difference from most windows-crate functions:** `GetSecurityInfo` returns `WIN32_ERROR`
(a newtype over `u32`), **not** `Result<()>`. Success check:

```rust
use windows::Win32::Foundation::ERROR_SUCCESS;

let err = GetSecurityInfo(
    handle,
    SE_KERNEL_OBJECT,
    OWNER_SECURITY_INFORMATION,
    Some(&mut owner_sid),
    None,
    None,
    None,
    Some(&mut sd),
);
if err != ERROR_SUCCESS {
    return Err(io::Error::from_raw_os_error(err.0 as i32));
}
// owner_sid now points into sd; valid until LocalFree(HLOCAL(sd.0))
```

Module paths:
- `SE_KERNEL_OBJECT` — `windows::Win32::Security::Authorization::SE_OBJECT_TYPE` (enum variant)
- `OWNER_SECURITY_INFORMATION` — `windows::Win32::Security::OBJECT_SECURITY_INFORMATION`
  constant (value `0x00000001u32`; some versions place it in `Authorization` — verify on target
  version). UNCONFIRMED exact module for the constant — implementer should check.

After retrieving `owner_sid`, compare it to the current user's SID with `EqualSid` (§4).
Free `sd` with `LocalFree` when done. The `owner_sid` pointer is valid only while `sd` is live.

**Why this is necessary:** even with `first_pipe_instance(true)`, a *different* user on the same
machine who created a pipe with the same name (e.g. in a shared terminal server session) before
your server did, could have set the pipe world-readable. Verifying ownership closes this gap.

Source: https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Security/Authorization/fn.GetSecurityInfo.html

---

## 6. CSPRNG for the Loopback Fallback Token

Use the `getrandom` crate (v0.4.x, current latest 0.4.3).

```toml
[dependencies]
getrandom = "0.4"
```

```rust
let mut token = [0u8; 32];  // 256-bit token
getrandom::fill(&mut token)?;
```

**CONFIRMED.** `getrandom::fill(&mut buf)` fills the slice from the OS CSPRNG:
- Windows 10+: `ProcessPrng`
- Windows 7/8: `RtlGenRandom`

The library prioritizes failure over returning low-quality bytes. `fill()` returns
`Result<(), getrandom::Error>`.

`getrandom` is almost certainly already a transitive dependency (via `tokio`, `ring`, or
`rand`) — check `Cargo.lock` before adding explicitly.

Source: https://docs.rs/getrandom/latest/getrandom/

---

## Cargo Feature Summary

Minimum required features for the `windows` crate to cover all APIs in this document:

```toml
[target.'cfg(windows)'.dependencies.windows]
version = "0.59"   # or pin to 0.62 if already in tree
features = [
    "Win32_Foundation",             # HANDLE, BOOL, WIN32_ERROR, HLOCAL, ERROR_SUCCESS
    "Win32_Security",               # SECURITY_ATTRIBUTES, EqualSid, GetTokenInformation,
                                    # TOKEN_USER, TOKEN_INFORMATION_CLASS, TOKEN_ACCESS_MASK,
                                    # RevertToSelf, PSID, OBJECT_SECURITY_INFORMATION
    "Win32_Security_Authorization", # GetSecurityInfo, ConvertSidToStringSidW,
                                    # ConvertStringSecurityDescriptorToSecurityDescriptorW,
                                    # SE_OBJECT_TYPE, SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION
    "Win32_System_Memory",          # LocalFree, HLOCAL
    "Win32_System_Pipes",           # ImpersonateNamedPipeClient
    "Win32_System_Threading",       # OpenProcessToken, OpenThreadToken,
                                    # GetCurrentProcess, GetCurrentThread
]
```

Note: windows-rs child features enable their parent features automatically, so
`Win32_Security_Authorization` does not require an explicit `Win32_Security` entry — but listing
both is harmless and makes intent clear.

---

## Known Gotchas

1. **`first_pipe_instance` bit collision with `WRITE_OWNER`:** Both `FILE_FLAG_FIRST_PIPE_INSTANCE`
   and `WRITE_OWNER` share the value `0x00080000`. Tokio's `ServerOptions` tracks them as separate
   boolean fields and ORs them in separately, so setting both via the builder adds the same bit
   twice (idempotent). In practice, don't set `write_owner(true)` alongside
   `first_pipe_instance(true)` unless you explicitly need WRITE_OWNER access.

2. **SD lifetime for `create_with_security_attributes_raw`:** the `SECURITY_ATTRIBUTES` and the
   `PSECURITY_DESCRIPTOR` it points to need to stay valid only for the duration of the
   `create_with_security_attributes_raw` call. Free after.

3. **`GetSecurityInfo` owner SID lifetime:** `owner_sid` points into the returned
   `PSECURITY_DESCRIPTOR` buffer. Do not use `owner_sid` after calling `LocalFree(sd)`.

4. **`ImpersonateNamedPipeClient` requires the server handle, not the client handle.** The
   `HANDLE` is the server-end handle from `NamedPipeServer::as_raw_handle()`.

5. **`ConvertSidToStringSidW` string must be freed with `LocalFree`, not `drop`.** It is a
   LocalAlloc'd buffer. Forgetting to free it is a memory leak (not UB since it's heap-allocated,
   but accumulates if done per-connection).

6. **`openasself = true` in `OpenThreadToken`:** pass `true` to open the token against the
   server's own security context (before impersonation), not the impersonated client's context.
   Using `false` here will cause the token open to fail when the impersonated user has lower
   privilege than the server's own user.

---

## 6-Line Summary

1. **tokio named pipe API** — `ServerOptions::create_with_security_attributes_raw(&self, addr, *mut c_void) -> Result<NamedPipeServer>` CONFIRMED; `NamedPipeServer::connect(&self) -> async Result<()>` CONFIRMED; both Server and Client implement `AsyncRead + AsyncWrite + AsRawHandle + AsHandle` CONFIRMED.

2. **Owner-only SECURITY_DESCRIPTOR** — use `ConvertStringSecurityDescriptorToSecurityDescriptorW` (`windows::Win32::Security::Authorization`) with SDDL `"D:P(A;;GA;;;{USER_SID})"` CONFIRMED; pass result to `create_with_security_attributes_raw`; free with `LocalFree` CONFIRMED.

3. **Client SID extraction** — `ImpersonateNamedPipeClient` (`Win32::System::Pipes`) → `OpenThreadToken(openasself=true)` (`Win32::System::Threading`) → `GetTokenInformation(TokenUser)` (`Win32::Security`) → `RevertToSelf()` (`Win32::Security`); all signatures CONFIRMED; RevertToSelf-on-error CONFIRMED requirement.

4. **Current process owner SID + comparison** — `OpenProcessToken` + `GetTokenInformation(TokenUser)` both CONFIRMED in `Win32::System::Threading`/`Win32::Security`; `EqualSid` CONFIRMED but return-value semantics (Ok=equal, Err=not-equal) UNCONFIRMED — verify on Windows or use `windows-sys` raw `BOOL`; `ConvertSidToStringSidW` in `Win32::Security::Authorization` CONFIRMED.

5. **Anti-squat** — `first_pipe_instance(true)` prevents cross-process squatting (create fails `PermissionDenied` if ANY instance exists globally) CONFIRMED by Win32 docs; client-side owner verification via `GetSecurityInfo(SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION)` (`Win32::Security::Authorization`) is the right approach CONFIRMED, but `GetSecurityInfo` returns `WIN32_ERROR` not `Result<()>` — success check against `ERROR_SUCCESS` required CONFIRMED.

6. **CSPRNG** — `getrandom` crate v0.4.x, `getrandom::fill(&mut [u8; 32])?` CONFIRMED; Windows 10+ uses `ProcessPrng`; likely already a transitive dependency — check `Cargo.lock` first.

---

## 7. Raw-Win32 synchronous timeout-bounded named-pipe accept (overlapped ConnectNamedPipe)

> Research date: 2026-06-28. API surfaces verified against MSDN (learn.microsoft.com) and
> microsoft.github.io/windows-docs-rs (windows crate 0.62.2). All signatures from the ergonomic
> `windows` crate unless noted. The `windows-sys` crate exposes raw `BOOL`/`DWORD` returns
> instead of `Result<()>`; signatures differ accordingly.

---

### 7.1 Canonical overlapped `ConnectNamedPipe` + `WaitForSingleObject(timeout)` pattern

**CONFIRMED** — the MSDN "Named Pipe Server Using Overlapped I/O" example and the
`ConnectNamedPipe` reference page together establish the canonical sequence.

**Critical correction on event reset mode:** MSDN explicitly states: *"the OVERLAPPED structure
should contain a handle to a **manual-reset** event object."* The MSDN overlapped I/O code
example creates the event as `CreateEvent(NULL, TRUE /*manual-reset*/, FALSE /*not signaled*/,
NULL)`. Question 1's premise of "auto-reset event" is **incorrect per MSDN** — use
`bManualReset = true` (`CreateEventW(None, true, false, None)?`). With a manual-reset event
`WaitForSingleObject` does not auto-clear the signal; the caller must `ResetEvent` before
re-arming.

**Full sequence:**

```rust
// 1. Create a manual-reset, initially-not-signaled event.
//    Module: windows::Win32::System::Threading
//    Feature: Win32_System_Threading
let event: HANDLE = CreateEventW(
    None,   // lpeventattributes: no security attributes
    true,   // bmanualreset: MANUAL-RESET (per MSDN requirement)
    false,  // binitialstate: not signaled initially
    None,   // lpname: unnamed
)?;

// 2. Zero-initialize OVERLAPPED and set the event handle.
//    Module: windows::Win32::System::IO   Feature: Win32_System_IO
let mut ol = OVERLAPPED::default();
ol.hEvent = event;

// 3. Arm: call ConnectNamedPipe ONCE before the wait loop.
//    Module: windows::Win32::System::Pipes  Feature: Win32_System_Pipes
//    Returns Result<()>; for overlapped pipes Ok(()) is UNEXPECTED (see §7.3).
let cn_result = ConnectNamedPipe(pipe_handle, Some(&mut ol as *mut OVERLAPPED));
// → handle cn_result per §7.3 table before entering the loop

// 4. Tick loop: wait on the SAME pending operation each iteration.
loop {
    // WaitForSingleObject returns WAIT_EVENT (newtype u32)
    // Module: Win32::System::Threading   Feature: Win32_System_Threading
    let status = WaitForSingleObject(event, tick_ms);
    match status {
        // WAIT_OBJECT_0 (0) — event signaled: connection arrived
        WAIT_OBJECT_0 => {
            // Retrieve result of the now-complete operation.
            // bwait=false: operation must be done; do not block here.
            GetOverlappedResult(pipe_handle, &ol, &mut 0u32, false)?;
            break; // connected
        }
        // WAIT_TIMEOUT (0x102) — still pending; do liveness check then loop
        WAIT_TIMEOUT => {
            // run_liveness_check();
            // Do NOT re-call ConnectNamedPipe here.
            continue;
        }
        // WAIT_FAILED (0xFFFF_FFFF) or anything else — real error
        _ => return Err(io::Error::last_os_error()),
    }
}
```

The MSDN multi-instance example uses `WaitForMultipleObjects(..., INFINITE)` rather than
`WaitForSingleObject` with a timeout; the single-pipe timeout variant is a standard direct
derivation of the same pattern.

Sources:
- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-server-using-overlapped-i-o
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe

---

### 7.2 `FILE_FLAG_OVERLAPPED` requirement

**CONFIRMED** — `FILE_FLAG_OVERLAPPED` (`0x40000000`) must be included in the `dwOpenMode`
parameter of `CreateNamedPipeW` for overlapped `ConnectNamedPipe` to be asynchronous.

MSDN `CreateNamedPipeW` on `FILE_FLAG_OVERLAPPED`:

> "If this mode is enabled, functions performing read, write, and connect operations that may
> take a significant time to be completed can return immediately."

MSDN `ConnectNamedPipe` confirms the requirement from the other direction:

> "If *hNamedPipe* was opened with `FILE_FLAG_OVERLAPPED` and *lpOverlapped* is not **NULL**,
> the OVERLAPPED structure should contain a handle to a manual-reset event object."

Without `FILE_FLAG_OVERLAPPED`, `ConnectNamedPipe` ignores the `lpOverlapped` parameter and
blocks the thread until a client connects (or an error occurs). There is no timeout in that mode.

`FILE_FLAG_OVERLAPPED` is defined as type `FILE_FLAGS_AND_ATTRIBUTES` in
`windows::Win32::Storage::FileSystem` (feature: `Win32_Storage_FileSystem`). See §7.5 for
feature implications.

Sources:
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew
- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-open-modes
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe

---

### 7.3 `ERROR_PIPE_CONNECTED` race — return-value handling table

**CONFIRMED** from MSDN `ConnectNamedPipe` reference and the overlapped I/O code example.

MSDN `ConnectNamedPipe` on the race:

> "If a client connects before the function is called, the function returns zero and
> `GetLastError()` returns `ERROR_PIPE_CONNECTED`. This can happen if a client connects in the
> interval between the call to `CreateNamedPipe` and the call to `ConnectNamedPipe`. In this
> situation, there is a **good connection** between client and server, even though the function
> returns zero."

MSDN `ConnectNamedPipe` on overlapped return values:

> "If the operation is still pending, the return value is zero and `GetLastError()` returns
> `ERROR_IO_PENDING`."
> "If the function fails, the return value is zero and `GetLastError` returns a value other
> than `ERROR_IO_PENDING` or `ERROR_PIPE_CONNECTED`."

The overlapped I/O example's `ConnectToNewClient` function further confirms: it explicitly
checks that `ConnectNamedPipe` returns zero (treating `TRUE` as unexpected), and handles the
three outcomes.

**Complete return-value handling table for overlapped mode** (using the `windows` crate's
`Result<()>` wrapper, where `FALSE` maps to `Err` and `TRUE` maps to `Ok(())`):

| windows crate return | `GetLastError()` / error code | Meaning | Action |
|---|---|---|---|
| `Ok(())` (TRUE) | — | Synchronous completion; **unexpected** for an overlapped-mode pipe | Treat as error; log and abort |
| `Err(e)` | `ERROR_IO_PENDING` | Normal: overlapped I/O in progress | Enter `WaitForSingleObject` tick loop (§7.1) |
| `Err(e)` | `ERROR_PIPE_CONNECTED` | Client connected before `ConnectNamedPipe` was called | **SUCCESS** — manually signal the event (`SetEvent(ol.hEvent)`) then proceed to serve |
| `Err(e)` | anything else | Real error | Propagate error |

The MSDN overlapped example implements the `ERROR_PIPE_CONNECTED` case with an explicit
`SetEvent(lpo->hEvent)` call so that the wait loop sees the event as signaled and calls
`GetOverlappedResult` on the next iteration; this is the standard idiom.

```rust
// Handling ConnectNamedPipe result in the windows crate:
match ConnectNamedPipe(pipe_handle, Some(&mut ol as *mut OVERLAPPED)) {
    Ok(()) => {
        // TRUE returned — unexpected for overlapped pipe; treat as error
        return Err(io::Error::other("ConnectNamedPipe returned TRUE on overlapped pipe"));
    }
    Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
        // Normal: pending. Enter tick loop.
    }
    Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => {
        // Client already connected. Signal the event to unify the code path.
        SetEvent(ol.hEvent)?;
        // Fall through to the tick loop (which will immediately see WAIT_OBJECT_0)
    }
    Err(e) => return Err(e.into()),
}
```

Sources:
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe
- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-server-using-overlapped-i-o

---

### 7.4 Per-tick loop hygiene — arm once, wait in loop, `CancelIo` on give-up

**CONFIRMED** from the MSDN overlapped I/O example structure.

The MSDN example calls `ConnectToNewClient` (which issues `ConnectNamedPipe`) **once** per pipe
instance before the event loop, and never re-calls `ConnectNamedPipe` during the wait. The
loop calls `WaitForMultipleObjects` (or equivalently `WaitForSingleObject` for a single pipe)
repeatedly until the operation completes.

**Correct pattern — arm once, wait N times:**

```
arm:    ConnectNamedPipe(handle, &mut overlapped)   ← called ONCE
loop:
  tick: WaitForSingleObject(event, tick_ms)
        → WAIT_TIMEOUT  → liveness check; loop (DO NOT re-call ConnectNamedPipe)
        → WAIT_OBJECT_0 → GetOverlappedResult(handle, &overlapped, ...)  → connected
        → WAIT_FAILED   → real error

give-up: CancelIo(handle) or CancelIoEx(handle, Some(&overlapped))
         then WaitForSingleObject(event, INFINITE) to drain completion
```

**Why re-arming per tick is wrong:** `ConnectNamedPipe` on an already-pending overlapped handle
returns `ERROR_PIPE_LISTENING` (or fails with `ERROR_INVALID_PARAMETER` / `ERROR_IO_PENDING`
again). The overlapped operation is already queued in the kernel; issuing a second
`ConnectNamedPipe` before the first completes is undefined behaviour for this API and can
leak handles or corrupt the OVERLAPPED state.

**`CancelIo` vs `CancelIoEx`:**

```rust
// CancelIo: cancels ALL pending I/O issued by the calling thread for this handle.
// Module: windows::Win32::System::IO   Feature: Win32_System_IO
CancelIo(handle)?;

// CancelIoEx: cancels a specific overlapped operation (or all if lpoverlapped is None).
// Preferred when multiple overlapped ops are in flight on the same handle.
CancelIoEx(handle, Some(&ol as *const OVERLAPPED))?;
```

After either cancel call, the pending `ConnectNamedPipe` completes with `ERROR_OPERATION_ABORTED`.
The caller **must** wait for the completion (via `WaitForSingleObject` or `GetOverlappedResult`)
before freeing the `OVERLAPPED` struct or closing the pipe handle — failing to do so is UB
because the kernel still holds a pointer to the struct.

```rust
// Safe cleanup after CancelIo/CancelIoEx:
CancelIoEx(handle, Some(&ol as *const OVERLAPPED)).ok(); // ignore "not found" error
// Wait for the kernel to finish writing the completion:
WaitForSingleObject(event, INFINITE); // or use GetOverlappedResult(..., bwait=true)
CloseHandle(event)?;
```

MSDN on `CancelIoEx`:
> "The operation being canceled is completed with the error `ERROR_OPERATION_ABORTED`, and
> the error is reported via the normal completion mechanisms for the operation."

Sources:
- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-server-using-overlapped-i-o
- https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex
- https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelio

---

### 7.5 windows crate module paths + Cargo features

All module paths CONFIRMED from microsoft.github.io/windows-docs-rs (windows 0.62.2).

#### Function → module table

| Symbol | Module | windows crate signature |
|---|---|---|
| `ConnectNamedPipe` | `Win32::System::Pipes` | `pub unsafe fn ConnectNamedPipe(hnamedpipe: HANDLE, lpoverlapped: Option<*mut OVERLAPPED>) -> Result<()>` (UNCONFIRMED exact sig — verify on Windows; wrapping is `Result<()>` per crate convention) |
| `CreateEventW` | `Win32::System::Threading` | `pub unsafe fn CreateEventW<P3>(lpeventattributes: Option<*const SECURITY_ATTRIBUTES>, bmanualreset: bool, binitialstate: bool, lpname: P3) -> Result<HANDLE> where P3: Param<PCWSTR>` **CONFIRMED** |
| `WaitForSingleObject` | `Win32::System::Threading` | `pub unsafe fn WaitForSingleObject(hhandle: HANDLE, dwmilliseconds: u32) -> WAIT_EVENT` **CONFIRMED** |
| `OVERLAPPED` | `Win32::System::IO` | `#[repr(C)] pub struct OVERLAPPED { pub Internal: usize, pub InternalHigh: usize, pub Anonymous: OVERLAPPED_0, pub hEvent: HANDLE }` **CONFIRMED** |
| `CancelIo` | `Win32::System::IO` | `pub unsafe fn CancelIo(hfile: HANDLE) -> Result<()>` **CONFIRMED** |
| `CancelIoEx` | `Win32::System::IO` | `pub unsafe fn CancelIoEx(hfile: HANDLE, lpoverlapped: Option<*const OVERLAPPED>) -> Result<()>` **CONFIRMED** |
| `GetOverlappedResult` | `Win32::System::IO` | `pub unsafe fn GetOverlappedResult(hfile: HANDLE, lpoverlapped: *const OVERLAPPED, lpnumberofbytestransferred: *mut u32, bwait: bool) -> Result<()>` **CONFIRMED** |
| `FILE_FLAG_OVERLAPPED` | `Win32::Storage::FileSystem` | constant of type `FILE_FLAGS_AND_ATTRIBUTES`, value `0x40000000` **CONFIRMED** |

#### `WAIT_EVENT` return type of `WaitForSingleObject`

`WAIT_EVENT` is a newtype wrapper over `u32` in the windows crate
(`windows::Win32::System::Threading`). Relevant constants (CONFIRMED from MSDN; exact
windows-crate newtype wrapping UNCONFIRMED — verify on Windows machine):

| Constant | Value | Meaning |
|---|---|---|
| `WAIT_OBJECT_0` | `0x00000000` | Event signaled (operation complete) |
| `WAIT_TIMEOUT` | `0x00000102` | Timeout elapsed; operation still pending |
| `WAIT_FAILED` | `0xFFFFFFFF` | Error — call `GetLastError()` |

#### Required Cargo features

The existing design lists: `Win32_Foundation, Win32_Security, Win32_Security_Authorization,
Win32_System_Memory, Win32_System_Pipes, Win32_System_Threading`.

**`Win32_System_IO` MUST be added** to enable `OVERLAPPED`, `CancelIo`, `CancelIoEx`, and
`GetOverlappedResult`.

**`Win32_Storage_FileSystem` — UNCONFIRMED whether already transitive.** `FILE_FLAG_OVERLAPPED`
lives in `windows::Win32::Storage::FileSystem`. The `CreateNamedPipeW` function (in
`Win32_System_Pipes`) takes `dwopenmode: FILE_FLAGS_AND_ATTRIBUTES` — it is plausible that
`Win32_System_Pipes` already enables `Win32_Storage_FileSystem` transitively (because
`FILE_FLAGS_AND_ATTRIBUTES` is defined there), but the windows-rs feature DAG is not fully
documented. **Verify on the target Windows machine**: if the build fails with "unresolved
`FILE_FLAG_OVERLAPPED`", add `Win32_Storage_FileSystem` explicitly.

**Updated feature block (add `Win32_System_IO`; check `Win32_Storage_FileSystem`):**

```toml
[target.'cfg(windows)'.dependencies.windows]
version = "0.59"   # or pin to 0.62 if already in tree
features = [
    "Win32_Foundation",             # HANDLE, BOOL, WIN32_ERROR, HLOCAL, ERROR_SUCCESS
    "Win32_Security",               # SECURITY_ATTRIBUTES, EqualSid, GetTokenInformation, …
    "Win32_Security_Authorization", # GetSecurityInfo, ConvertSidToStringSidW, …
    "Win32_Storage_FileSystem",     # FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES
                                    # (may be pulled transitively by Win32_System_Pipes)
    "Win32_System_IO",              # OVERLAPPED, CancelIo, CancelIoEx, GetOverlappedResult
                                    # ← NEW — required for §7
    "Win32_System_Memory",          # LocalFree, HLOCAL
    "Win32_System_Pipes",           # CreateNamedPipeW, ConnectNamedPipe,
                                    # ImpersonateNamedPipeClient, NAMED_PIPE_MODE
    "Win32_System_Threading",       # CreateEventW, WaitForSingleObject, SetEvent,
                                    # OpenProcessToken, OpenThreadToken,
                                    # GetCurrentProcess, GetCurrentThread, WAIT_EVENT
]
```

Sources:
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/IO/index.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/IO/fn.CancelIo.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/IO/fn.CancelIoEx.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/IO/fn.GetOverlappedResult.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/IO/struct.OVERLAPPED.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Threading/fn.CreateEventW.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Threading/fn.WaitForSingleObject.html
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Storage/FileSystem/constant.FILE_FLAG_OVERLAPPED.html

---

### 7.6 `CreateNamedPipeW` signature + `PIPE_TYPE`/`PIPE_WAIT` flags + `SECURITY_ATTRIBUTES`

**CONFIRMED** from microsoft.github.io/windows-docs-rs (windows 0.62.2):

```rust
// Module: windows::Win32::System::Pipes   Feature: Win32_System_Pipes
pub unsafe fn CreateNamedPipeW<P0>(
    lpname: P0,
    dwopenmode: FILE_FLAGS_AND_ATTRIBUTES,   // Win32_Storage_FileSystem type
    dwpipemode: NAMED_PIPE_MODE,             // Win32_System_Pipes type
    nmaxinstances: u32,
    noutbuffersize: u32,
    ninbuffersize: u32,
    ndefaulttimeout: u32,
    lpsecurityattributes: Option<*const SECURITY_ATTRIBUTES>,
) -> HANDLE
where
    P0: Param<PCWSTR>,
```

**`lpsecurityattributes: Option<*const SECURITY_ATTRIBUTES>`** — CONFIRMED. Pass `None` for
the default security descriptor (world-readable; insecure). Pass `Some(&sa)` with the
owner-only descriptor built in §2.

**`PIPE_TYPE`/`PIPE_WAIT` flag values (CONFIRMED from MSDN):**

| Constant | `dwPipeMode` value | Meaning |
|---|---|---|
| `PIPE_TYPE_BYTE` | `0x00000000` | Byte-stream pipe |
| `PIPE_TYPE_MESSAGE` | `0x00000004` | Message-mode pipe |
| `PIPE_WAIT` | `0x00000000` | Blocking mode (default; combine with `FILE_FLAG_OVERLAPPED` in `dwOpenMode` for async) |
| `PIPE_NOWAIT` | `0x00000001` | Non-blocking (LAN Manager compat; do NOT use for async I/O) |
| `PIPE_REJECT_REMOTE_CLIENTS` | `0x00000008` | Rejects remote client connections (security hardening) |

For the IPC use-case (local-only, byte-oriented, overlapped): `dwOpenMode` =
`PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE`, `dwPipeMode` =
`PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS`.

In the windows crate these flags are of type `FILE_FLAGS_AND_ATTRIBUTES` (ORed into `dwopenmode`)
and `NAMED_PIPE_MODE` (ORed into `dwpipemode`) respectively. Both types are transparent
`u32`-newtypes with `BitOr` implementations.

Sources:
- https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/System/Pipes/fn.CreateNamedPipeW.html
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew

---

## 6-Line Summary (§7 addendum)

1. **Overlapped ConnectNamedPipe + WaitForSingleObject(timeout)** — canonical MSDN pattern CONFIRMED: create MANUAL-RESET event (`CreateEventW(None, true, false, None)`), store in `OVERLAPPED.hEvent`, call `ConnectNamedPipe` once, then `WaitForSingleObject(event, tick_ms)` in a loop. NOTE: MSDN specifies MANUAL-RESET, NOT auto-reset — the premise in Q1 is incorrect. CONFIRMED.

2. **`FILE_FLAG_OVERLAPPED` in `dwOpenMode`** — CONFIRMED from MSDN `CreateNamedPipeW` + `ConnectNamedPipe` docs; without this flag `ConnectNamedPipe` blocks synchronously and ignores `lpOverlapped`. Constant lives in `windows::Win32::Storage::FileSystem`; feature `Win32_Storage_FileSystem` (may be transitive from `Win32_System_Pipes` — UNCONFIRMED).

3. **`ERROR_PIPE_CONNECTED` race** — CONFIRMED: `ConnectNamedPipe` returns `FALSE` + `ERROR_PIPE_CONNECTED` when client connected before the call; this is a SUCCESS (good connection exists). Full table: `TRUE`=unexpected error for overlapped; `FALSE`+`ERROR_IO_PENDING`=pending; `FALSE`+`ERROR_PIPE_CONNECTED`=success (signal event manually); `FALSE`+other=real error. CONFIRMED.

4. **Per-tick loop hygiene** — CONFIRMED: arm `ConnectNamedPipe` ONCE before the loop; on `WAIT_TIMEOUT` do liveness check and re-wait on the SAME pending op (never re-call `ConnectNamedPipe`); on give-up call `CancelIoEx(handle, Some(&ol))` then drain the completion before freeing `OVERLAPPED`. Re-arming per tick would double-issue an already-queued kernel operation. CONFIRMED.

5. **`Win32_System_IO` must be added** — `OVERLAPPED` struct, `CancelIo`, `CancelIoEx`, and `GetOverlappedResult` all live in `windows::Win32::System::IO` (feature `Win32_System_IO`). `CreateEventW`/`WaitForSingleObject`/`SetEvent` in `Win32::System::Threading`. `FILE_FLAG_OVERLAPPED` in `Win32::Storage::FileSystem`. `Win32_System_IO` is absent from the current design feature list and MUST be added. `Win32_Storage_FileSystem` transitivity UNCONFIRMED — verify on Windows machine. CONFIRMED (`Win32_System_IO`); UNCONFIRMED (transitive `Win32_Storage_FileSystem`).

6. **`CreateNamedPipeW` signature** — `lpsecurityattributes: Option<*const SECURITY_ATTRIBUTES>` CONFIRMED; pass `None` for default (insecure) or `Some(&sa)` for owner-only (§2). `dwpipemode` type is `NAMED_PIPE_MODE` (bitflag u32 newtype). For local IPC: `dwOpenMode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE`, `dwPipeMode = PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS`. CONFIRMED.
