# Implementation Plan: launch-prompt-via-file

> Grounded in current codebase, symbols verified 2026-11-23. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- `SpawnSpec { binary: PathBuf, args: Vec<String>, env: HashMap<String,String> }` at `src-tauri/src/agents/mod.rs:388`. It is built in exactly four places: `agents/claude_code/adapter.rs:231`, `agents/pi/adapter.rs:177`, `agents/opencode/adapter.rs:225`, `agents/codex/adapter.rs:205`.
- Claude Code pushes `ctx.initial_prompt` (non-empty only) as the last element of `args` at `agents/claude_code/adapter.rs:223-225`, after `--append-system-prompt-file <path>` (`:210-217`). Its test `spawn_appends_initial_prompt_as_positional_arg` (`adapter.rs:499`) asserts `spec.args == ["implement auth"]` and that an empty prompt leaves `args` empty.
- Codex pushes `fold_prompt_preamble(injected_context, initial_prompt)` as a positional at `agents/codex/adapter.rs:198-202`, only when `resume_from` is `None`. OpenCode pushes `--prompt` then the same folded text at `agents/opencode/adapter.rs:217-222`, only when `resume_from` is `None`. Pi pushes no prompt (`agents/pi/adapter.rs`).
- `fold_prompt_preamble` at `agents/mod.rs:373` returns `None` when both inputs are empty.
- `build_launch_command(kind, cwd, prelude, binary, args) -> String` at `src-tauri/src/pty.rs:349`. Posix branch: `" cd '<cwd>' && [prelude && ]<bin> <args…>\n"`, args through `posix_maybe_quote` (`pty.rs:421`), cwd through `posix_quote` (`pty.rs:415`). PowerShell ends with `\r` and uses `ps_maybe_quote`; Cmd ends with `\r` and uses `cmd_maybe_quote`.
- Its only production caller is `start_claude_session` at `pty.rs:972`, right after `kind` is derived from `config::shell_kind` (`pty.rs:966-971`). The command is written to the PTY writer at `pty.rs:974-980`. Four inline tests call it in `mod tests` at `pty.rs:2128-2187`.
- `start_claude_session` (`pty.rs:761`): builds `pty_id = "pty-<session_id>-<millis>"` (`pty.rs:787-794`); reserves with `reserve_session_pty` (`pty.rs:796`), which inserts `session_ptys[sid]`, calls `mark_live` and records `"running"` (`pty.rs:670-686`); spawns the shell with `spawn_pty` (`pty.rs:804`), rolling back with `release_session_pty` only when `spawn_pty` fails (`pty.rs:817`); consumes the queued prompt with `pending_prompts.remove(&session_id)` (`pty.rs:915-919`); calls `adapter.spawn` and returns `Err` on failure with no rollback (`pty.rs:943`).
- `PtyManager` fields end with `pending_prompts: Mutex<HashMap<String,String>>` (`pty.rs:132`); `PtyManager::new()` (`pty.rs:182`) is used directly by tests (`pty_liveness_tests.rs`), with `AgentRuntimeState::bootstrap()`.
- The reader thread's agent EOF branch (`pty.rs:573-582`) calls `finalize_session_obsidian` then `on_agent_pty_eof`. `claim_finalization` (`obsidian/post_session.rs:38-43`) is a once-per-process set keyed by session id.
- After `kind`, the launch block only logs on failure (`start_event_pump` warn, id poller): no `Err` after the PTY write at `pty.rs:974-980`, except the lock and write `?` on that write itself.
- `release_session_pty` (`pty.rs:689`) removes the reservation only while `pty_id` owns it, then `mark_dead_if`. It does not remove the PTY instance.
- `kill_session_pty_inner` (`pty.rs:1582`) kills aux shells, removes the reservation with `mark_dead_if`, removes the instance under `instances` and drops it with `drop_off_main_thread`, then deletes `spawn_context_file(session_id)`.
- `queue_session_prompt` (`pty.rs:1556`) is the only writer of `pending_prompts` (`PtyManager.pending_prompts: Mutex<HashMap<String,String>>`, `pty.rs:132`). Callers: `mcp/worktree_sessions.rs:801`, `linear/mod.rs:608`, `clickup/mod.rs:990`, and the frontend deep-link flow (registered at `lib.rs:521`).
- `platform::ipc_dir()` (`src-tauri/src/platform/mod.rs:68-69`) is `#[cfg(unix)]` only. It returns `/run/user/<uid>/nergal/`, or a home fallback, and enforces `0700` via `ensure_ipc_dir_0700` (`platform/mod.rs:143`).
- `clear_stale_spawn_context()` (`agents/mod.rs:360`) deletes every file in the spawn-context dir. It runs once at startup at `lib.rs:1149`.
- `config::shell_kind` (`config.rs:49`) maps any non-PowerShell, non-cmd shell (bash, zsh, fish, sh) to `ShellKind::Posix`.
- `tempfile = "3"` and `portable-pty = "0.9"` are already dependencies (`src-tauri/Cargo.toml:162`, `:16`).
- Binary behavior, verified 2026-11-23: `claude` 2.1.330 reads text after `--` as the prompt (`-- --version`, `-- --settings={…}`), and parses `--resume <id>` placed before `--`. `codex` 0.142 reads `-- --version` as the prompt. `opencode` 2.2.7 reads `--prompt=--version` as the prompt value.
- Reproduction, 2026-11-23: clean `bash --norc -i` and `zsh -f -i` deliver multi-line prompts intact. With `bind Space:magic-space` (bash) or `bindkey ' ' magic-space` (zsh), the inline form corrupts `!!` and `!#` on continuation lines.

