## Context

Nergal starts an agent by typing one command line into the user's interactive shell inside the PTY (`build_launch_command`, `src-tauri/src/pty.rs:349`). The adapters append the launch prompt to that line as an argument: Claude Code as the last positional (`src-tauri/src/agents/claude_code/adapter.rs:223`), Codex as the positional `[PROMPT]` (`agents/codex/adapter.rs:198-202`), OpenCode as `--prompt <text>` (`agents/opencode/adapter.rs:217-222`). Pi takes no launch prompt. The prompt comes from MCP `request_session_resume` and `create_worktree_session`, the `nergal://session/new` deep link, and Linear or ClickUp spawn-with-issue. `create_worktree_session` lets the calling agent pick the agent, so Codex and OpenCode are reachable even under the focus freeze.

The POSIX quoting (`posix_quote`, `pty.rs:415`) is correct. The failure comes from the line editor, not the parser. Reproduced on 2026-11-23 with a PTY probe:

- The user's zsh binds Space to `magic-space`. That widget runs history expansion on the edit buffer each time a space arrives.
- On a continuation line, the buffer holds only that line. The widget does not see the single quote opened on an earlier line, so `!!`, `!#` and `!-` expand inside the prompt.
- A clean `zsh -f -i` and a clean `bash -i` pass. Adding `bindkey ' ' magic-space` (zsh) or `bind Space:magic-space` (bash) makes them fail the same way.
- The FRI-601 walk text had `<!--` on line 3. `!-` inserted an old history line with an apostrophe and backticks. The quote closed early and zsh reported `parse error near ()`.

`!#` repeats the current line so far, so a crafted prompt can break the quote on purpose. The prompt is often text the user did not write, so this is a command-injection path into the user's shell. A second path exists without the shell: a prompt that starts with `-` reaches the agent as an option. For Claude Code, `--settings={…}` can register a hook command.

## Goals / Non-Goals

**Goals:**
- The prompt reaches the agent unchanged, whatever the user's shell configuration.
- The prompt text never passes through the shell's line editor or parser.
- The agent never reads the prompt as one of its options.
- A failed launch leaves no shell, reservation or liveness behind, and keeps the prompt for a retry.
- The prompt file is `0600` and exists only between the write and the shell's read.

**Non-Goals:**
- Hiding the prompt after launch. Once the agent runs, the prompt is in its argument list, which other local users can read through `/proc/<pid>/cmdline` unless `/proc` uses `hidepid`. That exposure exists today and this change does not alter it.
- The frozen Windows shells (PowerShell, cmd). They keep the inline argument.
- The user's own `startup_command` prelude. The user writes it, so it keeps its typed form.
- Prompts larger than the kernel's single-argument limit (128 KiB on Linux). The limit is unchanged.

## Decisions

### D1. The prompt rides a private file, and the typed line reads it

`SpawnSpec` gets `prompt: Option<LaunchPrompt>`, where `LaunchPrompt { text, attach }` and `attach` is `AfterDoubleDash` or `Flag("--prompt")`. For a POSIX shell, the PTY layer writes the text to `<ipc_dir>/launch-prompts/<pty_id>.txt` with mode `0600`. `platform::ipc_dir()` (`src-tauri/src/platform/mod.rs:69`) is a per-user `0700` directory. The typed line becomes:

` cd '<cwd>' && [<prelude> && ]test -r '<file>' && <bin> <args…> <attach>"$(command cat -- '<file>'; command rm -f -- '<file>')"`

- Double-quoted command substitution gives one argument, with no word splitting, no globbing and no re-parsing of the output.
- The line editor sees only the file path, on the first and only line.
- `command` skips user aliases and functions. A `cat` alias to `bat`, or an `rm` alias to `rm -i`, cannot change the result.
- `test -r` stops the line when the file is gone. A replay from history then does nothing, instead of starting the agent with an empty prompt.
- The shell deletes the file before the agent starts.
- The file stem is the `pty_id`. `launch_prompt_file_path` rejects a stem with `/`, `..`, `'`, whitespace or a control character, and both the writer and the deletes go through it, so every path stays inside the directory and inside its quotes by construction.

Alternatives considered:
- **(a) An environment variable read by the typed line.** Rejected. The shell would keep the variable for every later command in that tab, and the agent's own children would inherit it.
- **(b) Bracketed paste of the prompt into the agent's TUI after boot.** Rejected. It brings back the REPL timing race. `linear-agent-integration` and `clickup-agent-integration` require that the initial prompt submits on spawn without that race.
- **(c) Exec the agent directly instead of typing into the shell.** Rejected. Dropping to the shell when the agent exits depends on the typed launch. It is a large architectural change for a delivery bug.
- **(d) Disable history expansion first (`setopt nobanghist`, `set +H`) or escape `!`.** Rejected. It patches one known expansion per shell. It misses other line-editor widgets and plugins, and it changes the user's shell state for the rest of the tab.

