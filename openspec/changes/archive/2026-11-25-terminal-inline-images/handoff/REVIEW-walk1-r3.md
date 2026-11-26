# Review: walk 1 fixes, round 3 (Revision 4, group 9)

Scope: the uncommitted delta on `mufdidev/fri-611-terminal-images` after group 9. Files: `src/components/terminal/imageCache.ts`, `imageCache.test.ts`, `src-tauri/src/terminal/image_fetch.rs`, `image_fetch_tests.rs`, the visibility paths in `src/components/terminal/terminalService.ts`, and the change artifacts. Areas that rounds 1 and 2 passed and group 9 did not touch are not re-audited.

## Checks that passed

- `npx vitest run src/components/terminal/imageCache.test.ts`: 40 passed. `cargo test --lib image_fetch`: 21 passed.
- The Red locks match. `git hash-object` gives the hashes in `handoff/.red-tests` for `image_fetch_tests.rs` (`4c008bd`), `imageCache.test.ts` (`5866bb7`), `csp_tests.rs` (`75ba914`) and `image_runs_tests.rs` (`acd591f`).
- In-flight accounting. `beginFetch` decrements `inFlight` on every settle before the epoch check (`imageCache.ts:182-183`, `:188-189`). `releaseAll` no longer resets it (`:280-291`). An old-epoch settle drains only the queue that `releaseAll` replaced (`:290`), so it starts only work queued after the release. After `dispose` a settle returns before the decrement. This is harmless, because `get` returns `undefined` after `dispose` (`:260`) and no fetch starts again.
- Re-entrancy. `onFetched` can call `fail` then `retryBlocked` then `beginFetch` synchronously. The increment happens before the settle handler calls `drainQueue`, so the count stays correct.
- `drainQueue` (`:160-167`). It skips entries that are no longer `queued` and drops a non-visible queued hash through `drop`, which keeps `retryable` correct. A `get` of a hash that is still `queued` returns without a second push (test `imageCache.test.ts:535`).
- Restart count. `overCeiling(size, held, restarted)` (`:114-117`) lets the first restart cross the ceiling only when no decode is in flight. Each later restart in the same loop is checked as if a decode is in flight. I traced the test at `imageCache.test.ts:441`: a=150 and b=140 referenced, x=200 decoding, c and d (150 each) blocked. x fails, held is 290, c restarts (held 440), and d is blocked (590 > 512). That is 3 fetches. The evidence records a mutation check of `restarted`.
- Warn flag. `warnedNonBuffer` is in the `createImageCache` closure (`:84`). The test at `imageCache.test.ts:634` checks one warning per cache.
- Permit. `payload_for_bounded` moves the permit into the `spawn_blocking` closure (`image_fetch.rs:29-32`), so a dropped future cannot free a slot while the decode runs.
- Guess before clone. `payload_for` refuses formats other than Tiff, Bmp and WebP with `image::guess_format` before `bytes.clone()` (`image_fetch.rs:130-137`). The `image.data()` guard is a temporary of the `let bytes = match ...;` statement, so the guard is released before `decode_to_rgba` (`:141`). `decode_to_rgba` keeps the same allowlist as the authority (`:158-163`).
- `visibilityKnown` depends on `setVisible` running before any `get`. This is true on every path in `terminalService.ts`:
  - The only `get` call is in `drawImageRuns` (`terminalService.ts:1508`). It reads only `entry.rowImages`.
  - The only writer of `entry.rowImages` is `applyUpdate` (`:1382`). The next two lines rebuild `rowsByHash` and call `setVisible` with the same hashes (`:1383-1384`). The row paints come after that (`:1407-1409`).
  - Attach: a new entry starts with `rowImages: []` (`:462`), so no `get` happens before the first `applyUpdate`.
  - Show after hide (`showTerminal`, `:353-367`) and quake open (`setRegionVisible`, `:270`): `paintAll` reads `rowImages` from the last `applyUpdate`. `releaseAll` does not clear `visible` or `visibilityKnown`, so the visible set and the rows stay the same pair.
  - Scroll: the backend resends rows, and they go through `applyUpdate`.
  - `onReady` repaints (`repaintImageRows`, `:1520`): the row comes from `rowsByHash`, so every hash in it is in `visible`.
  - The guard `rowsByHash.size > 0 || incomingImages` (`:1381`) skips `setVisible` only when the old and the new rows hold no runs. In that case no `get` can happen.
  - Result: the `!visibilityKnown` branch (`imageCache.ts:164`) is reached only in tests. The recorded deviation (`implementation.md:129`) is correct.