## Execution order

1. `agents/mod.rs`: `LaunchPrompt`, `PromptAttach`, `SpawnSpec.prompt`; prompt-dir helpers with an explicit-dir variant for tests; the startup sweep.
2. Adapters: Claude Code, Codex and OpenCode move the prompt into `SpawnSpec.prompt`; Pi sets `None`.
3. `pty.rs`: `resolve_launch_prompt`, the new `build_launch_command` parameter, the launch rollback, and the prompt-file delete in `kill_session_pty_inner`.
4. `lib.rs:1149`: call the new sweep next to `clear_stale_spawn_context()`.

## Plan

### `agents/mod.rs`
- `pub enum PromptAttach { AfterDoubleDash, Flag(&'static str) }` and `pub struct LaunchPrompt { pub text: String, pub attach: PromptAttach }`.
- `SpawnSpec.prompt: Option<LaunchPrompt>`: free-text launch prompt, never part of `args`.
- `#[cfg(unix)] pub fn launch_prompt_dir() -> io::Result<PathBuf>`: `platform::ipc_dir()?.join("launch-prompts")`, created with mode `0700` only when missing; an existing directory keeps its mode. Production only; tests never call it.
- `#[cfg(unix)] pub fn write_launch_prompt_file(dir: &Path, stem: &str, text: &str) -> io::Result<PathBuf>`: path from `launch_prompt_file_path` (`dir/<stem>.txt`); remove any existing file at that path; create with `create_new(true)` and mode `0600` (`std::os::unix::fs::OpenOptionsExt`); write `text` exactly.
- `pub fn launch_prompt_file_path(dir: &Path, stem: &str) -> io::Result<PathBuf>`: rejects a `stem` that is empty or holds `/`, `..`, `'`, whitespace or a control character (`io::ErrorKind::InvalidInput`); shared by the writer and both deletes.
- `pub fn clear_launch_prompts_in(dir: &Path)`: deletes every file in `dir`; missing dir is a no-op.
- `pub fn clear_stale_launch_prompts()`: `cfg(unix)` → `launch_prompt_dir()` then `clear_launch_prompts_in`; `cfg(not(unix))` → no-op.

