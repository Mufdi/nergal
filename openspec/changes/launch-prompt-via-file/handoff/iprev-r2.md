1. **Medium: D3 still leaves several failure paths without rollback (D3, implementation `start_claude_session`).** The plan routes two failures through `abort_launch`: `adapter.spawn` and the prompt-file write. Every other `Err` between `reserve_session_pty` (`pty.rs:796`) and the end of the block leaks the same half-started state that round-1 #3 described:
   - `registry.get(&agent_id).ok_or_else(...)?` (`pty.rs:869-872`).
   - `state.instances.lock()?`, `instance.writer.lock()?`, `w.write_all(...)?` and `w.flush()?` (`pty.rs:974-980`).

   The write failure is the worst of these because it is new. The prompt file has already been written by then, so the failure leaves the reservation, the live mark and a shell with no agent. The prompt has been consumed and is lost. The file stays until kill or the next startup, and with the on-disk fallback dir it survives a reboot until then.

   Fix: make the rollback structural rather than per call site. Either move everything after reservation into one inner `fn` whose `Err` always goes through `abort_launch`, or use a drop guard that is disarmed only on success. Change the requirement text from "the prompt file cannot be written, or the adapter cannot build the spawn spec" to "any error after the reservation".

2. **Medium: the rollback finalizes the Obsidian session for a session that never ran (D3, `abort_launch`).** Dropping the `PtyInstance` kills the shell. The reader thread then reaches EOF, and for agent PTYs it calls `finalize_session_obsidian` unconditionally (`pty.rs:573-582`). That function calls `claim_finalization(csid)`, a once-per-process `HashSet` (`obsidian/post_session.rs:39-44`), then writes the footer and the MOC marker.
   - An aborted launch therefore writes a session footer and marker to the vault for a session that never started.
   - When the retry succeeds, the real end of that session is never finalized, because the claim is already taken.

   This contradicts "A failed launch leaves no half-started session". Fix: have `abort_launch` suppress EOF finalization for its `pty_id`. For example, record the id in a "launch-aborted" set that the EOF branch checks, or release the claim afterwards. Add a test assertion for it.

3. **Medium: the two rollback scenarios cannot be run as written, and the wiring itself is untested (spec "A failed launch…", task 1.1).**
   - Both scenarios say "WHEN Nergal starts the session", which means `start_claude_session`. That is a Tauri command needing `AppHandle`, `State` and `SharedDb`, and it resolves the prompt dir from the real `platform::ipc_dir()` through `launch_prompt_dir_or_unsupported()`. A test cannot point it at a "parent is a regular file" path.
   - Task 1.1 tests `abort_launch` in isolation. Nothing then catches the regression this change exists to prevent: an error return in `start_claude_session` that skips `abort_launch`. That is exactly how today's `adapter.spawn` leak happened.
   - Without a real `PtyInstance` in the test, "no PTY instance" is asserted against a map that was empty to begin with.
   - The retry scenario ("new PTY id, agent receives `hello`") has no planned test at all.

   Fix: pull the post-reservation launch preparation into a function that takes the prompt dir (and the writer) as parameters, and test the failure→rollback→retry sequence against it. Otherwise, say explicitly that the retry scenario is checked only by a manual walk, and add that walk to group 2.

4. **Low: a kill during startup leaks the prompt file and loses the prompt (D4).** The kill-path delete only works if the file already exists when the kill runs. Consider this order:
   - A `kill_session_pty_inner` lands during the 5 s ready wait or the 200 ms sleep (`pty.rs:837-840`). It removes the reservation and the instance and deletes a file that does not exist yet.
   - `start_claude_session` then continues and writes the prompt file.
   - It finds no instance (`if let Some(instance)` at `pty.rs:976`) and returns `Ok`.

   The file stays until the next startup, and the prompt is consumed without being delivered. Fix: write the file only after re-checking that the reservation is still held. When the instance is missing at write time, treat it as a failure: delete the file, and requeue or report.

5. **Low: the scenario "command SHALL contain exactly one newline" is false whenever there is a prelude.** `startup_command` is only trimmed (`pty.rs:953-957`), so a multi-line prelude puts its newlines into the typed line. In that case `test -r … && claude …` runs as a separate command no matter how the prelude ends. Either scope the scenario to "no prelude", or reject or flatten multi-line preludes. The latter is a behaviour change for the user's own text, so if chosen it should be a stated decision.

6. **Low: the PowerShell `--` claim in D5 does not hold for npm shims.** On Windows, `claude` and `codex` installed through npm resolve to `*.ps1` shims under PowerShell. PowerShell's parameter binder consumes `--` before the script ever sees it, so an option-like prompt is read as an option again. The scope is frozen, but D5 states that the same attachment protects these shells. Either reword D5 so it promises nothing for PowerShell, or use cmd-style shims.

7. **Low: `resume_pick` plus a prompt changes behaviour without being noted.** `cc_resume_args(Some("resume_pick"))` emits a bare `--resume` (`claude_code/adapter.rs:85`).
   - Today `--resume <prompt>` silently uses the prompt as the resume search term.
   - After this change, `--resume -- <ref>` opens the picker and sends the prompt as the first turn.

   The new behaviour is probably the better one, but it is a change and was not verified on the binary. Either check it or note in D2 that no current caller combines the two.

8. **Low: the "byte-identical" claim has unstated exceptions.** Two cases fall outside it:
   - NUL bytes: bash drops them from command substitution with a warning, and `exec` cannot carry them anyway.
   - A prompt made only of newlines: it passes the non-empty check, but `$(…)` reduces it to `""`, so the agent starts with an empty positional argument.

   Either reject or trim these in Rust before the write, or list both as accepted next to the trailing-newline risk.

9. **Low: a requeued prompt can be delivered much later than expected (D3).** If the failure keeps happening (stem rejected, dir broken), `entry().or_insert` holds the relayed or MCP prompt until the next successful start of that session, which may be a manual restart hours later. Neither the MCP caller nor the user is told. Mention this in Risks, and consider emitting an error toast or event when the launch aborts.

**Round-1 items:** #1 (`--` / `--prompt=` verified on the real binaries), #2 (Codex and OpenCode moved to `SpawnSpec.prompt`), #4 (`test -r` guard), #5 (the cmdline exposure now stated honestly), #6 (explicit-dir functions), #7 (shell test setup), #8, #9, #10, #11 and #12 are all addressed. The security core is sound:
- The prompt no longer passes through the shell parser or line editor.
- The prompt can no longer be read as an option.
- The `0700` owner-checked `ipc_dir` (`platform/mod.rs:143-165`) plus `create_new` closes the symlink and TOCTOU window.

The remaining blockers are #1–#3. They are about the new rollback requirement being only partly implemented, having a side effect, and lacking a test, not about the injection fix itself.

VERDICT: REVISE