- Anchors. Every anchor in `implementation.md:127-130` matches the code: `image_fetch.rs:12, 14, 16-21, 24, 116, 145, 154, 177`, `pty.rs:2489`, `imageCache.ts:7, 75, 84, 114, 132, 151, 160, 169, 174, 204, 276, 280`, `tauri.conf.json:15-16`, `Cargo.toml:15`.

## Round 2 findings

| # | Sev | Status | Evidence |
|---|---|---|---|
| 1 | Important | Resolved | Decrement on every settle before the epoch check (`imageCache.ts:182-189`). No reset in `releaseAll` (`:280-291`). Test `imageCache.test.ts:593`. Scenario "Requests from before a release still count" (`spec.md:87-90`). Requirement text (`spec.md:79`). Walk row 4.9 (`tasks.md:69`). Re-fetch of a hash that is still in flight from the old epoch stays (new finding 1). |
| 2 | Nit | Resolved | `drainQueue` drops a non-visible queued hash (`imageCache.ts:164-165`). Test `imageCache.test.ts:610`, which also checks that a later `get` fetches it again. Scenario `spec.md:92-95`. The `visibilityKnown` guard is safe in the app (see above). |
| 3 | Nit | Resolved | `restarted` in `overCeiling` and `requestRetries` (`imageCache.ts:114-117`, `:135-145`). Test `imageCache.test.ts:441`. The sizes in the task text are different from the test (new finding 4). |
| 4 | Nit | Resolved | The permit is inside the closure (`image_fetch.rs:29-32`). The doc comment (`:23`) is now true. A `JoinError` and a closed semaphore still map to `UnsupportedFormat` (`:28`, `:34`), and the frontend marks the hash dead. This is acceptable: the semaphore is never closed, and under `panic = "abort"` a decoder panic ends the process before a `JoinError` exists. |
| 5 | Nit | Resolved | `design.md:132` says that 2 permits let a second fetch proceed, that two decodes stall other fetches, and that every payload takes a permit. |
| 6 | Nit | Resolved | `guess_format` before the clone (`image_fetch.rs:130-137`). |
| 7 | Nit | Resolved | `iterm2_webp_is_decoded_to_rgba_kind_zero` (`image_fetch_tests.rs:319`) with a lossless fixture from `WebPEncoder::new_lossless` (`:105-110`). Scenario "Fetch an iTerm2 WebP" (`spec.md:67-70`). The lock is updated. |
| 8 | Nit | Resolved | Per-cache flag (`imageCache.ts:84`). The scope is stated: "the first non-bytes response of each cache" (`spec.md:79`). Test `imageCache.test.ts:634`. |
| 9 | Nit | Resolved, with the ruling | D3 and D8 say "narrowed in Revision 3" and state the narrow rule (`design.md:67`, `:112`). Revision 3 is an H2 (`design.md:118`). The Revision 3 facts are marked as code before group 8 (`implementation.md:84`). The anchors are at `implementation.md:127-130`. Task 8.4 is checked (`tasks.md:50`). The request to renumber Verification is declined by ruling. |
| 10 | Nit | Partial | Row 4.8 now gives the attack shape and an RSS number (`tasks.md:68`). Row 4.9 is added for finding 1 (`tasks.md:69`). Row 4.7 (`tasks.md:67`) still has no console check (new finding 2). The 4.8 measurement has gaps (new finding 3). |

## New findings

