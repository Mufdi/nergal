The review is below. The plan does fix the line-editor bug it describes, but I found five issues that block approval.

1. **Critical: an untrusted prompt can still inject options into the `claude` command line (Design D1, Spec).** The plan stops the shell from parsing the prompt, but the whole prompt still arrives as one argument to `claude`. If it starts with `-`, `claude` reads it as an option rather than as the prompt. The `claude_code/adapter.rs:223-225` code being changed puts it after the other args with no `--` in front.
   - A message relayed through `request_session_resume`, a `create_worktree_session` prompt or a deep-link prompt can be exactly `--settings={"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"…"}]}]}}`.
   - Claude Code's option parser accepts the `--option=value` form, so the hook command would run.
   - That is the same outcome as today's shell injection (attacker-chosen commands run on the user's machine), reached without the shell.
   - Fix: put `--` before the prompt, after checking that `claude` stops reading options at `--` and also exercising it with `--resume <id>`. If it does not, refuse or prefix prompts that start with `-`.
   - Add a spec scenario where the prompt is `--settings={…}` and the agent gets it as text, not as an option.

2. **High: the plan keeps the same injection path open for Codex and OpenCode, and an MCP caller can reach it (Non-Goals, D4, Risks "They are not in use").** I am reopening a settled decision here because it is unsafe.
   - `codex/adapter.rs:198-202` and `opencode/adapter.rs:218` pass `fold_prompt_preamble(injected_context, initial_prompt)` as an inline argument. That text includes the initial prompt plus the pinned-note and bound Linear/ClickUp task text, so it goes through the same quoted inline path the bug abuses.
   - `create_worktree_session` lets the caller pick the agent (`mcp/worktree_sessions.rs:83`, checked at `:314`, used at `:916`).
   - On this machine, `codex`, `opencode` and `pi` are all installed (checked with `command -v`). The claim "they are not in use" is not true here.
   - An agent influenced by a malicious issue body can ask for `agent: "codex"` with a crafted prompt. The person approving sees normal-looking text.
   - Fix (the plan itself calls this a one-line fix): set `prompt: fold_prompt_preamble(...)` in those two adapters instead of pushing it into `args`. This changes no behaviour for those agents; it is a security fix, not new feature work, so it fits the freeze. Pi should be checked the same way.
   - The minimum alternative: `start_claude_session` refuses any non-`None` inline prompt on a POSIX shell, so the frozen adapters fail closed.

3. **High: the fail-closed path (D2) leaves a dead shell that later launches reuse.** By the time the prompt file is written, `spawn_pty` (`pty.rs:804`) has already created the shell, and `reserve_session_pty` (`pty.rs:680-682`) has already recorded it and marked the session live and `running`.
   - The plan says to fail "like an `adapter.spawn` failure (`pty.rs:943`)". That path just returns `Err` and never calls `release_session_pty`, never kills the instance, and never clears the live mark.
   - Result: the frontend gets an error and no `pty_id`. The next `start_claude_session` for that session gets the old shell's id back (`pty.rs:677-678`) and shows a plain shell with no agent.
   - FRI-601's liveness tracking reports the session as live, and the queued prompt (already removed at `:915`) is lost without notice.
   - Fix: D2 must release the reservation, kill the shell and mark the session dead, and should put the prompt back in the queue or report the failure to the caller. The same fix should go on the existing `adapter.spawn` error path. Add a scenario: after a write failure, a second start creates a new shell.

4. **Medium: when the prompt file is missing, `claude` starts with an empty argument (D1, Spec).** The command is `cat …; rm …` inside `"$(…)"`, so a missing or unreadable file gives `claude … ""` plus a `cat` error on screen, not an aborted launch.
   - This happens if the line runs twice (the user presses Up then Enter, since the line is in history unless `HISTCONTROL=ignorespace` or `HIST_IGNORE_SPACE` is set), or if a sweep runs first.
   - The spec only defines "absent prompt adds no argument" on the Rust side. Decide what should happen (for example `command cat … || exit`, or accept it) and add a scenario.

5. **Medium: the plan claims "never readable by other local users", which is false (Design Goals, Spec "0600").** Once `claude` starts, the prompt is in its argument list, which other local users can read through `/proc/<pid>/cmdline` unless `/proc` is mounted with `hidepid`. That was already true before this change, and the 0600 file does not change it.
   - Either drop the goal or state it honestly: the 0600 file protects the prompt only between the write and the launch; afterwards it is as exposed as before.
   - Leave it as is and the spec claims a confidentiality property the system does not have.

6. **Medium: the sweep test would delete the running app's files (tasks 1.1 and 1.3, implementation `clear_stale_launch_prompts()`).** The sweep has no directory parameter and works on the real `ipc_dir()/launch-prompts`.
   - On the dev machine the installed Nergal is running, so a test calling it can delete that app's in-flight prompt files, and it creates real directories under `/run/user/<uid>`.
   - Fix: split it into `clear_stale_launch_prompts_in(dir: &Path)` and test only that, against a `tempfile` directory. Do the same for `launch_prompt_dir`.

7. **Medium: the regression test setup contradicts itself (task 1.1).** It says "`--noprofile --norc -i`, rc with `bind Space:magic-space`", but `--norc` stops bash from reading any rc file.
   - It needs `--rcfile <tmp>`.
   - It needs `HISTFILE=/dev/null` (or a temp file) so the test does not write to the developer's real history.
   - It should send the command in one write to the PTY, as `pty.rs:977` does.
   - It should wait for a sentinel file written by the fake agent, not for the prompt file to disappear. Otherwise a broken quote that hangs the shell at a continuation prompt is only noticed as a timeout.
   - The zsh version is skipped in CI (Ubuntu runners have no zsh), so only bash is checked there. Say so in Risks.

8. **Low: the "unwritable dir" scenario depends on file permissions, which root ignores.** If CI runs as root or in a container, a `0500` directory is still writable and the test passes for the wrong reason. Use a path whose parent is a regular file, or a directory that does not exist.

9. **Low: the file name comes from `session_id` without any check (implementation `write_launch_prompt_file(dir, stem, …)`).** `pty_id` embeds `session_id`. Session ids are made by Nergal today, but the function should reject any stem containing `/`, `..`, `'` or a control character, so the "path has no `!`" and "cannot escape the folder" claims hold by construction rather than by assumption.

10. **Low: the reason given for rejecting the environment-variable alternative is wrong (D1 (a)).** "The shell spawns before `start_claude_session` consumes the queued prompt" is not accurate: `spawn_pty` (`:804`) and the `pending_prompts.remove` (`:915`) both happen inside `start_claude_session` and could be reordered. The rejection still holds because the variable would be inherited by every later command in the tab; reword the reason.

11. **Low: the "tmpfs, cleared at logout" note in D3 is not always true.** When `/run/user/<uid>` is unavailable, `ipc_dir()` falls back to `~/.local/share/nergal/ipc/`, which is on disk. Files left by failed launches (for example a failed startup prelude) then stay there for the whole app session, not until logout. Either acknowledge it, or also delete the file from Rust once the agent's `SessionStart` hook arrives or after a timeout.

12. **Low: the deep-link and Linear/ClickUp manual checks are not detailed enough (tasks 2.3 and 2.4).** Add one manual walk with a prompt of the form `--settings=…` (after fixing #1), and one through `create_worktree_session` with `agent: "codex"` (after fixing #2).

The plan is easy to roll back: a plain revert leaves only an empty `launch-prompts` directory behind.

VERDICT: REVISE