### D2. The prompt is never read as an option

- Claude Code and Codex: the adapter pushes nothing for the prompt into `args`. The PTY layer renders ` -- <ref>`. Verified 2026-11-23: `claude` 2.1.330 treats `--version` and `--settings={…}` after `--` as prompt text, and still parses `--resume <id>` before `--`. Codex 0.142 (clap) treats `-- --version` as the prompt.
- OpenCode: the PTY layer renders ` --prompt=<ref>` as one word. Verified 2026-11-23: OpenCode 2.2.7 started its TUI with `--prompt=--version` instead of printing its version.
- Codex and OpenCode keep their current rule: no prompt on resume.
- `resume_pick` renders a bare `--resume` (`claude_code/adapter.rs:85`). With a prompt, the line becomes `--resume -- <ref>`: the picker opens and the prompt is the first turn, where today the prompt silently became the resume search term. No caller queues a prompt for a `resume_pick` start today (the prompt sources create new sessions or resume by id), so this is noted, not tested on the binary.

User decision 2026-11-23: Codex and OpenCode get the same fix despite the focus freeze, because an MCP caller can choose them.

### D3. A failed launch leaves no trace and keeps the prompt

User decision 2026-11-23, after two iprev rounds found new leaks in per-call-site rollback. Invariant: any error after the PTY reservation leaves no reservation, no live mark, no shell, no prompt file and no vault write for that launch, and the prompt goes back to the queue. The activity mode that `reserve_session_pty` recorded (`"running"`) stays; the not-live descriptor ignores it (FRI-601).

The rollback lives in one place, a drop guard:
- `start_claude_session` arms a `LaunchRollback` guard right after `reserve_session_pty` (`pty.rs:796`). Every later `?` or `return Err` drops the guard, and its `Drop` runs the rollback. A panic does too.
- The guard records the consumed prompt as soon as `pending_prompts.remove` runs (`pty.rs:915`), and the prompt file path as soon as it is written.
- `disarm()` runs once the launch command is written to the PTY. After that point, nothing in `start_claude_session` returns an error: the event pump and the id poller only log.
- The startup-kill path (`ReservationOutcome::Gone`, `pty.rs:822-831`) keeps the guard armed. Its rollback is idempotent there: `session_ptys[sid]` no longer holds this `pty_id`, `mark_dead_if` is id-guarded, `instances.remove` finds nothing because the orphan was already extracted, and no file or prompt exists yet. Disarming there would mark the PTY launched and write the vault for a session that never ran.

The rollback, in order:
1. Under `session_ptys`, remove the entry only while it equals this `pty_id`, then `mark_dead_if`.
2. Remove `instances[pty_id]` and drop it off the main thread, as `kill_session_pty_inner` does (`pty.rs:1582`). Aux shells stay.
3. Delete the prompt file, if one was written.
4. `pending_prompts.entry(session_id).or_insert(prompt)`, so a newer prompt wins.

`Drop` never panics: every lock uses `lock().ok()` or `PoisonError::into_inner`, there is no fallible indexing, and I/O errors are ignored. A second panic while unwinding would abort the whole app. The guard also fires when the command future is dropped, for example at app shutdown; the rollback is harmless then.

Lock contract for the delivery:
- The prompt file is written before any lock is taken. Under the locks only the reservation re-check, `write_all`, `flush` and the insert into `launched_ptys` run, because `instances` serializes all terminal I/O (`pty.rs:1355-1359`). When the re-check fails, the file is deleted after the locks are released.
- `disarm()` (the insert into `launched_ptys`) runs inside the lock scope, after `flush`, so a kill cannot reach EOF before the id is marked launched. The order is `session_ptys` → `instances` → `launched_ptys`; the reader thread takes `launched_ptys` alone.
- The `LaunchRollback` guard is declared before the lock guards. On a `?` or a panic, Rust drops the locks first, then the rollback re-takes `session_ptys`. Reversing that order would deadlock.

Vault writes follow the good case, not the failed one. `disarm()` inserts the `pty_id` into `PtyManager.launched_ptys`. The reader thread's agent EOF branch (`pty.rs:573-582`) removes its `pty_id` from that set and calls `finalize_session_obsidian` only when the id was there. A rollback, a `Gone` exit, a kill before delivery and a failed `spawn_pty` therefore never write to the vault, with no race against the EOF thread and nothing left in the set. Without this, the once-per-process claim (`claim_finalization`, `obsidian/post_session.rs:38-43`) would be spent on a session that never ran, and the real end of the session would never be finalized.

