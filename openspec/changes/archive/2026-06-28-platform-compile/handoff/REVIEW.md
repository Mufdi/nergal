# iterative-plan-review — platform-compile

Evaluator: claude · persona: architect · 2026-06-26
Round 1: 8 issues (2 HIGH, 4 MEDIUM, 2 LOW) -> REVISE
Round 2: 0 blocking (5 LOW residual, all absorbed) -> **APPROVED**

Full transcript:
```
=== Round 1 (claude) ===
Warning: no stdin data received in 3s, proceeding without it. If piping from a slow command, redirect stdin explicitly: < /dev/null to skip, or wait longer.
I verified the plan's claims against the actual `src-tauri/` tree. Findings below; several are grounded in greps I ran.

## Review: platform-compile

**Verified-correct claims (no action):**
- `libc` IS under `[target.'cfg(target_os = "linux")'.dependencies]` (Cargo.toml:114-115). D1's premise holds; moving to `cfg(unix)` is the right fix.
- All three `Command::new("which")` sites exist (commands.rs:179, 185, 2026). D2 sound.
- `webkit2gtk` is correctly already linux-gated (Cargo.toml:118).
- The 0o600 fallback-file seams (linear/auth.rs, clickup/auth.rs) compile on macOS as unix and `.mode(0o600)` still applies — secret-file perms are NOT weakened on macOS. No regression.

---

1. **[HIGH] The CI gate cannot enforce the invariant the spec says it enforces.** `spec.md` → "Requirement: CI cross-target compile gate" → Scenario "PR with new ungated unix seam fails CI" asserts that a new `std::os::unix::*` import or `libc::` call *outside* a `#[cfg]` block "SHALL fail the cross-check CI job." This is provably **false**. The gate target is `aarch64-apple-darwin`, and macOS *is* a unix target — a new `UnixStream` import or `libc::kill()` call compiles cleanly there. The macOS check catches `target_os = "linux"`-only and non-unix-superset breakage, which is strictly weaker than "no ungated unix seam." Catching ungated unix requires a **non-unix (Windows) target** in CI, which is explicitly deferred. The flagship enforcement scenario must be rewritten to claim only what a macOS-only check actually guarantees ("compiles on macOS"), or the gate is overselling protection it does not provide. This also undercuts the proposal's "Why" ("every sibling change risks reintroducing Linux-only assumptions" — the gate only catches the macOS-breaking subset).

2. **[HIGH] The gate's feasibility — the change's primary deliverable — is unverified, and two near-certain hard blockers are framed as contingent.** `keyring` with the `async-secret-service` feature (Cargo.toml:109) and `zbus` (Cargo.toml:36) are **unconditional** deps (under `[dependencies]`, no target qualifier). Both are D-Bus/Secret-Service and definitionally cannot compile for macOS. design.md "Additional compile risks" and task 5.2 hedge these with "if the CI gate fails on keyring/zbus" — they are not "if," they are guaranteed. Consequently the change's real scope is larger than the proposal's "move 1 dep + 3 `which` + 1 os-release + 1 opencode path": it necessarily includes gating `zbus` + its call sites and choosing a **macOS keyring backend** (`apple-native`/Keychain). That backend choice is a secret-storage seam — security-relevant — and the plan defers it with zero design. Per the project's "pause on ambiguity when it touches security" rule, this decision should be made in-plan, not "when the gate fires." Until `cargo check --target aarch64-apple-darwin` is run **once** locally (migration step 4), D5's "cheap 30-60s Linux runner" premise is unproven.

3. **[MEDIUM] D5's Linux-runner design is load-bearing on the unverified `wezterm-term` question.** The git-pinned `wezterm-term`/`termwiz` deps (Cargo.toml:85-87) carry C/terminal code. design.md's own Open Question admits these may need the macOS SDK even for `cargo check`. If so, D5 collapses (you need a `macos-latest` runner or `cross`), invalidating the cost rationale the plan used to *reject* the macOS runner. The design commits to a runner choice before resolving the question that determines whether that choice is viable. De-risk by running the local check during revision, not during implementation.

4. **[MEDIUM] The Windows-deferred seam registry is materially incomplete, yet the spec promises completeness.** `spec.md` → "Windows-deferred seams are documented" → Scenario "Deferred seams are findable by implementers ... so no fresh grep is needed." The registry lists **5** sites. Actual `std::os::unix::*` sites in the tree number ~15: missing are `lib.rs:77` (AsRawFd), `migrate_legacy.rs:141`, `mcp/transport.rs:85`, `obsidian/post_session.rs:114,152` (CommandExt), `atomic_write.rs:48,107`, `commands.rs:224`, `pty.rs:250` (CommandExt), `scratchpad/mod.rs:396` (symlink), `linear/auth.rs:296`, `clickup/auth.rs:214` (PermissionsExt). The deliverable fails its own acceptance scenario — the Windows implementer *will* need a fresh grep.

