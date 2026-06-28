## ADDED Requirements

### Requirement: libc scoped to unix targets

The `libc` crate dependency in `Cargo.toml` SHALL be declared under `[target.'cfg(unix)'.dependencies]`, not `[target.'cfg(target_os = "linux")'.dependencies]`. This makes every ungated `libc::*` call site — which are all valid POSIX calls — available on all unix-family targets including macOS without any source-level changes.

#### Scenario: All ungated libc call sites compile on macOS

- **WHEN** the crate is compiled for `aarch64-apple-darwin`
- **THEN** every existing ungated `libc::*` call site SHALL compile without errors — including but not limited to `libc::getuid` (`lib.rs:674`), `libc::statvfs` (`mcp/worktree_sessions.rs:604`), `libc::kill` (`browser.rs:411`, `pty.rs:64,109-114`), `libc::dup2` (`lib.rs:82,88`), `libc::setsid` (`obsidian/post_session.rs`, `pty.rs:253`), and `libc::localtime_r` (`obsidian/channels.rs:228`)

#### Scenario: Linux behavior unchanged

- **WHEN** the crate is compiled for `x86_64-unknown-linux-gnu`
- **THEN** all `libc::*` call sites SHALL compile and behave identically to before this change

#### Scenario: statvfs field-width difference handled portably

- **WHEN** `free_disk_bytes` (`mcp/worktree_sessions.rs`) multiplies `statvfs.f_bavail` by `f_frsize` and the crate is compiled for `aarch64-apple-darwin`
- **THEN** the multiplication SHALL widen both operands to `u64` (Darwin types `f_bavail` as `u32`, `f_frsize` as `u64`; Linux types both as `u64`), so it typechecks and never truncates on either target — the one libc site that required a source edit beyond the dep-scope move

---

### Requirement: Binary-lookup uses the which crate

The crate SHALL NOT invoke `Command::new("which")` to locate executables. All binary-lookup call sites SHALL use `which::which()` from the `which = "6"` crate, which is cross-platform and does not depend on a `which` binary existing in `PATH`.

#### Scenario: Binary lookup compiles on macOS

- **WHEN** `commands.rs` is compiled for `aarch64-apple-darwin`
- **THEN** the three former `Command::new("which")` sites (lines 179, 185, 2026) SHALL compile and resolve binaries correctly via `which::which()`

#### Scenario: Binary lookup compiles on Windows (future)

- **WHEN** `commands.rs` is compiled for `x86_64-pc-windows-msvc`
- **THEN** `which::which()` SHALL resolve without requiring a `which.exe` in `PATH`

---

### Requirement: Unconditional D-Bus / Secret-Service deps compile on macOS

The `keyring` and `zbus` dependencies — declared today under unconditional `[dependencies]` and backed by Linux-only D-Bus / Secret-Service — SHALL be made target-conditional so the crate compiles for `aarch64-apple-darwin`. `keyring` SHALL use the `async-secret-service` feature on Linux and the `apple-native` (macOS Keychain) backend on macOS. `zbus` and its sole call site (`updater.rs` `FileManager1.ShowItems` reveal) SHALL be gated `#[cfg(target_os = "linux")]` with a non-Linux stub. Secret storage SHALL remain functional on every target: Secret Service on Linux, Keychain on macOS — no plaintext fallback is introduced by this change.

#### Scenario: keyring + zbus compile on macOS

- **WHEN** the crate is compiled for `aarch64-apple-darwin`
- **THEN** `keyring` SHALL compile against its macOS Keychain backend and `zbus` SHALL be excluded from the build (its call site behind a `#[cfg(target_os = "linux")]` stub), with no D-Bus compile error

#### Scenario: Linux secret storage unchanged

- **WHEN** the crate is compiled and run for `x86_64-unknown-linux-gnu`
- **THEN** `keyring` SHALL use the Secret Service (`async-secret-service`) backend exactly as before, and the ClickUp/Linear token storage and reveal-in-file-manager paths SHALL behave identically

---

### Requirement: opencode config path matches opencode's real macOS location

