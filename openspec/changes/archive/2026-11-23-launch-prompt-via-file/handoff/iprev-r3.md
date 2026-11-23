1. **Medium: the Obsidian rule uses a list of aborted launches, which is racy and misses two error paths the spec covers (D3 "Gone path disarms", spec "Aborted launch writes nothing to the vault").** The requirement says *any* error after the reservation leaves no vault write. The plan only blocks the vault write when the rollback has put the `pty_id` into `aborted_launches` before the shell's end of file.
   - **`ReservationOutcome::Gone`** (`pty.rs:822-831`) returns `Err` after the reservation. D3 disarms the guard there, so nothing is put into `aborted_launches`. The orphan shell is dropped and its reader reaches end of file. `finalize_session_obsidian` then runs for a session whose agent never started (`pty.rs:573-578`) and uses up the once-per-process claim (`hooks/server.rs:648`). The next real run of that session is then never finalized. This is the same bug round 2 #2 raised, still open on this path.
   - **A kill during the 5 s wait or the 200 ms sleep, after the `Intact` check.** `kill_session_pty_inner` drops the instance on its own (`pty.rs:1597-1602`), so that shell's end of file races the rollback's later insert into `aborted_launches`. If the end of file arrives first, the vault is written and the claim is used up. If it arrives later, the id is skipped as intended. The result depends on thread timing.
   - **A leak.** When `spawn_pty` fails there is no reader thread, so the id put into `aborted_launches` is never removed. A retry in the same millisecond (`pty_id` is only `session_id` plus wall-clock milliseconds, `pty.rs:787-794`, and `SystemTime` can go backward) would get the same id. Its real end of file would then skip finalization.
   - **Fix:** mark the good case instead of the failed one. Keep a `launched` flag or set of pty ids, set exactly where `disarm()` runs, and finalize at end of file only when it is set. This covers the rollback, `Gone`, the kill during startup and a failed `spawn_pty`, with no race and no leak. Rewrite the scenario around it, and add a test for the `Gone` and kill-before-delivery cases.

2. **Medium: the kill-path test would use the running app's real prompt directory (task 1.1, last bullet; implementation "`kill_session_pty_inner` … delete `launch_prompt_file_path(dir, &pty_id)`").** The plan has `kill_session_pty_inner` find the directory through `launch_prompt_dir()`, which points at the real `ipc_dir()` and creates the directory if it is missing.
   - The test "`kill_session_pty_inner` deletes the PTY's prompt file" therefore has to place a file in `/run/user/<uid>/nergal/launch-prompts`, the folder the installed Nergal is using.
   - That contradicts the plan's own Per-phase risk line ("Tests only call the explicit-dir functions … never `launch_prompt_dir()`").
   - **Fix:** have the kill path call a helper that takes the directory (for example `remove_launch_prompt_in(dir, pty_id)`), and test that helper against a `tempfile` directory. Alternatively, pass the directory into `kill_session_pty_inner`.

3. **Low: `LaunchRollback::drop` must never panic, and the plan does not say so (D3).** The guard runs while a panic is already unwinding: D3 counts that as a feature.
   - A panic inside the guarded code while it holds `instances` or `pending_prompts` poisons that lock. If `Drop` then hits a second panic (an `unwrap`, a poisoned lock it fails to handle), the process aborts, killing every session.
   - State it as a contract: every step uses `lock().ok()` or `PoisonError::into_inner`, does no fallible indexing, and ignores I/O errors.
   - Also note that a dropped command future (app shutdown) runs the rollback. That is harmless, but it is a second way the guard can fire.

4. **Low: "leaves the session as it was before the start" is not literally true.** `reserve_session_pty` records the activity mode `"running"` with a fresh timestamp (`pty.rs:682`, `agents/state.rs:222-234`). Neither the rollback nor `release_session_pty` resets it, so the session looks `"running"` and recently active right after a failed launch. Either restore the previous activity in the rollback, or narrow the requirement text to reservation, liveness, shell, file, prompt and vault.

5. **Low: a kill can still land inside delivery and the start reports success.** `kill_session_pty_inner` removes `session_ptys` and marks the session dead first, then takes the `instances` lock in a separate step (`pty.rs:1589-1602`).
   - If delivery takes the `instances` lock between those two steps, it finds the writer, writes the file and the command, disarms and returns `Ok(pty_id)` for a session that was just killed. The consumed prompt is lost and not requeued.
   - This window was already there before the change. The plan's "writer resolved under the instances lock" only narrows it; it does not apply round 2 #4's "re-check the reservation" advice.
   - Either re-check that `session_ptys[sid]` still equals `pty_id` while holding the `instances` lock (check lock order first: `confirm_reservation_or_extract` takes the two locks one after the other and never nests them, so taking `session_ptys` inside `instances` adds no new ordering conflict, but confirm this across the file), or accept the window explicitly in Risks.

6. **Low: manual walk 2.7 can pass without exercising the failure.** If `launch_prompt_dir()` re-applies `0700` to an existing directory, like `ensure_ipc_dir_0700` does for the parent, the `chmod 0500` is undone and the launch succeeds. Then "the reopen delivers the message" tells you nothing.
   - Pin the contract in the implementation: create the directory only when it is missing and never change its mode afterwards. Or use a failure the code cannot undo, such as replacing the directory with a regular file.
   - Say how to check "no shell stays behind", for example `pgrep -P <nergal pid> -a` before and after.

7. **Low: the retry test proves less than its name says.** "After a rollback a second `reserve_session_pty` succeeds with a new id": the test chooses that id itself, so "new id" is true by construction. It proves the reservation was freed, nothing more. Rename it to say that. Real id freshness only holds while milliseconds differ (see #1).

**Earlier items:**
- **Round 2 #1 (structural rollback): fixed.** The drop guard covers every `?` and early return between the reservation and the PTY write, including `registry.get` and the lock and write errors.
- **Round 2 #3 (testability): largely fixed.** `deliver_launch` and `LaunchRollback` can be tested without Tauri. The wiring inside `start_claude_session` is checked only by manual walk 2.7, as the plan states.
- **Round 2 #4–#9: fixed** (writer resolved under the lock, prelude scope, D5 wording, `resume_pick`, D6 normalization, requeue risk).
- **Round 2 #2 (Obsidian): only partly fixed.** See #1.

The security core is still sound. The prompt never reaches the shell parser or the line editor, it is never read as an option, and the `0600` file sits in an owner-checked `0700` directory. Findings #1 and #2 are small, contained edits. #1 contradicts the plan's own stated invariant, and #2 contradicts its stated test-isolation guarantee, so both should be fixed in the plan before the build starts.

VERDICT: REVISE
