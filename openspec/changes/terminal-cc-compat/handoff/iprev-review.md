1. **HIGH, tests share the process-wide cell, so they race.** `cargo test` runs tests as threads in one process.
   - The plan's 1.1 tests set different values through `set_default_colors`: first fg `#18181b` and bg `#ffffff`, then bg `#09090b`.
   - Those writes race with each other and with any other session test that queries a color, because `session.rs:739-799` builds sessions through `NergalTerminalConfig::new()`.
   - The "one test that covers set, query, reset" advice does not fix this, since other tests still read the cell concurrently.
   - Fix: keep the cell in an `Arc<RwLock<…>>` that the config holds. Production gets the shared static `Arc`, while tests build a config with a private `Arc` through a `with_default_colors_source(...)` constructor. That removes the global from the tests and makes `reset_default_colors` unnecessary.
   - The existing OSC 11 test only checks the `\x1b]11;rgb:` prefix (`session.rs:793`), so it won't break today. Any test that asserts an exact color will.

2. **MED, the risk analysis for an unclosed mode 2026 is wrong, and the wait logic is under-specified.**
   - The plan says "delays every frame by 150 ms". With `sync_since` set only on the off-to-on edge and compared against `SYNC_OUTPUT_CAP`, that holds only for the first 150 ms. After that, `sync_since` is older than the cap and no wait happens until the next `l`.
   - The plan must state that a second `h` while the mode is already on does not reset `sync_since`. Otherwise a program that repeats `h` can hold the frame forever.
   - Say explicitly that `l` clears it, and decide whether a terminal reset or `RIS` (`ESC c`) also clears it.
   - Also record that during a hold, the reader's `notify_one` permits collapse into one. That costs at most one extra empty cycle after the hold, which is harmless.

3. **MED, how the emitter reads `sync_output_since`.** `run_emitter` (`emitter.rs:93-145`) would lock `session` on every 8 ms step, and the reader thread holds that mutex during `advance_bytes` (`pty.rs:501-502`).
   - Prefer an `Arc<AtomicU64>` or a small separate `Mutex<Option<Instant>>` that the session shares with the handle.
   - Each wait step must also re-check `shutdown` and the subscriber, as the existing loop does at `emitter.rs:107` and `:114`. Otherwise drop or abort during a wait is slow to take effect.
   - Better still, wait on `notify` with a timeout so an `l` flushes at once instead of up to 8 ms late.

4. **MED, not every snapshot goes through the emitter, so some paths can still show half a frame.** `pty.rs:1967` calls `grid_snapshot()` directly. `terminal_attach` sends a full grid, and the scroll and resize commands may snapshot too.
   - The plan says nothing on whether those paths honor the hold. Either state that they are out of scope, or gate them the same way.

5. **MED, the scanner needs a defined state machine.**
   - It must tolerate the sequence being split at every byte offset (tests should iterate all split points).
   - It must not match inside OSC, DCS or APC strings, or after a stray `ESC`.
   - The "tail at most 8 bytes" claim needs the exact bound. `ESC [ ? 2 0 2 6 h` is 8 bytes, so the tail is 7.
   - Combined forms such as `CSI ? 1 ; 2026 h` are explicitly untracked. A silent miss is bounded because the 150 ms cap only applies once the mode is detected, so a miss means tearing, not a stall. Say so.

6. **MED, `set_terminal_default_colors` can lose a race at startup.** `refreshTermTheme()` runs at module load (`terminalService.ts:117`) and `invoke` is async.
   - A session spawned right then can receive Claude Code's startup `OSC 11 ?` before the backend has the colors.
   - Also confirm that `data-theme` and the CSS vars are applied before `refreshTermTheme` reads them at load. The same call at `:141` is the MutationObserver path.
   - Fix: add a mount-time resend and wait for the first send before spawning sessions, or say plainly that the first query can return the default.
   - Catch and log a rejected `invoke` so it doesn't become an unhandled rejection.

7. **LOW, apps that set colors keep the old palette.** If an app sends `OSC 10;rgb:` or `OSC 11;rgb:`, wezterm forks the palette (`mod.rs:670-675`) and theme changes stop reaching that session. This is the "keeps precedence" requirement, so it is correct. Add a scenario for it so the behavior is pinned.

8. **LOW, the cached `palette` (`session.rs:82`) is built from the default config.** The plan says this is fine because only indexed colors use it. That matches the plan's own claim that default fg/bg cells are `None`. Add a test that default-colored cells still snapshot as `None` after `set_default_colors`.

9. **LOW, security.** The command takes only `[u8;3]` values, so there is no injection surface. In the lock code, do not `unwrap()` a poisoned `RwLock`, because the project forbids it outside tests. Use `.ok()`, or recover with `into_inner`.
   - On the cross-platform rule, the cell and the command are platform-neutral, so the `#[cfg]` rule doesn't apply.

