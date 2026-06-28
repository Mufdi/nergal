# Design — platform-compile

Design session 2026-06-26 (revised post plan-review round 1). Scope: compile-correctness on macOS + the CI gate. Functional macOS impls belong to sibling changes.

## Context

Nergal's Rust backend grew on Linux-only primitives: libc POSIX calls, `/etc/os-release`, Unix-socket helpers, D-Bus (`zbus`), Secret Service (`keyring`), `webkit2gtk`. It has never been compiled against a non-Linux target. The compile blockers for `aarch64-apple-darwin` are:

1. `libc` dep is declared linux-only (`cfg(target_os = "linux")`), blocking every ungated POSIX call site.
2. `Command::new("which")` in three places (compiles on macOS, but `which` binary is absent on Windows — fixed now to future-proof).
3. **`keyring` (`async-secret-service`) and `zbus` are unconditional `[dependencies]`** backed by Linux-only D-Bus → **guaranteed** macOS compile failures (not contingent).
4. `agents/opencode/adapter.rs` hard-codes `~/.config/opencode` (XDG), which may be wrong on macOS.

`std::os::unix::*` import sites compile on macOS (unix superset) but break on Windows; recorded exhaustively below and deferred.

## Goals / Non-Goals

**Goals:**

- `cargo check --target aarch64-apple-darwin` passes after this change (empirically verified, not assumed — see D5).
- A CI job enforces this on every PR touching `src-tauri/`, with its reach honestly documented.
- The `#[cfg]`-gating convention (and the gate's reach limit) documented in `CLAUDE.md`.
- No Linux behavior changes; secret storage stays functional on every target.

**Non-Goals:**

- Running/linking Nergal on macOS (functional correctness = sibling territory). **One forced exception**: selecting the macOS `keyring` backend (D7) is a runtime decision, but compiling *requires* picking a backend — it cannot be deferred without a plaintext fallback, which the security rule forbids.
- Porting procfs reads, **including the runtime `/etc/os-release` (`updater.rs:406`) and `/proc/sys/kernel/osrelease` (`updater.rs:436`) diagnostics reads** — reassigned to `platform-proc`.
- Porting the Unix-socket IPC transport (`platform-ipc`), desktop integration (`platform-desktop`), bundling/signing (`platform-bundle-ci`).
- Windows compilation (next iteration; Windows-deferred seams tracked below).

## Decisions

### D1: libc scoped to cfg(unix)

**Chosen**: `[target.'cfg(unix)'.dependencies]`. POSIX functions (`getuid`, `statvfs`, `kill`, `dup2`, `setsid`, `localtime_r`) exist on every unix target; scoping to `unix` is semantically correct and compiles all ungated sites on macOS with zero code edits.

**Alternative**: keep `linux`-only + `#[cfg(unix)]` per call site. Rejected: more diff, no semantic gain.

---

### D2: which crate instead of Command::new("which")

**Chosen**: replace the three `commands.rs` sites with `which::which(...)`. The `which = "6"` crate is already declared. Future-proofs for Windows (no `which.exe`).

**Alternative**: `#[cfg(unix)]` + `where.exe` stub. Rejected: crate centralizes the abstraction.

---

### ~~D3: gate read_os_pretty_name~~ — DROPPED (plan-review round 1, finding #5)

`read_os_pretty_name()` reads `/etc/os-release` via `std::fs::read_to_string` of a string literal — that is pure `std` and **compiles fine on macOS**. Gating it does nothing for the compile goal and would be a *functional* change (return `None` on macOS), contradicting the non-goals; it was also incoherent to gate it while leaving the adjacent `/proc/sys/kernel/osrelease` read (`updater.rs:436`) untouched. **Both runtime Linux-path diagnostics reads are reassigned to `platform-proc`**, where macOS functional behavior is owned. No action in this change.

---

### D4: opencode config paths via dirs::config_dir()

**Chosen**: on macOS derive the opencode config root from `dirs::config_dir()`. `dirs = "6"` already declared.

**Implementation note**: the implementer MUST research opencode's actual macOS config location (opencode repo/docs — many Go/Node tools keep `~/.config` even on macOS; others use `Application Support`) before writing code:

```rust
fn opencode_config_root() -> PathBuf {
    #[cfg(target_os = "linux")]
    { dirs::home_dir().unwrap_or_default().join(".config/opencode") }
    #[cfg(not(target_os = "linux"))]
    { dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
        .join("opencode") }
}
```

Adjust the subdir name to match what opencode actually uses.

---

### D5: CI target aarch64-apple-darwin — empirically-resolved runner

**Chosen**: a `cross-check` GitHub Actions job that installs the `aarch64-apple-darwin` target and runs `cargo check --target aarch64-apple-darwin` from `src-tauri/`. **The runner choice (Linux vs macOS) is resolved EMPIRICALLY by task 1 (a local cross-check run) before the CI job is written** — not assumed. `cargo check` does not link, so for pure-Rust code a cheap Linux runner suffices; but C-build-script crates may need the macOS SDK.

**Fallback ladder** (decided up front so implementation does not stall):
1. Linux runner + `rustup target add` — preferred if task 1 shows it works.
2. If a C-extension crate (`wezterm-term`/`termwiz`, `arboard`) requires the macOS SDK even for `cargo check`, switch the cross-check job to a **`macos-latest` runner** (still `cargo check`, no signing/SDK-of-Apple-account needed).
3. `cross` tool only if neither is viable.

**Rationale for not using a full macOS build+test runner here**: that entangles compile-gate with functional-port work; reserved for the functional milestone.

---

### D6: Cross-platform invariant in CLAUDE.md

**Chosen**: add a "Cross-platform invariant" section stating (a) every new OS-specific seam MUST be born `#[cfg]`-gated with a stub, (b) the CI `cross-check` is the enforcement arm, **and (c) its reach limit** — the macOS check catches macOS-breaking regressions but NOT ungated-unix seams (that needs the deferred Windows gate), so reviewers must not over-trust a green cross-check.

---

### D7: keyring backend per target (NEW — plan-review #2, security-relevant)

**Chosen**: make `keyring`'s feature set target-conditional in `Cargo.toml`:

```toml
[target.'cfg(target_os = "linux")'.dependencies.keyring]
version = "3"
features = ["async-secret-service", "tokio", "crypto-rust"]

[target.'cfg(target_os = "macos")'.dependencies.keyring]
version = "3"
features = ["apple-native"]
```

**Rationale**: `async-secret-service` is Linux D-Bus and cannot compile on macOS. `keyring` v3 ships an `apple-native` backend (macOS Keychain). This keeps token storage (ClickUp/Linear OAuth, MCP summary secret) on a real OS keystore on both platforms — **no plaintext fallback introduced**. This is a secret-storage decision, made in-plan per the project's "pause on ambiguity when it touches security" rule, not deferred to "when the gate fires."

**Verification obligation**: task 1's local cross-check confirms the `apple-native` feature resolves; the Keychain API differences (no `target`/collection concept) must not break the existing `keyring::Entry::new(service, user)` call shape — confirm the call sites (`linear/auth.rs`, `clickup/auth.rs`, `mcp/summary/secret.rs`, `migrate_legacy.rs`) compile unchanged against the macOS backend.

**Alternative**: a single cross-platform feature set. Rejected: `keyring`'s backends are mutually-exclusive features chosen per platform; there is no universal feature.

---

### D8: zbus gated to cfg(target_os = "linux") (NEW — plan-review #2)

**Chosen**: move `zbus` to `[target.'cfg(target_os = "linux")'.dependencies]` and gate its sole call site — `show_items_via_dbus()` + `reveal_in_downloads`'s call to it (`updater.rs:329-345,451`) — behind `#[cfg(target_os = "linux")]`, with a non-Linux stub that makes reveal a no-op (returns `Ok(())` / falls through).

**Rationale**: `zbus` is D-Bus, Linux-only, used solely for `FileManager1.ShowItems`. Gating it unblocks the macOS compile now. `platform-desktop` later **removes** this path entirely (Tauri opener plugin's reveal works on all OSes); this change only quarantines it so the crate compiles. Note: `zbus` may still be pulled **transitively** by `keyring`'s `async-secret-service` on Linux — that is fine and expected (Linux-only).

**Alternative**: port the reveal to the opener plugin here. Rejected: that is `platform-desktop`'s scope; doing it here widens this change and couples two capabilities.

## Windows-deferred seam registry (EXHAUSTIVE — plan-review #4)

These `std::os::unix::*` sites compile on macOS (unix superset). None need a change in THIS iteration. Categorized for the Windows port:

**A. Currently UNGATED (compile on macOS; need `#[cfg(unix)]` + Windows stub for Windows):**

| File | Line(s) | Usage | Windows note |
|---|---|---|---|
| `hooks/cli.rs` | 3 | `std::os::unix::net::UnixStream` | named pipe (`\\.\pipe\...`) — see `platform-ipc` |
| `hooks/server.rs` | 207-208 | `PermissionsExt` + `set_permissions(0o600)` | named-pipe ACL; no chmod |
| `mcp/worktree_sessions.rs` | 597 | `OsStrExt` (for `CString` path to `statvfs`) | Windows path encoding (UTF-16) differs |
| `linear/auth.rs` | 232-253 | `OpenOptionsExt.mode(0o600)` (fallback token file) | `SetFileSecurity`/ACL |
| `clickup/auth.rs` | 154-171 | `OpenOptionsExt.mode(0o600)` (fallback token file) | same as linear |

**B. Already `#[cfg(unix)]` / `#[cfg(target_os = "linux")]` gated (compile on macOS via the gate; need a Windows impl/stub branch):**

| File | Line(s) | Usage | Current gate |
|---|---|---|---|
| `lib.rs` | 77 | `AsRawFd` (journald dup2) | `cfg(target_os="linux")` |
| `migrate_legacy.rs` | 141-143 | `PermissionsExt` (exec bit copy) | `cfg(unix)` |
| `mcp/transport.rs` | 85-86 | `PermissionsExt` (socket 0o600) | `cfg(unix)` |
| `mcp/transport.rs` | 116-117 | `peer_uid` via `peer_cred()` | `cfg(unix)` + stub |
| `obsidian/post_session.rs` | 114, 152 | `CommandExt` (`pre_exec`/`setsid`) | `cfg(target_os="linux")` |
| `atomic_write.rs` | 48 | `PermissionsExt` (preserve mode) | `cfg(unix)` |
| `commands.rs` | 224 | `PermissionsExt` (`mode()&0o111`) | inline `if cfg!(unix)` |
| `pty.rs` | 250-253 | `CommandExt` + `setsid` (docker stop) | `cfg(unix)` |

**C. Test-only (no production impact):**

| File | Line(s) | Usage |
|---|---|---|
| `atomic_write.rs` | 107 | `PermissionsExt` (test) |
| `scratchpad/mod.rs` | 396 | `std::os::unix::fs::symlink` (test) |
| `linear/auth.rs` | 296 | `PermissionsExt` (test) |
| `clickup/auth.rs` | 214 | `PermissionsExt` (test) |

> Recipe to re-verify completeness before the Windows port: `rg "std::os::unix" src-tauri/src` + `rg "libc::" src-tauri/src`.

## Additional compile risks the gate exercises (re-classified — plan-review #2, #8)

- **`keyring` `async-secret-service`** — **GUARANTEED blocker**, resolved in-scope by D7 (per-target backend).
- **`zbus`** — **GUARANTEED blocker**, resolved in-scope by D8 (gate to linux).
- **`arboard` (Cargo.toml:41, unconditional)** — native clipboard (objc on macOS); likely compiles but is the other unconditional native dep the gate exercises first. Task 1's local check confirms; if it fails, gate/feature per target.
- **`wezterm-term` / `termwiz` (git-pinned, C/terminal code)** — open question whether `cargo check --target` needs the macOS SDK on a Linux runner; resolved empirically by task 1, feeding D5's runner fallback.
- **`webkit2gtk`** — already `cfg(target_os="linux")` — safe.

## Empirical pre-flight results (task 1 — RUN 2026-06-26, ground truth)

Local `cargo check --target aarch64-apple-darwin` on the Linux dev host, results that **resolve D5 and confirm/correct D7/D8 + scope**:

1. **Runner — D5 RESOLVED to `macos-latest`.** A plain Linux runner CANNOT cross-check: the build aborts in `objc2-exception-helper`'s build script (`cc -arch arm64 -mmacosx-version-min=11.0 -xobjective-c`) — Linux `cc` rejects `-arch` and lacks the macOS SDK/objc headers. Pulled transitively by `arboard`, `tao`/`tauri-runtime-wry`, `rfd`/`tauri-plugin-dialog` (the macOS objc2 GUI stack). This hits BEFORE our crate compiles, so fallback-ladder **branch 2 (macos-latest)** is selected; branch 1 (ubuntu + rustup target) is ruled out. CI job written accordingly.
   - *Local validation method*: a throwaway `cc` wrapper that strips the macOS-only flags and stubs the objc `.o` outputs (cargo check does not link) lets the build proceed past the objc stack to type-check our Rust against the macOS target. This is a LOCAL dev hack only — **not** used in CI (CI compiles the objc for real on macos-latest).
2. **Our-crate blocker set (after the objc stack is bypassed):**
   - `libc` unresolved (10 sites) → fixed by D1 (`cfg(unix)`). ✓
   - `zbus::Connection` at `updater.rs:333` → fixed by D8 gating (+ the 2 downstream `E0282` type-inference errors vanish with it). ✓
   - **NEW, not predicted by the plan:** `worktree_sessions.rs:606` `stat.f_bavail.saturating_mul(stat.f_frsize)` — `mismatched types`. Darwin types `statvfs.f_bavail` as `u32` (`fsblkcnt_t`) and `f_frsize` as `u64` (`c_ulong`); both are `u64` on Linux. The design's claim that libc sites compile "with zero code edits" has this one exception. Fixed by widening both to `u64` (`as u64`), portable + non-truncating on both targets; needs `#[allow(clippy::unnecessary_cast)]` because the cast is a no-op on Linux (where clippy runs) but required on macOS.
   - `find_in_environ` (`mcp/shim.rs:146`) — dead-code warning on macOS (its only prod caller `session_hint_from_ancestors` is `cfg(target_os="linux")`). Gated `#[cfg(any(target_os = "linux", test))]` to match its real call sites. *(Compile-hygiene; the procfs read itself is platform-proc's.)*
3. **D7 CONFIRMED.** `keyring` `apple-native` resolves cleanly for macOS — zero keyring compile errors, the `keyring::Entry::new(service, user)` call shape is unchanged across all four call sites (`linear/auth.rs`, `clickup/auth.rs`, `mcp/summary/secret.rs`, `migrate_legacy.rs`). Cargo.lock gains `security-framework` + `core-foundation` (Keychain) as macOS-target deps; Linux keeps `zbus 4.4.0` via `async-secret-service` (byte-identical). 684 Linux tests pass.
4. **D4 REVERSED by research (task 5.1).** opencode resolves config/data via the `xdg-basedir` package, which keeps the **Linux XDG layout on macOS too** (`~/.config/opencode` + `~/.local/share/opencode`, NOT `~/Library/Application Support` — that path is opencode's org-level *managed* settings). Sources: opencode docs + `sst/opencode#8235`. So the current hard-coded paths are **already correct on macOS** and were never a compile blocker (no adapter.rs error in the cross-check). `dirs::config_dir()` (= `~/Library/Application Support` on macOS) would have INTRODUCED a bug. **D4 implemented as a documenting WHY-comment, no path change** — the spec requirement is corrected accordingly (see spec.md).
5. **`arboard` / `wezterm-term` / `termwiz` produced NO Rust compile errors** in our crate — the only macOS issue they raise is the objc C-build-script SDK requirement (point 1, runner-level), not source incompatibility. No per-target gating of these needed.

## Risks / Trade-offs

- **[Risk] keyring backend swap breaks Linux secret storage** → the rename GOTCHAs already showed keyring-service changes can silently lose ClickUp/Linear tokens. Mitigation: D7 keeps the Linux feature set byte-identical; only the macOS target gets a *new* (additive) block. Verify Linux token round-trip in task 7.
- **[Risk] `wezterm-term` needs macOS SDK for cargo check** → D5 fallback ladder (macos-latest runner) decided up front so implementation does not stall.
- **[Risk] CI gate per-PR latency** → accepted; `cargo check` no-link is ~30-60s (Linux) or a few min (macos runner).
- **[Risk] Sibling changes break the gate** → caught every PR, not at milestone.

## Migration Plan

1. **Pre-flight (empirical):** `rustup target add aarch64-apple-darwin`; `cd src-tauri && cargo check --target aarch64-apple-darwin`. Record the EXACT blocker set (expected: libc, keyring, zbus; possibly arboard/wezterm). This drives D5's runner choice and confirms D7/D8.
2. Cargo.toml: `libc` linux→unix; `keyring` per-target features (D7); `zbus` →linux (D8).
3. Source: three `which` replacements; `zbus` call-site gating + stub; opencode path fix.
4. Re-run the cross-check until exit 0.
5. Full Linux check green (`cargo clippy -- -D warnings && cargo test && cargo fmt --check`) **including a Linux keyring token round-trip** (regression guard for D7).
6. Add the CI `cross-check` job on the runner D5 resolved.
7. CLAUDE.md convention section (with the reach limit).

**Rollback**: all changes are Cargo.toml + Rust source + CI config, git-revertible. **Caveat (plan-review #7)**: the `keyring`/`zbus` Cargo.toml edits touch secret-storage and D-Bus dep resolution — a botched target-table edit can break the **Linux** build's secret storage (the rename-GOTCHA regression class), so a revert must restore the exact Linux feature set, not just drop the macOS block.

## Open Questions — all RESOLVED by the task-1 pre-flight (2026-06-26)

- ~~opencode's actual macOS config dir~~ — **RESOLVED**: opencode keeps the XDG layout (`~/.config/opencode`) on macOS (xdg-basedir; `sst/opencode#8235`). Current code already correct; D4 = documenting comment, no path change. Spec corrected.
- ~~Does `wezterm-term` need the macOS SDK on a Linux runner?~~ — **RESOLVED**: irrelevant — the build aborts earlier in `objc2-exception-helper` (objc C build script). Linux runner ruled out; D5 = `macos-latest`.
- ~~Does `keyring` `apple-native` keep `Entry::new(service, user)` unchanged?~~ — **RESOLVED**: yes, confirmed clean against all four call sites.