5. **[MEDIUM] D3 (gating `read_os_pretty_name`) is mis-scoped and internally inconsistent.** Reading `/etc/os-release` via `std::fs::read_to_string` with a string-literal path is **not a compile error** on macOS — it is pure `std`. Gating it does zero work toward "cargo check passes" (the stated scope) and is instead a *functional* change (return `None` on macOS), contradicting the non-goal "running on macOS is sibling territory / no functional macOS behavior." Worse, it is arbitrary: `updater.rs:436` reads `/proc/sys/kernel/osrelease` (also Linux-only, also ungated) and is left untouched, as are the `browser.rs`/`pty.rs` `/proc` readers (correctly deferred to platform-proc). Either runtime Linux-path reads are out of scope here (then drop D3 — it compiles fine) or they're in scope (then D3 must also cover `updater.rs:436`). As written, singling out one of two adjacent reads in the same file is incoherent.

6. **[MEDIUM] libc call-site enumeration is incomplete across proposal, design, and spec.** The plan names 3 sites (getuid, statvfs, kill). The tree has ~16: `lib.rs:82,88` (dup2/STDOUT_FILENO/STDERR_FILENO), `obsidian/post_session.rs:130,164` + `pty.rs:253` (setsid), `obsidian/channels.rs:228-233` (time_t/tm/localtime_r), `pty.rs:64,109,113,114` (kill). The **fix is still correct** — all are POSIX and present in macOS libc, so the dep move covers them — but `spec.md` Scenario "libc compiles on macOS" enumerates only the 3 sites as the verification surface, so a reader verifies an incomplete set. Update the enumeration so verification is exhaustive.

7. **[LOW] Rollback section understates partial-failure risk.** The Migration Plan claims "no rollback concerns — all changes are Rust source and CI config." True for the source edits, but if task 5.2 ends up gating `keyring`/`zbus` and swapping the keyring backend, a botched feature-flag/target-table edit can break the **Linux** build's secret storage (the exact regression class called out in the project's rename GOTCHAs about keyring service loss). Rollback is still git-revertible, but the "no concerns" framing is too strong for a change that touches the keyring dependency.

8. **[LOW] `arboard` (Cargo.toml:41, unconditional) is unmentioned as a macOS-check risk.** Likely compiles (objc bindings), but it belongs in the "Additional compile risks" list alongside keyring/zbus for completeness, since it is another unconditional native dep the gate will exercise first.

---