10. **LOW, rollback.** It is clean: one static or `Arc`, one command, and a scanner that is additive. No schema or config migration is involved. The plan could note that removing the frontend call alone restores the default palette answers.

11. **LOW, timing tests.**
    - "Between 150 and 300 ms" and "within 50 ms" can flake on loaded CI.
    - Use `tokio::time::pause` where possible, or widen the upper bounds.
    - The "no update has AAA without BBB" test needs the sequence to be deterministic. Parse `h` and `AAA` as separate `advance_bytes` calls with the `notify` in between, as the plan intends.
    - Also assert that `l` flushes the frame in well under 150 ms.

12. **LOW, task 3.1.** Add `npx tsc --noEmit` for the new `cssColor.ts` and the macOS and Windows gates. They are unaffected, since nothing is OS-specific.

Items 1 and 2 are the main fixes. Items 3, 4 and 6 are real design gaps but small to resolve.

VERDICT: REVISE

=== Round 2 (sonnet) ===
1. **MED, ESC-reset can desync the mode state.**
   - D2 treats a literal `ESC c` as RIS and clears `sync_since`. A literal match can also fire on an `ESC c` inside a DCS or APC payload that wezterm's parser swallows. That would clear the mode early, which risks tearing but never a stall.
   - Round 1 item 5 asked for state-machine behavior. It is now accepted as "literal matcher", but the plan never says that.
   - Add one line to the design: false positives and false negatives only mean tearing, and the 150 ms cap bounds any stall.
   - Add one test that an `ESC c` split across chunks (`ESC`, then `c`) also clears the mode. The plan's split-at-every-byte test covers only the `h` sequence.

2. **MED, the emitter wait can lose a wakeup.**
   - D3 waits on `notify` with a timeout. `tokio::sync::Notify` stores a single permit, and the emitter's outer `notified().await` at `emitter.rs:106` consumes it.
   - When the reader parses `h`+`AAA` and calls `notify_one`, the emitter wakes, sleeps 8 ms, then starts the hold wait.
   - An `l` chunk that arrives during the 8 ms sleep stores a permit. It also clears the state, so a re-check of the state before waiting handles that case.
   - State plainly in the task that the check order is: read state, wait only if it is `Some` and younger than the cap, then re-read after every wake. Without it, a naive "wait then check" can stall 150 ms after an `l` that already arrived.
   - Task 2.3 does not specify this. Add a test where `h`, `AAA`, `BBB` and `l` all arrive inside the 8 ms coalesce window, and assert the update arrives promptly.

3. **MED, the plan does not say what happens when the cap passes while chunks keep arriving.**
   - After the cap, every later `notify` snapshots without holding. That is fine and intended.
   - The unclosed-frame scenario implies a program that never closes the mode loses synchronization for good. That is acceptable.
   - Say so in the spec, so nobody reads "does not hold again until the next off-to-on edge" as a bug. The "Output without the mode" scenario covers only mode-off.

4. **LOW, startup race on the first color send is still only partly resolved.**
   - Facts from `globals.css`: the theme tokens are hex, so `cssColorToRgb` covers every theme. That addresses my earlier question about `oklch`.
   - `refreshTermTheme()` at `terminalService.ts:117` reads a CSS var. The plan says the module loads before any session spawns, but it does not check that `data-theme` is applied by then.
   - The plan accepts that a query before the first send gets the default. That is an acceptable trade-off if it is noted.
   - Add to 1.3 that `invoke` is fire-and-forget with a `.catch` that logs. The spec already says that.

5. **LOW, the `SyncState` mutex follows the repo rule only if the tasks say how.**
   - A poisoned `Mutex` must not call `unwrap` outside tests. Task 2.2 and the locked interface do not state how the matcher and the emitter read it.
   - Add: `.lock().ok()` or `into_inner`, matching the `ColorCell` treatment.
   - The default-cells-stay-`None` test and the `ESC c` test both depend on this too.

6. **LOW, flaky bounds.** The 140 to 600 ms and less than 100 ms bounds are wide enough for the stated risk. Say once that the `140` lower bound requires the clock to measure from the `h` parse and not from test start, or measure from the `advance_bytes` call.

Round 1 items 1 (private cells), 2 (first-`h` edge, `l`/RIS clear), 3 (separate `SyncState`, `notify` wait with a timeout), 4 (attach, scroll and resize out of scope), 5 (7 split points, untracked combined forms), 7 (app-set colors scenario), 8 (default cells `None`) and 11 (wide bounds) are addressed with sound rationale. None of the remaining issues needs a design change; they are task-level tightenings.

VERDICT: APPROVED