1. **Nit.** `src/components/terminal/imageCache.ts:184` (and `:190`).
   Expectation broken: a hide/show cycle should not repeat a full backend decode for an image whose request is still running. A settle from an old epoch discards the bytes, even when the new epoch has queued the same hash. Walk 4.9: the quake closes while A and B (large TIFFs) are in flight, then opens. The paint queues A and B behind the 2 old requests. Old A settles and its bytes are dropped. Then A is fetched again, and the backend decodes it a second time. The last open thus waits for up to two decodes of each image in series. This can exceed the "about 2 s" in row 4.9 on large TIFFs. The cap holds, so this is a latency cost, not a memory or starvation defect.
   Fix: in the old-epoch success branch, use the bytes when the new epoch is waiting for the same hash. `drainQueue` already skips an entry that is no longer `queued`:
   ```ts
   (buf) => {
     if (disposed) return;
     inFlight -= 1;
     if (startedIn === epoch || entries.get(hash)?.state === "queued") onFetched(hash, buf);
     drainQueue();
   },
   ```
   Add a test: `get(a)`, `get(b)`, `releaseAll()`, `setVisible({a})`, `get(a)`, then resolve the old a. Expect one decode of a and no second fetch of a.

2. **Nit.** `openspec/changes/terminal-inline-images/tasks.md:67`.
   Expectation broken: a walk row must have a clear pass condition (round 2 finding 10, first part, not acted on). "without errors" does not say where to look. The two failures this change can cause are a console warning from Tauri (`tauri-2.11.1/scripts/ipc-protocol.js:61`, "IPC custom protocol failed, Tauri will now use the postMessage interface instead") and the cache warning (`imageCache.ts:208`). Neither one shows in the UI.
   Fix: append "and the webview console shows no 'IPC custom protocol failed' and no 'non-ArrayBuffer' warning".

3. **Nit.** `openspec/changes/terminal-inline-images/tasks.md:68`.
   Expectation broken: the measurement must measure the bound that the row claims.
   - `pidof nergal` can return more than one PID. With the MCP server on, every agent session runs `nergal mcp` (`src-tauri/src/mcp/registration.rs:142`). The hook CLI is the same binary too. `ps` then prints several rows, and the pass condition does not say which row counts.
   - The command measures only the Rust process (the 2 decode slots, about 600 MB at peak). The decoded bitmaps and the 512 MB frontend ceiling are in the WebKit web process, which the row does not measure.
   - 1.5 GB is higher than round 2 proposed (about 1 GB), and no text gives the reason.
   Fix: measure the main process with `pgrep -o -x nergal`. Also record `ps -o rss= -C WebKitWebProcess` (sum the rows). Give the two limits with the reason. For example: "backend below about 1 GB (baseline + 2 decodes of about 300 MB), web process below about 1 GB (baseline + 512 MB ceiling + one image)".

4. **Nit.** `openspec/changes/terminal-inline-images/tasks.md:54` against `src/components/terminal/imageCache.test.ts:441`.
   Expectation broken: the docs must match the code. Task 9.1 says "with 400 MB referenced and two 100 MB blocked hashes, a decode settling restarts only one of them". The rebuilt test uses 290 MB referenced (a=150, b=140), a failing 200 MB decode of x, and two 150 MB blocked hashes. The task text also leaves out the decode in flight that blocks the two hashes, and without that decode neither 100 MB hash is blocked at 400 MB. `evidence.md` records the rebuild, but the task text was not updated.
   Fix: change 9.1 to "with 290 MB referenced, a 200 MB decode in flight and two 150 MB hashes blocked behind it, the failure of that decode restarts only one of them".

## Summary

- Round 2 findings: 9 resolved, 1 partial (10). The Important finding 1 is resolved in code, and a test and a spec scenario cover it.
- New findings: Important 0, Nit 4.
- The group 9 code is correct. The in-flight count stays correct across epochs and after `dispose`. The queue drops only hashes that left the screen. The restart loop no longer restarts hashes that then block each other. The permit stays held for the whole decode. A refused format is rejected before the copy. `terminalService.ts` calls `setVisible` before any `get` on every path, so `visibilityKnown` only changes behavior in tests.
- Top 3 priority fixes: (1) use the bytes from an old-epoch settle when the same hash is queued again, so a quake toggle does not decode twice (finding 1); (2) add a console check to walk row 4.7 (finding 2); (3) measure the correct processes in walk row 4.8 (finding 3).

VERDICT: PASS