### Adapters
- `claude_code/adapter.rs:223-225`: no prompt in `args`; set `prompt: Some(LaunchPrompt { text, attach: AfterDoubleDash })` for a non-empty `initial_prompt`. Rewrite the comment above it to the new WHY (the shell never parses the prompt, and `--` keeps it from being read as an option).
- `codex/adapter.rs:198-202`: same resume rule; `prompt` = folded text with `AfterDoubleDash`; nothing pushed into `args`.
- `opencode/adapter.rs:217-222`: same resume rule; `prompt` = folded text with `Flag("--prompt")`; push neither `--prompt` nor the text into `args`.
- `pi/adapter.rs:177`: `prompt: None`.

### `pty.rs`
- `PtyManager.launched_ptys: Mutex<HashSet<String>>` (pty ids whose launch command reached the PTY), initialized in `PtyManager::new()`.
- `pub(crate) enum PromptRef { File(PathBuf), Inline(String) }`.
- `pub(crate) fn normalize_launch_prompt(text: &str) -> Option<String>`: drop `\0`, trim trailing `\n` / `\r`; empty → `None`.
- `build_launch_command(kind, cwd, prelude, binary, args, prompt: Option<(&PromptRef, &PromptAttach)>)`:
  - Posix + `File(p)`: after the prelude, insert `test -r <posix_quote(p)> && ` before the binary. After the args, append ` -- ` (AfterDoubleDash) or ` <flag>=` (Flag, no space before the reference), then `"$(command cat -- <posix_quote(p)>; command rm -f -- <posix_quote(p)>)"`. Exactly one `\n`, at the end, when there is no prelude.
  - Posix + `Inline(t)`: ` -- ` + `posix_maybe_quote(t)`, or ` ` + `posix_maybe_quote("<flag>=" + t)`. Reached only on non-unix hosts.
  - PowerShell / Cmd + `Inline(t)`: same tokens with `ps_maybe_quote` / `cmd_maybe_quote`.
  - `None`: nothing appended; output byte-identical to today.
- `pub(crate) fn resolve_launch_prompt(kind, text: &str, dir: io::Result<PathBuf>, stem: &str) -> io::Result<PromptRef>`: Posix on `cfg(unix)` → `dir?` then `write_launch_prompt_file` → `File`; a non-POSIX kind on `cfg(unix)` → `Err(InvalidInput)` (design D5 Revision 1); every kind on non-unix → `Inline`.
- `ps_quote` doubles `'` and U+2018–U+201B; `ps_maybe_quote` also quotes any text holding one of them.
- Delivery runs in two steps so no file I/O happens under the locks: `prepare_launch` (normalize, resolve, write the file, build the command) runs before any lock and the caller reports the file to the guard; `send_launch` (`write_all`, `flush`) runs under `session_ptys` → `instances` after the reservation re-check, then `rollback.disarm()`. `send_launch` never deletes the file: on failure the guard's rollback does.
- `prepare_launch` / `send_launch` signatures: see `handoff/evidence.md` § Pinned API (they replace the earlier `deliver_launch` draft).
- `struct LaunchRollback<'a> { state, agents, session_id, pty_id, prompt: Option<String>, file: Option<PathBuf>, armed: bool }` with `set_prompt`, `set_file`, `disarm`. `disarm()` inserts `pty_id` into `launched_ptys`. `Drop` when armed, in order, never panicking (`lock().ok()` or `into_inner`, I/O errors ignored): remove `session_ptys[sid]` only if it equals `pty_id`, then `agents.mark_dead_if`; remove `instances[pty_id]` and `drop_off_main_thread`; delete `file`; `pending_prompts.entry(sid).or_insert(prompt)`; `tracing::warn!` with the session id.
- `pub(crate) fn take_launched(state: &PtyManager, pty_id: &str) -> bool`: removes the id, returns whether it was present.
- Reader thread, agent branch (`pty.rs:573`): `if launched { finalize_session_obsidian(...) }` where `launched = eof_app.try_state::<PtyManager>().map(|m| take_launched(&m, &eof_pty_id)).unwrap_or(false)`. `on_agent_pty_eof` still runs.
- `start_claude_session`:
  - right after the reservation returns `None` (`pty.rs:796`), arm `LaunchRollback`;
  - `spawn_pty` failure: drop the explicit `release_session_pty` call (`pty.rs:817`); the guard covers it;
  - `ReservationOutcome::Gone`: keep the guard armed; its idempotent rollback runs when the function returns `Err` (never `disarm()` here: that marks the PTY launched);
  - after `pending_prompts.remove` (`pty.rs:915`): `rollback.set_prompt(initial_prompt.clone())`;
  - replace the build + write block (`pty.rs:972-980`) with `deliver_launch`, holding `session_ptys` and, nested inside it, `instances` (the existing order, `kill_session_aux_shells` at `pty.rs:1342-1343`) through `write_all` + `flush`; the writer is `None` when `session_ptys[sid]` no longer equals `pty_id` or the instance is gone. Never take `session_ptys` inside `instances`, `launch_prompt_dir_or_unsupported()` and `&pty_id`; `rollback.set_file` when `prepare_launch` wrote a file;
  - on success `rollback.disarm()`; the rest of the function stays.