The delivery steps are plain functions, `prepare_launch` and `send_launch`, so tests can drive them without Tauri. `prepare_launch` takes the shell kind, the command parts, the prompt, the prompt directory as an `io::Result<PathBuf>` and the stem; `send_launch` takes the prepared command and an optional writer. Delivery is split in two (`prepare_launch` writes the file and builds the command before any lock; `send_launch` writes the command under the locks). `send_launch` never deletes the file: on any failure the guard's rollback deletes it. `start_claude_session` calls it while holding both locks in the existing order, `session_ptys` then `instances` nested inside (the order `kill_session_aux_shells` already uses, `pty.rs:1342-1343`). It passes `None` as the writer when `session_ptys[sid]` no longer equals this `pty_id` or the instance is gone. Both locks stay held through `write_all` and `flush`. A kill must take `session_ptys` to drop the reservation, so it lands strictly before delivery (writer `None`, rollback) or strictly after it (a normal close of a launched shell). The cost is holding `session_ptys` while one short line is written, as `instances` already is today (`pty.rs:975-980`).

Alternative considered: an inner `async fn` for everything after the reservation, with the rollback on its `Err`. Rejected in favor of the guard because the guard also covers panics and early returns added later, and it needs no error type to carry the prompt and file back out.

### D4. Leftover prompt files

`launch_prompt_dir()` creates the directory with mode `0700` only when it is missing, and never changes the mode of an existing directory.

A launch can stop after the write and before the shell reads the file, for example when the prelude fails and `&&` stops the line. Three cleanups cover it:
- `kill_session_pty_inner` deletes the prompt file of the PTY it removes.
- The startup hook that deletes stale spawn-context files (`clear_stale_spawn_context`, `src-tauri/src/agents/mod.rs:360`, called at `src-tauri/src/lib.rs:1149`) also empties the prompt directory.
- `/run/user/<uid>` is a tmpfs, cleared at logout. When it is missing, `ipc_dir()` falls back to `~/.local/share/nergal/ipc/` on disk; the first two cleanups still apply there.

### D5. Non-POSIX shells

Revision 1 (security review, user decision 2026-11-23): PowerShell treats U+2018 to U+201B as single quotes, and `ps_quote` only doubled `'`. A prompt like `’; <command> #` closed the string and ran the command. `pwsh` can be the user's shell on Linux (`config::shell_kind` maps it to PowerShell), so the frozen-Windows premise did not cover it.

- On a unix host, `resolve_launch_prompt` refuses a prompt for any non-POSIX shell kind. The launch fails, the guard rolls back and the prompt goes back to the queue. Alternative rejected: read the file from PowerShell with `Get-Content -Raw`; `pwsh` is not installed here, so it could not be verified.
- On Windows (frozen), PowerShell and cmd keep the inline argument. `ps_quote` doubles all four typographic quotes, and `ps_maybe_quote` always quotes a text that holds one. This change promises nothing else for those shells: PowerShell's parameter binder consumes `--` before an npm `*.ps1` shim sees it, and `cmd_quote` does not escape `"`.

### D6. The prompt is normalized before delivery

Nergal removes NUL bytes and trailing newlines from the prompt before it writes the file, on every shell. A prompt left empty counts as absent: no file, no argument. NUL cannot travel in an argument, and command substitution drops trailing newlines anyway, so normalizing in Rust makes the delivered text predictable and testable.

## Risks / Trade-offs

- [A multi-line `startup_command` splits the typed line] → Pre-existing: the prelude is only trimmed (`pty.rs:953-957`), so its newlines already split today's line. The prompt stays safe either way, because it is never on the typed line. The one-newline scenario applies to launches without a prelude.
- [The shell exits on its own during the startup wait, yet the master write succeeds] → `disarm()` then inserts an id whose EOF already passed, and it stays in `launched_ptys` for the app's lifetime. Accepted: no vault write happens, and the id never repeats.
- [A prompt requeued after a deliberate close is delivered when the user reopens the session] → Accepted under the user's invariant. A deleted session's queued prompt stays in `pending_prompts` until restart; that leak already exists for prompts queued and never launched.
- [A requeued prompt waits until the next successful start] → When a launch keeps failing, the prompt stays queued and the MCP caller is not told. The start error reaches the frontend through the rejected `start_claude_session` call, and Nergal logs a warning with the session id.
- [fish older than 3.4 has no `$(…)` syntax] → Accepted. The typed launch already needs `&&` (fish 3.0+). Current fish supports `$(…)` inside double quotes. bash and zsh are the default shells on Linux.
- [A future `claude`, `codex` or `opencode` release changes how it parses `--` or `--prompt=`] → The unit tests pin the rendered command. The manual walk exercises the real binaries.
- [The interactive-shell test needs bash] → It runs only on unix and skips when bash is missing. CI runs `cargo test` on Ubuntu, where bash is present. The zsh variant skips in CI because the runners have no zsh.

## Migration Plan

No data migration. Rollback is a plain revert: the prompt goes back to an inline argument, and only an empty `launch-prompts` directory remains.