**Summary:** Direction is sound and the core fixes (D1, D2) are correct and verified. But a flagship spec scenario is provably false (#1), the gate's feasibility and true scope are understated by treating guaranteed blockers as contingent (#2, #3), two completeness deliverables don't meet their own acceptance criteria (#4, #6), and D3 is scope-inconsistent (#5). These are all fixable in a revision pass but material enough that the plan should not proceed as written.

VERDICT: REVISE
=== Round 2 (claude) ===
Warning: no stdin data received in 3s, proceeding without it. If piping from a slow command, redirect stdin explicitly: < /dev/null to skip, or wait longer.
Verified the dependency claims against the real `src-tauri/Cargo.toml` (lines 36, 41, 73, 104-118):
- `keyring = { version = "3", features = ["async-secret-service", "tokio", "crypto-rust"] }` — D7's Linux block reproduces this feature set **byte-identically**. The "byte-identical" claim holds.
- `zbus = { version = "5", default-features = false, features = ["tokio"] }` — confirms D8's "sole direct call site" framing; note the keyring comment (104-108) shows `async-secret-service` pulls its *own* transitive zbus 4.x, which `apple-native` drops on macOS. Consistent with D8.
- `libc = "0.2"` under the linux target table, `webkit2gtk` linux-gated, `which = "6"` present — all as the plan states.

## Review: platform-compile (round 2)

**Round-1 disposition — all eight addressed:**
- **#1 (gate overselling, HIGH)** → fixed. `proposal.md` "Why" now states the macOS gate catches only macOS-breaking regressions; `spec.md` adds the scenario "Ungated unix seam is NOT caught by the macOS gate (documented limit)"; D6 carries the reach limit into CLAUDE.md. The flagship false scenario is gone.
- **#2 (keyring/zbus contingent → guaranteed, HIGH)** → fixed. D7/D8 bring both in-scope with concrete Cargo.toml surgery; the keyring backend (`apple-native`) is decided in-plan per the security-pause rule, no plaintext fallback. Verified the Linux feature set is preserved verbatim.
- **#3 (D5 load-bearing on wezterm, MEDIUM)** → fixed. Task 1 pre-flight empirical `cargo check --target` + a pre-decided fallback ladder (Linux → macos-latest → cross).
- **#4 (registry incomplete, MEDIUM)** → fixed. Registry is now exhaustive (A ungated / B gated / C test-only) and includes every site round 1 flagged (lib.rs:77, migrate_legacy.rs:141, mcp/transport.rs:85/116, post_session.rs:114/152, atomic_write.rs:48/107, commands.rs:224, pty.rs:250, scratchpad:396, linear/clickup auth).
- **#5 (D3 mis-scoped, MEDIUM)** → fixed. D3 dropped; both runtime Linux-path reads (`/etc/os-release` updater.rs:406 **and** `/proc/sys/kernel/osrelease` updater.rs:436) reassigned to `platform-proc`, resolving the prior incoherence.
- **#6 (libc enumeration, MEDIUM)** → fixed. spec scenario now enumerates dup2/setsid/localtime_r/kill-tree, marked "including but not limited to."
- **#7 (rollback understated, LOW)** → fixed. Caveat added re: a botched keyring/zbus target-table edit breaking Linux secret storage.
- **#8 (arboard, LOW)** → fixed. Listed in "Additional compile risks."

**Residual issues (all low severity, none blocking):**

1. **[LOW] `task 4.2` invites a literal `TODO`.** "the non-Linux branch is a no-op or a **TODO-logged** skip" collides with the project's hard convention "No TODO/FIXME — track in issues or OpenSpec changes" (CLAUDE.md Critical conventions). Reword to "log-only skip; the macOS reveal impl is tracked in `platform-desktop`." Otherwise a clippy/grep convention check trips on the artifact this change introduces.

2. **[LOW] Non-goal vs D7 latent tension.** Goals/Non-Goals says "No functional macOS behavior," but selecting `apple-native` *is* a functional runtime decision (macOS secrets land in Keychain). The prose defends it (compile-forced + security rule) and that defense is sound, but the non-goal wording should explicitly carve out the one exception ("…except the keyring backend, which compiling forces") so a future reader can't weaponize the contradiction. Substance is fine; only the wording is unguarded.

3. **[LOW] `task 2.3` ("Move zbus") doesn't state "preserve the declaration verbatim."** The current line carries `default-features = false, features = ["tokio"]`; a relocation that silently drops those flips Linux zbus resolution. "Move" implies verbatim, but given #7's own warning about target-table edits breaking Linux, the task should say "relocate the line unchanged." Same applies implicitly to D8's stub: confirm `reveal_in_downloads`'s non-Linux branch returns `Ok(())` rather than falling into the existing `xdg-open` fallback (which would `cargo check` fine but is dead/misleading on macOS).

4. **[LOW] No explicit task/acceptance criterion forcing a gate if `arboard`/`wezterm-term` genuinely fail (not just SDK-on-Linux artifacts).** The plan assumes any failure is a Linux-runner-missing-SDK artifact curable by switching to `macos-latest` (D5), and design says "if it fails, gate/feature per target" — but no task encodes that branch; only "task 8.1 exits 0" implies it. Task 1.2 covers it loosely. Add a one-line contingency task so implementation doesn't stall on a true incompatibility.

5. **[LOW] CI path filter `src-tauri/**` can miss macOS-breaking changes that don't touch that subtree.** The skip-scenario ("PR with no src-tauri changes skips gate") is correct for source edits, but a workspace-root `Cargo.lock`/dependency change could break the macOS target while the gate skips. For this repo the crate manifest + lockfile live under `src-tauri/`, so the gap is narrow — note it so a future workspace restructuring doesn't silently open a hole.

**Assessment:** The revision is substantive, not cosmetic — the two HIGH findings are genuinely resolved (gate reach is now honestly bounded in proposal/spec/CLAUDE.md; the guaranteed blockers are in-scope with verified-accurate Cargo.toml edits), both MEDIUM completeness deliverables now meet their own acceptance scenarios, and the incoherent D3 is cleanly excised. The empirical pre-flight (task 1) de-risks the remaining open questions (wezterm SDK, apple-native API shape) before any CI commitment. Security posture is sound: real keystore on both targets, 0o600 seams unweakened, backend decided in-plan. The five residual items are wording/contingency polish a competent implementer absorbs during execution; none invalidate the design or its scope.

VERDICT: APPROVED
```