- `pub(crate) fn discard_launch_prompt(dir: io::Result<PathBuf>, pty_id: &str)`: deletes `launch_prompt_file_path(dir, pty_id)` when `dir` is `Ok`; ignores errors. `kill_session_pty_inner` (`pty.rs:1582`) calls it with the prompt dir when it removes a `pty_id` (`cfg(unix)`). Tests call the helper with a temp dir, never the kill path.

### `lib.rs`
- `lib.rs:1149`: add `crate::agents::clear_stale_launch_prompts();` next to the existing sweep.

## Per-phase risk

- [Inline adapter and `build_launch_command` tests pin the old contract] → The test-writer rewrites `spawn_appends_initial_prompt_as_positional_arg` and the Codex/OpenCode prompt tests to assert `spec.prompt`, and adds the `None` argument to the four `build_launch_command` tests (expected strings unchanged). Inline tests in production files cannot be hash-locked; evidence records the edits.
- [Tests delete the installed app's prompt files] → Tests only call the explicit-dir functions (`write_launch_prompt_file`, `clear_launch_prompts_in`) against a `tempfile` dir; never `launch_prompt_dir()` or `clear_stale_launch_prompts()`.
- [Rollback tests need a `PtyManager` and `AgentRuntimeState`] → `LaunchRollback` and `deliver_launch` take plain references; test them the way `pty_liveness_tests.rs` tests the reservation helpers. The "unwritable dir" case uses a dir path whose parent is a regular file (root ignores permission bits). Killing a real PTY instance needs an `AppHandle`, so instance removal is covered by the shared `drop_off_main_thread` path plus manual walk 2.7, and the tests say so.
- [The guard holds references across `.await`] → `PtyManager` and `AgentRuntimeState` are `Sync`, so the future stays `Send`; `cargo check` proves it.
- [Interactive-shell test is flaky or touches real history] → bash `--noprofile --rcfile <tmp rc> -i` with `bind Space:magic-space` and `HISTFILE=/dev/null` in the rc; the whole command in one PTY write (as `pty.rs:977`); a fake agent (`/bin/sh -c 'printf %s "$1" > OUT; : > DONE' sh`) writes a sentinel; bounded poll on the sentinel (10 s). Skip when bash is missing.
- [Cross-platform CI] → Every new unix API is `#[cfg(unix)]` with a `cfg(not(unix))` stub or an `Inline` path; `windows-check` and `macos-cross-check` must stay green.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check` (format only touched files with `rustfmt --edition 2024 <files>`).
- `npx tsc --noEmit` (no frontend change expected; run as a guard).
- Manual walks on the installed `.deb`: see `tasks.md` group 2.
