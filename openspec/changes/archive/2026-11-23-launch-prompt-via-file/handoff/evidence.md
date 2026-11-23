# Evidence: launch-prompt-via-file (FRI-602)

## Baseline

- Base: `main` at `5fbed56`. GitHub CI run on that commit: `success` (clippy, cargo test, fmt, tsc, macos-cross-check, windows-check). No pre-existing failures recorded.
- Branch: `mufdidev/fri-602-launch-prompt-via-file`.

## Plan review

- iprev (opus) 5 rounds, critical risk: R1 REVISE (12) → R2 REVISE (9; convergence rule hit on the launch rollback, user stated the invariant) → R3 REVISE (7) → R4 REVISE (5) → R5 APPROVED (4 low notes folded into design/implementation).
- User decisions 2026-11-23: Codex and OpenCode get the same fix despite the focus freeze; invariant "a failed launch leaves no trace and keeps the prompt".

## Pinned API (for test-writer and Builder)

`src-tauri/src/agents/mod.rs`:
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum PromptAttach { AfterDoubleDash, Flag(&'static str) }`
- `#[derive(Debug, Clone, PartialEq, Eq)] pub struct LaunchPrompt { pub text: String, pub attach: PromptAttach }`
- `SpawnSpec.prompt: Option<LaunchPrompt>`
- `#[cfg(unix)] pub fn launch_prompt_dir() -> std::io::Result<PathBuf>` (production only)
- `pub fn launch_prompt_file_path(dir: &Path, stem: &str) -> std::io::Result<PathBuf>` (`dir/<stem>.txt`; rejects empty stem, `/`, `..`, `'`, whitespace, control chars with `InvalidInput`)
- `#[cfg(unix)] pub fn write_launch_prompt_file(dir: &Path, stem: &str, text: &str) -> std::io::Result<PathBuf>` (mode 0600, `create_new` after removing a stale file)
- `pub fn clear_launch_prompts_in(dir: &Path)`; `pub fn clear_stale_launch_prompts()` (production only)

`src-tauri/src/pty.rs`:
- `#[derive(Debug, Clone, PartialEq, Eq)] pub(crate) enum PromptRef { File(PathBuf), Inline(String) }`
- `pub(crate) fn normalize_launch_prompt(text: &str) -> Option<String>`
- `fn build_launch_command(kind: ShellKind, cwd: &str, prelude: Option<&str>, binary: &Path, args: &[String], prompt: Option<(&PromptRef, &PromptAttach)>) -> String`
- `pub(crate) fn resolve_launch_prompt(kind: ShellKind, text: &str, dir: std::io::Result<PathBuf>, stem: &str) -> std::io::Result<PromptRef>`
- `pub(crate) struct PreparedLaunch { pub command: String, pub prompt_file: Option<PathBuf> }`
- `pub(crate) fn prepare_launch(kind: ShellKind, cwd: &str, prelude: Option<&str>, binary: &Path, args: &[String], prompt: Option<&LaunchPrompt>, dir: std::io::Result<PathBuf>, stem: &str) -> Result<PreparedLaunch, String>` (normalizes, writes the file, builds the command; on error leaves no file)
- `pub(crate) fn send_launch(prepared: &PreparedLaunch, writer: Option<&mut dyn std::io::Write>) -> Result<(), String>` (`None` → Err; write/flush error → Err; never deletes the file: the rollback does)
- `pub(crate) struct LaunchRollback<'a>`: `new(state: &'a PtyManager, agents: &'a AgentRuntimeState, session_id: &'a str, pty_id: &'a str) -> Self` (armed), `set_prompt(&mut self, Option<String>)`, `set_file(&mut self, PathBuf)`, `disarm(&mut self)` (inserts `pty_id` into `launched_ptys`); `Drop` rolls back when armed.
- `PtyManager.launched_ptys: Mutex<HashSet<String>>`; `pub(crate) fn take_launched(state: &PtyManager, pty_id: &str) -> bool`
- `pub(crate) fn discard_launch_prompt(dir: std::io::Result<PathBuf>, pty_id: &str)`

## Group 1: Red (test-writer, sonnet)

- New locked file: `src-tauri/src/pty_launch_prompt_tests.rs` (663 lines; hash in `handoff/.red-tests`). Registered at the end of `src-tauri/src/pty.rs`.
- Inline edits (cannot be hash-locked; the Builder must not change them):
  - `agents/claude_code/adapter.rs`: `spawn_appends_initial_prompt_as_positional_arg` → `spawn_carries_initial_prompt_in_spec_prompt_not_args` (asserts `spec.prompt`, no prompt in `args`, empty prompt → `None`).
  - `agents/codex/adapter.rs`: new `spawn_carries_folded_preamble_in_spec_prompt_only_on_fresh_launch`.
  - `agents/opencode/adapter.rs`: new `spawn_carries_folded_preamble_in_spec_prompt_only_on_fresh_launch` (`Flag("--prompt")`).
  - `pty.rs` inline `mod tests`: `None` sixth argument on the four `build_launch_command` calls; expected strings unchanged.
- Red run: `cargo test --no-run` → 50 compile errors, every one a missing pinned item (`PromptRef`, `PromptAttach`, `LaunchPrompt`, `SpawnSpec.prompt`, `prepare_launch`, `send_launch`, `PreparedLaunch`, `LaunchRollback`, `take_launched`, `discard_launch_prompt`, `normalize_launch_prompt`, `write_launch_prompt_file`, `launch_prompt_file_path`, `clear_launch_prompts_in`, and `build_launch_command` arity). No typo or wrong existing API.
- Not unit-testable (need `start_claude_session` / the EOF thread): "The session was killed during startup" and "A launch that never wrote its command writes nothing to the vault". Their pieces are covered (`Gone`-style rollback, `take_launched`); the wiring is covered by manual walks 2.3–2.7 and the adversarial review.
- Harness note: zsh variant uses `zsh -i` with `ZDOTDIR` (not `-f`, which would skip the rc and the binding).

## Group 1: Green (Builder, sonnet)

- Files: `agents/mod.rs` (PromptAttach, LaunchPrompt, SpawnSpec.prompt, prompt-dir helpers, sweep), `agents/{claude_code,codex,opencode}/adapter.rs` (prompt → `SpawnSpec.prompt`), `agents/pi/adapter.rs` (`prompt: None`), `pty.rs` (PromptRef, push_prompt, normalize/resolve/prepare/send, LaunchRollback, launched_ptys, take_launched, discard_launch_prompt, start_claude_session wiring, EOF finalize gate, kill-path discard), `lib.rs` (startup sweep).
- Deviations reported by the Builder (accepted):
  - The guard's step 1 reuses `release_session_pty` (same owned-check + `mark_dead_if`).
  - `start_claude_session` returns `pty_id.clone()` (the guard borrows `pty_id`).
  - A missing writer or lost reservation now returns `Err("session closed during PTY startup")` and rolls back; before it was a silent skip (intended by the plan).
  - `spawn_pty(...)?` replaces the explicit rollback (the guard covers it).
  - `cfg_attr(not(unix), allow(dead_code / unused_variables))` on `PromptRef` / `resolve_launch_prompt` for the Windows check.
- Locked-test conflict #1 (ruling: TEST WRONG): `after_double_dash_puts_the_reference_last_right_behind_the_separator` asserted the reference holds no " -- ", but design D1's reference `"$(command cat -- '<f>'; command rm -f -- '<f>')"` contains it by contract. Test-writer rewrote only that test to assert the exact tail `claude --continue -- "$(command cat -- '<p>'; command rm -f -- '<p>')"\n`; re-locked at `0f058b84`.
- Comment audit: one WHAT clause removed from the `push_prompt` doc.

### Gates (2026-11-23)

| Gate | Result | Detail |
|------|--------|--------|
| 1. Compile | PASS | `cargo clippy --all-targets -- -D warnings`: no issues; `npx tsc --noEmit`: no errors |
| 2. Tests | PASS | `cargo test`: `1232 passed; 0 failed; 1 ignored` (lib) + integration binaries ok; 23 `launch_prompt_tests` incl. real bash and zsh with `magic-space` |
| 2. Red locks | PASS | 1 locked file unchanged (`0f058b84`) |
| 3. Lint/format | PASS | `cargo fmt --check` exit 0 |
| 5. Deps | SKIP | no manifest changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | WARN | `#[allow(clippy::too_many_arguments)]` on `prepare_launch`: the signature is pinned by the plan (command parts + prompt + dir + stem); bundling them in a struct would only move the same fields |
| 6c. Secrets | PASS | gitleaks clean |
| 4. Security | via B5 security reviewer (risk critical) |
| CI-only | PENDING | `macos-cross-check` and `windows-check` cannot run on this host; they run on the PR/push |

## B5 reviews (opus) and user rulings

- Adversarial: FAIL (3 Important, 5 nits), no code bug. Security: FAIL (1 Important: PowerShell incl. `pwsh` on Linux accepts U+2018–U+201B as single quotes; pre-existing on `main`). Full text in `handoff/REVIEW.md`.
- User (2026-11-23) confirmed all three adversarial Importants as tasks and chose "fail closed on unix" for PowerShell. Nits fixed by default: `push_prompt` doc, stale positional-prompt comments, `launch-prompts/` in `CLAUDE.md` and `docs/hooks.md`. Not acted on: stem allowlist (security review showed no boundary), vacuous `if let Ok(spec)` adapter tests (pre-existing pattern).
- Artifacts: design D5 Revision 1; MODIFIED deltas for `agent-adapter` (SpawnSpec carries `prompt`), `post-session-runner` (EOF marker only for launched PTYs) and `obsidian-context-injection` (preamble rides `SpawnSpec.prompt`); `deliver_launch` wording replaced by `prepare_launch` / `send_launch`; walks 3.8 and 3.9 added.

## Group 2: review follow-ups

- Red (test-writer): new locked file `src-tauri/src/pty_launch_prompt_ps_tests.rs` (`ab295d9d`): 3 fail on assertions, the `None`-prompt case passes.
- Green (Builder): `resolve_launch_prompt` refuses non-POSIX kinds on unix (`InvalidInput`); `ps_quote` / `ps_maybe_quote` handle U+2018–U+201B via `is_ps_single_quote`; comments and docs updated. Deviation: `#[allow(dead_code)]` on `PromptRef` (with WHY): on unix `Inline` is only built by tests, on non-unix `File` is never built.
- Re-review skipped: the fixes are small, test-covered, and follow the exact remedy the reviewers named.

### Gates (2026-11-23, after group 2)

| Gate | Result | Detail |
|------|--------|--------|
| 1. Compile | PASS | `cargo clippy --all-targets -- -D warnings`: no issues |
| 2. Tests | PASS | `cargo test`: `1236 passed; 0 failed; 1 ignored` |
| 2. Red locks | PASS | 2 locked files unchanged |
| 3. Format | PASS | `cargo fmt --check` ok |
| 6a / 6c | PASS | no placeholders; gitleaks clean |
| 6b | WARN | `#[allow(clippy::too_many_arguments)]` (pinned signature) and `#[allow(dead_code)]` on `PromptRef` (cfg-split variants), both justified above |

## Manual walks (2026-11-23, installed .deb)

- 3.3–3.6: pass (user). FRI-601 walk text, `--settings=` message, Linear multi-line body with `!`, Codex worktree prompt all reached the agent intact.
- 3.7 first half: pass. `launch-prompts` replaced by a file, approve → toast "Could not start the terminal → Not a directory (os error 20)", GUI children unchanged (5 before, 5 after), session not live. The worktree of the approved session stays, by design: the session row keeps its worktree and the requeued prompt for a retry. Second half pass (user): after restoring the dir, reopening the tab delivered the prompt. The walk's `pgrep -P $(pgrep -x nergal)` failed because several processes match; fixed to `pgrep -xo`.
- 3.8: pass. One footer per launched PTY across agent exit and shell exit, marker drained, no ERROR in the post-session log.
- 3.9: pass after correcting the walk. Early close leaves `launch-prompts/` empty. The footer is written because the launch command lands milliseconds after the tab opens; the pre-launch close is covered by the `LaunchRollback` tests.
- Out of scope, pre-existing (not touched by this change): the session log is one file per workspace (`obsidian/channels.rs:60`), so activity lines and footers carry no session name and land under the last open block; the MOC skip rule then reads another session's activity; no MOC written for 9 drained markers without a log line saying why. User decision: no patch now; recorded in the vault note FRI-467 (Obsidian intelligence layer), which will redefine the session log and MOCs.