The opencode adapter's config/data roots SHALL match the directories opencode itself uses on each target. Research (opencode docs + `sst/opencode#8235`) established that opencode resolves these via the `xdg-basedir` package, which keeps the **Linux XDG layout on macOS** — `~/.config/opencode` and `~/.local/share/opencode`, NOT `~/Library/Application Support/opencode` (the Apple path is opencode's org-level *managed* settings, not user config). The adapter therefore SHALL use `~/.config/opencode` + `~/.local/share/opencode` on macOS as well, and SHALL NOT switch to `dirs::config_dir()` (which returns the wrong `~/Library/Application Support` directory on macOS). A WHY-comment SHALL record this so the path is not mistakenly "corrected" to Apple dirs later.

> Correction note: an earlier draft required deriving the macOS root from `dirs::config_dir()`; the task-1 research reversed that — it would have pointed at the wrong directory. This is not a compile blocker (the existing pure-`PathBuf` join compiles on macOS unchanged); it is a correctness requirement.

#### Scenario: Config root on macOS matches opencode (XDG, not Apple dirs)

- **WHEN** `agents/opencode/adapter.rs` is compiled and run for `aarch64-apple-darwin`
- **THEN** the config root SHALL resolve to `~/.config/opencode` (and data to `~/.local/share/opencode`), NOT `dirs::config_dir()`/`~/Library/Application Support/opencode`

#### Scenario: Config root on Linux unchanged

- **WHEN** `agents/opencode/adapter.rs` is compiled for `x86_64-unknown-linux-gnu`
- **THEN** the config root SHALL continue to resolve to `~/.config/opencode` (XDG convention unchanged)

---

### Requirement: CI cross-target compile gate

The CI pipeline SHALL include a job that runs `cargo check --target aarch64-apple-darwin` on every pull request that modifies files under `src-tauri/`, and SHALL fail the PR if compilation for that target fails. The gate's reach is bounded and SHALL be documented as such: it catches **macOS-breaking (Linux-only) regressions**, NOT newly-added *ungated* `std::os::unix::*` / `libc::*` seams (those compile on macOS because macOS is a unix target — detecting them requires a Windows target, deferred to the Windows iteration).

#### Scenario: PR that breaks macOS compilation fails CI

- **WHEN** a PR introduces a `target_os = "linux"`-only construct, an unconditional Linux-only dependency, or another macOS-incompatible change under `src-tauri/src/`
- **THEN** the `cross-check` CI job SHALL fail with a compile error for `aarch64-apple-darwin`

#### Scenario: Ungated unix seam is NOT caught by the macOS gate (documented limit)

- **WHEN** a PR adds a new ungated `std::os::unix::*` import or `libc::` call under `src-tauri/src/`
- **THEN** the macOS `cross-check` SHALL still pass (macOS is unix), and the CLAUDE.md convention SHALL document that catching this class requires the deferred Windows-target gate — so reviewers do not over-trust the macOS gate

#### Scenario: PR with no src-tauri changes skips gate

- **WHEN** a PR only modifies files outside `src-tauri/`
- **THEN** the `cross-check` job SHALL either be skipped or pass trivially without running a Rust compile

---

### Requirement: Windows-deferred seams are documented exhaustively

Every `std::os::unix::*` site in the tree SHALL be recorded in design.md under a "Windows-deferred seam registry" section, categorized as already-`#[cfg]`-gated (needs a Windows impl/stub), currently-ungated (needs gating), or test-only. The registry SHALL be exhaustive enough that a Windows implementer needs no fresh grep. No `#[cfg]` change is required for these seams in THIS change (they compile on macOS); they are tracked for the Windows-porting iteration.

#### Scenario: Registry is exhaustive and categorized

- **WHEN** an implementer starts the Windows port (a sibling change)
- **THEN** design.md SHALL list every `std::os::unix::*` site with its file:line, its category (gated / ungated / test-only), and a one-line Windows note — with no known site omitted

#### Scenario: Deferred seams compile today on macOS

- **WHEN** the crate is compiled for `aarch64-apple-darwin` after this change
- **THEN** all registry-listed files SHALL compile without errors (because `std::os::unix::*` is available on all unix targets, including macOS)
