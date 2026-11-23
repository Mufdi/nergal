## 1. Launch prompt off the shell parser

- [x] 1.1 Test (Red): every scenario of the three requirements in `specs/agent-adapter/spec.md`, in a new `#[path]` test module `src-tauri/src/pty_launch_prompt_tests.rs`:
  - real interactive bash through a PTY (`--noprofile --rcfile <tmp rc> -i`, rc with `bind Space:magic-space` and `HISTFILE=/dev/null`; command sent in one write; fake agent writes the last arg and a sentinel; bounded poll). The file form delivers the spec prompt byte-identical; the inline form corrupts it, which proves the test reproduces the bug. Skipped when bash is missing. A zsh variant (`bindkey ' ' magic-space`) is skipped when zsh is missing.
  - unit tests: the typed command never holds the prompt text and has one trailing newline; `--` before the reference for `AfterDoubleDash`; `--prompt=` glued to the reference for `Flag`; prompt file mode `0600` with exact content, deleted after the shell runs; a replay with the file gone does not start the fake agent; a bad stem is rejected; no prompt adds nothing; `clear_launch_prompts_in` empties a temp dir.
  - `prepare_launch` / `send_launch` tests: unwritable dir (parent is a regular file) → `Err` and no file; no prompt → no file; missing writer → `Err`; failing writer → `Err`; a `Vec<u8>` writer receives exactly the command. The guard's rollback deletes the file on any failure.
  - `LaunchRollback` tests: an armed guard dropped after `reserve_session_pty` leaves no reservation and no live mark, requeues the prompt, keeps a newer queued prompt, and deletes the recorded file; a disarmed guard changes nothing and marks the pty id launched; `take_launched` returns true once then false, and false for an id that never disarmed (rollback, failed spawn); a `Gone`-style rollback (reservation already taken by a kill, instance already gone) is a no-op that leaves the other owner's reservation alone and marks nothing launched; after a rollback the reservation is free (`reserve_session_pty` returns `None`).
  - normalization: `a\0b\n\n` → `ab`; only newlines → no prompt.
  - `launch_prompt_file_path` rejects `a/b`, `..`, `a'b`, `a b` and an empty stem;
  - `discard_launch_prompt` deletes the PTY's prompt file in a temp dir and ignores a missing file or an `Err` dir.
- [x] 1.2 Test (Red): update the inline tests that pin the old contract. Claude Code `spawn_appends_initial_prompt_as_positional_arg`, and the Codex and OpenCode prompt tests, assert `spec.prompt` (text and attach) and that `args` holds no prompt. The four `build_launch_command` tests in `pty.rs` pass `None` and keep their expected strings.
- [x] 1.3 Add `PromptAttach`, `LaunchPrompt`, `SpawnSpec.prompt`, `launch_prompt_dir`, `launch_prompt_file_path`, `write_launch_prompt_file`, `clear_launch_prompts_in` and `clear_stale_launch_prompts` in `src-tauri/src/agents/mod.rs` (unix APIs `#[cfg(unix)]`, non-unix stub for the sweep).
- [x] 1.4 Move the launch prompt into `SpawnSpec.prompt` in the Claude Code (`AfterDoubleDash`), Codex (`AfterDoubleDash`) and OpenCode (`Flag("--prompt")`) adapters; `prompt: None` in Pi.
- [x] 1.5 Add `PromptRef`, `normalize_launch_prompt`, `resolve_launch_prompt`, `prepare_launch`, `send_launch`, `LaunchRollback`, `launched_ptys`, `take_launched`, `discard_launch_prompt`, and the new `build_launch_command` parameter in `src-tauri/src/pty.rs`. Arm the guard right after the reservation in `start_claude_session` and disarm it after the PTY write. Finalize in Obsidian at EOF only for launched pty ids. Call `discard_launch_prompt` in `kill_session_pty_inner`.
- [x] 1.6 Call `clear_stale_launch_prompts()` at startup next to `clear_stale_spawn_context()` in `src-tauri/src/lib.rs`.

## 2. Review follow-ups (B5, confirmed by the user 2026-11-23)

- [x] 2.1 Test (Red): PowerShell on a unix host refuses a launch prompt (`prepare_launch` returns `Err`, no file, no command); `ps_quote` doubles U+2018–U+201B and `ps_maybe_quote` quotes any text holding one, in a new `#[path]` test module.
- [x] 2.2 `resolve_launch_prompt` refuses non-POSIX kinds on `cfg(unix)`; `ps_quote` / `ps_maybe_quote` escape the typographic quotes (design D5 Revision 1).
- [x] 2.3 Fix stale comments: the `push_prompt` doc (a `File` renders on every shell kind), and the positional-prompt wording in `agents/claude_code/adapter.rs` and `pty.rs` near the prompt consumption.
- [x] 2.4 Docs: list `launch-prompts/` under the per-user IPC dir in `CLAUDE.md` (Naming) and `docs/hooks.md`.

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit`.
- [x] 3.2 `~/.claude/skills/work/scripts/work-gates.sh main launch-prompt-via-file` passes.
- [x] 3.3 Manual walk on the installed `.deb`: approve a `request_session_resume` with the exact FRI-601 walk text. The target resumes, the agent receives the full text, and the shell shows no `parse error`.
- [x] 3.4 Manual walk: approve a `request_session_resume` whose message is `--settings={"model":"x"} reply OK`. The agent receives it as its first turn, and no setting changes.
- [x] 3.5 Manual walk: start a session from a Linear issue (spawn-with-issue) whose body has several lines and a `!`. The first turn shows the full body.
- [x] 3.6 Manual walk: approve a `create_worktree_session` with `agent: "codex"` and a multi-line prompt with a `!`. Codex starts with the full prompt.
- [x] 3.7 Manual walk: replace the prompt directory with a regular file (`rmdir /run/user/$UID/nergal/launch-prompts && touch /run/user/$UID/nergal/launch-prompts`), save `pgrep -P "$(pgrep -xo nergal)" -a`, approve a resume with a message, wait about 1 s and list again. The start fails with an error and the child list matches the saved one. Then `rm /run/user/$UID/nergal/launch-prompts` and reopen the tab: the message arrives.
- [x] 3.8 Manual walk: in a workspace with a vault, start a session, exit the agent and then the shell. The session footer appears in its note and `pending-mocs/<session_id>.json` is written and drained.
- [x] 3.9 Manual walk: open a new session tab with a queued prompt and close it right away. `/run/user/$UID/nergal/launch-prompts/` holds no file. The launch command is written milliseconds after the tab opens, so a manual close always finds a launched PTY and its footer is written by design; the close before the launch command is covered by the `LaunchRollback` tests in task 1.1.
