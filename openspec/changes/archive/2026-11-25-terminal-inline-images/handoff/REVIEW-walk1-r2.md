# Review: walk 1 fixes, round 2 (Revision 3, group 8)

Scope: the uncommitted delta on `mufdidev/fri-611-terminal-images` after group 8. Files: `src-tauri/src/terminal/image_fetch.rs`, `image_fetch_tests.rs`, `src-tauri/src/pty.rs` (`terminal_image`), `src/components/terminal/imageCache.ts`, `imageCache.test.ts`, `src-tauri/src/csp_tests.rs`, `src-tauri/tauri.conf.json`, and the change artifacts.

## Checks that passed

- `cargo test --lib image_fetch`: 20 passed. `npx vitest run src/components/terminal/imageCache.test.ts`: 36 passed.
- The Red locks match. `git hash-object` gives the hashes in `handoff/.red-tests` for `image_fetch_tests.rs`, `imageCache.test.ts`, `csp_tests.rs` and `image_runs_tests.rs`. The file uses git blob hashes, not `sha1sum`.
- `openspec validate terminal-inline-images` passes. Only the old warnings about long requirement text remain.
- The semaphore is static and the code never closes it. tokio's `Semaphore` is fair (FIFO). `tokio::task::spawn_blocking` runs inside Tauri's tokio runtime, so the switch from `tauri::async_runtime::spawn_blocking` is safe.
- WebP gives no new memory amplification. image 0.24.9 decodes only the first ANMF frame and keeps the other frames compressed (`image-0.24.9/src/codecs/webp/extended.rs:187-203`). wezterm already decodes that frame once, when it reads the dimensions (`WZ/term/src/terminalstate/image.rs:299-310`).
- The `retryable` counter stays correct. `put` and `drop` update it on every state change. `queued` is not retryable. `releaseAll` and `dispose` set it to zero when they clear the entries.

## Prior findings

| # | Sev | Status | Evidence |
|---|---|---|---|
| 1 | Important | Resolved in code. Walk row is partial. New finding 1 weakens the frontend cap. | Backend: `DECODE_PERMITS = 2` and `payload_for_bounded` (`image_fetch.rs:14-32`), called at `pty.rs:2489`. Frontend: `MAX_IN_FLIGHT_FETCHES = 2` and a queue (`imageCache.ts:7`, `:149-163`). Risk text: `design.md:124-125`, `implementation.md:77`. Walk row 4.8 (`tasks.md:61`) does not give the attack shape or an RSS bound (new finding 10). |
| 2 | Important | Resolved | `iterm2_jpeg_payload_is_kind_one_with_the_file_bytes` (`image_fetch_tests.rs:186`) and `iterm2_gif_payload_is_kind_one_with_the_file_bytes` (`:201`) check kind 1, 64x48 and the file bytes. The lock is updated. The optional WebP case is not added (new finding 7). |
| 3 | Nit | Partial | 7.1-7.4 are `[x]` (`tasks.md:40-43`). Group 4 still comes after groups 5-8 (`tasks.md:52`). 8.4 is still `[ ]` (`tasks.md:50`). |
| 4 | Nit | Partial | One `console.warn` for a non-`ArrayBuffer` (`imageCache.ts:199-203`), with a test (`imageCache.test.ts:548`). Walk 4.7 (`tasks.md:60`) has no console check. |
| 5 | Nit | Resolved | Runtime allowlist `Tiff \| Bmp \| WebP` (`image_fetch.rs:147-152`). `design.md:122` states the `panic = "abort"` exposure. `implementation.md:77` no longer says that `spawn_blocking` isolates a failure. |
| 6 | Nit | Resolved | `decode_to_rgba` uses one reader and has no dimension pre-check (`image_fetch.rs:143-164`). The over-limit test now uses 6000x6000 RGB, 108 MB of decoder buffer (`image_fetch_tests.rs:336`). |
| 7 | Nit | Resolved | `design.md:124` and `image_fetch.rs:13` say that one decode peaks near 300 MB. The optional copy saving is not done. That is acceptable. |
| 8 | Nit | Resolved | `spec.md:45` says "decoder buffer exceeds 100,000,000 bytes". |
| 9 | Nit | Resolved | The test is renamed to `encoded_file_whose_decoder_buffer_exceeds_the_limit_is_unsupported_fast` (`image_fetch_tests.rs:332`). The timing assertion matches "fast". |
| 10 | Nit | Resolved | The unsupported-format error is permanent (`imageCache.ts:165-168`, `:186-187`), with tests at `imageCache.test.ts:526` and `:538`. The re-decode after a hide is accepted in `design.md:127`. |
| 11 | Nit | Resolved | `design.md:128` gives the reason for `http://ipc.localhost`. `csp_tests.rs` pins the source list. |
| 12 | Nit | Partial | `design.md:95` now says which path holds the mutex. `implementation.md:65` corrects the wezterm statement. The line references at `implementation.md:64` and `:85-89` still point to the code before Revision 3. Revision 3 has no `path:line` anchors. D3 and D8 contradict Revision 3 (new finding 9). |
| 13 | Nit | Not acted on, by decision | `docs/shortcuts.md` is an unrelated user edit and is never committed. This finding does not count toward the verdict. |

## Spec conformance (Revision 3 requirements)

| Requirement / scenario | Implementing path:line | Test |
|---|---|---|
| Backend narrows: Fetch an iTerm2 TIFF | `image_fetch.rs:127-131`, `:143-164` | `image_fetch_tests.rs:291` |
| Backend narrows: Unsupported encoded format | `image_fetch.rs:156-158` | `image_fetch_tests.rs:323`. The test calls `payload_for` on a hand-built `ImageData`, not on a visible row. Task 7.1 allows this. |
| Backend narrows: JPEG and GIF stay encoded | `image_fetch.rs:121-126`, `:166-171` | `image_fetch_tests.rs:186`, `:201` |
| Backend narrows: Other encoded formats are refused | `image_fetch.rs:147-152` | `image_fetch_tests.rs:216` (PNM) |
| Backend narrows: Backend decodes are bounded | `image_fetch.rs:14-32`, `pty.rs:2489` | `image_fetch_tests.rs:362` |
| Backend narrows: requirement text "decode ... WebP" | `image_fetch.rs:149` | None (new finding 7) |
| Frontend bounds: Third fetch waits for a slot | `imageCache.ts:149-163`, `:178-181` | `imageCache.test.ts:480`, `:499`, `:512` |
| Frontend bounds: Unsupported format is not retried | `imageCache.ts:165-168`, `:186-187` | `imageCache.test.ts:526`, `:538` |
| Frontend bounds: A non-bytes response is reported once | `imageCache.ts:10`, `:198-206` | `imageCache.test.ts:548` |
| Frontend bounds: requirement text "at most 2 fetches in flight" | Broken after `releaseAll` (new finding 1) | None |

## New findings

1. **Important.** `src/components/terminal/imageCache.ts:283`, `:178`, `:184`.
   Expectation broken: `spec.md:74` says that each cache SHALL keep at most 2 `terminal_image` fetches in flight. `releaseAll` sets `inFlight = 0`, but the old fetches keep running in the backend. Their settle handlers return early on the epoch check, so they never decrement anything. A session switch calls `hideEntry` and then `releaseAll` (`terminalService.ts:278`). A quake close calls `setRegionVisible` and then `releaseAll` (`terminalService.ts:269`). The next `show` paints and starts 2 new fetches while the 2 old ones still run. Each hide/show cycle leaves up to 2 more orphan requests. The backend semaphore is global and FIFO. Thus the orphans hold the decode slots, and new fetches in every terminal wait behind them. For example, 10 fast quake toggles over 2 large TIFFs queue 20 full decodes (each up to about 300 MB peak) before any visible image in the app gets a slot. When the same hash comes back, the backend also decodes it twice. No test covers a fetch cap across `releaseAll`.
   Fix: count real requests. Do not reset the count in `releaseAll`. Decrement it on every settle, before the epoch check:
   ```ts
   (buf) => {
     inFlight -= 1;
     if (disposed) return;
     if (startedIn === epoch) onFetched(hash, buf);
     drainQueue();
   },
   (reason) => {
     inFlight -= 1;
     if (disposed) return;
     if (startedIn === epoch) {
       if (isUnsupported(reason)) put(hash, { state: "dead" });
       else { /* failed + count, as today */ }
     }
     drainQueue();
   },
   ```
   Remove `inFlight = 0` from `releaseAll` (`:283`). `drainQueue` after an old-epoch settle is correct, because `releaseAll` already replaced the queue. Add a test: `get(a)`, `get(b)`, `releaseAll()`, `get(c)`. Expect no fetch of c until a or b settles. Add a scenario under "The frontend bounds image fetches".

2. **Nit.** `src/components/terminal/imageCache.ts:158-163`.
   Expectation broken: work should go to images that rows still show. `drainQueue` starts every queued hash in FIFO order and does not check `visible`. The queue has no size limit. In yazi (walk 4.4), the user moves across 20 files quickly. The cache queues 20 previews and then fetches and decodes all of them, oldest first. The current preview starts last. A hash that wezterm has dropped meanwhile fails with "image no longer held". It then stays as a retryable `failed` entry.
   Fix: in `drainQueue`, skip a hash that is not in `visible` and `drop` it, so that a later `get` starts a new fetch:
   ```ts
   const next = queue.shift();
   if (next === undefined || entries.get(next)?.state !== "queued") continue;
   if (!visible.has(next)) { drop(next); continue; }
   beginFetch(next);
   ```
   Add a test: queue c and d, then `setVisible({d})`, then settle a. Expect a fetch of d and no fetch of c.

3. **Nit.** `src/components/terminal/imageCache.ts:132-147` against `:114-117` and `:216`.
   Expectation broken: a blocked hash that `requestRetries` restarts should be able to decode when its bytes arrive. `requestRetries` checks each blocked hash with the live `decoding` count. `retryBlocked` runs right after `decoding -= 1`, so `decoding` is often 0. In that case the "a lone decode may cross the ceiling" rule passes for each hash while `held` stays below 512 MB. Example: 400 MB is referenced and B and C (100 MB each) are blocked. The loop restarts both. B arrives first and decodes. C arrives while B decodes, so `onFetched` blocks it again, and C is fetched again after B settles. Since D8, each extra fetch of a TIFF is a full backend decode.
   Fix: after the loop restarts one blocked hash, check the next ones as if a decode is in flight. For example, pass a flag to `overCeiling`: `overCeiling(e.size, held, decoding > 0 || restarted > 0)`.

4. **Nit.** `src-tauri/src/terminal/image_fetch.rs:23-31`.
   Expectation broken: the doc comment says "The permit is held until the blocking task ends, so an abandoned request cannot free a slot while its decode still runs." The permit is a local of the async future. If something drops the future while it awaits the `JoinHandle`, the permit is released and the blocking decode continues without a slot. Tauri does not cancel async commands today, so there is no current impact. But the code does not give the invariant that the comment states. Also, a `JoinError` and a closed semaphore both become `UnsupportedFormat` (`:28`, `:31`). The frontend then marks the hash dead for good.
   Fix: move the permit into the closure. `SemaphorePermit<'static>` is `Send + 'static`:
   ```rust
   let permit = decode_slots().acquire().await.map_err(|_| ImageFetchError::UnsupportedFormat)?;
   tokio::task::spawn_blocking(move || {
       let _permit = permit;
       payload_for(&image)
   })
   .await
   .map_err(|_| ImageFetchError::UnsupportedFormat)?
   ```

5. **Nit.** `openspec/changes/terminal-inline-images/design.md:132`.
   Expectation broken: the docs must match the code. The text says "2 permits keep one slot free". No slot is reserved. Every payload takes a permit, including sixel copies and PNG/JPEG/GIF copies (`image_fetch.rs:24-29`). Two large TIFF decodes take both slots and stop every other image fetch in the app. `implementation.md:101` already says this.
   Fix: change the text to "2 permits let a second fetch proceed while one decode runs; two concurrent decodes stall other fetches until one ends". Alternatively, take the permit only on the `decode_to_rgba` path, and say that copies are bounded by the frontend cap.

6. **Nit.** `src-tauri/src/terminal/image_fetch.rs:127` and `:147-152`.
   Expectation broken: a refused format should cost nothing. `payload_for` clones the whole encoded file while it holds the permit. `decode_to_rgba` refuses the format only after the clone. A PNM that wezterm places can be up to about 75 MB.
   Fix: under the guard, check the format with `image::guess_format(bytes)`. Return `UnsupportedFormat` before the clone for any format other than TIFF, BMP or WebP. Keep the check in `decode_to_rgba` as the authority.

7. **Nit.** `src-tauri/src/terminal/image_fetch_tests.rs` (no case), `spec.md:45`.
   Expectation broken: each format that the requirement names needs a test. wezterm keeps every WebP as `EncodedFile`, scaled or not (`WZ/term/src/terminalstate/iterm.rs:104-111`). Thus every iTerm2 WebP goes through `decode_to_rgba`. If a change drops `WebP` from `:149`, every WebP fails and no test detects it.
   Fix: add `iterm2_webp_is_decoded_to_rgba_kind_zero`. Make a 64x48 lossless fixture with image 0.24's pure-Rust `codecs::webp::WebPEncoder::new_lossless`, or embed a base64 fixture. Assert kind 0, 64x48 and 12288 bytes. Re-lock.

8. **Nit.** `src/components/terminal/imageCache.ts:10`.
   Expectation broken: tests must not depend on order. `warnedNonBuffer` is module state. It is shared by every cache in the page and by every test in `imageCache.test.ts`. A second test that expects the warning will see zero calls if it runs after `:548`. Prior finding 4 asked for "once per cache". The spec says "once" and does not give the scope.
   Fix: decide the scope and state it in `spec.md:74`. Once per app is acceptable. Then either keep the flag in the `createImageCache` closure, or export a reset that only tests use.

9. **Nit.** `openspec/changes/terminal-inline-images/design.md:67`, `:112`, `:118`, `tasks.md:50`, `:52`, `implementation.md:64`, `:85-96`.
   Expectation broken: the docs must match the code, and task 8.4 is still open.
   - D3 (`design.md:67`) says that "any other encoded format ... is decoded in Rust" and "only an undecodable one returns an error". D8 (`:112`) says "decodes any other encoded file" and "RGBA would exceed 100,000,000 bytes". Revision 3 narrows both. The original text still contradicts it.
   - `### Revision 3` (`:118`) is an H3 under `## Revision 2`, so the document makes it part of Revision 2.
   - `implementation.md:93-96` has no `path:line` anchors for the Revision 3 code. Add `image_fetch.rs:14-32`, `:147-152`, `pty.rs:2489`, `imageCache.ts:149-163`, `:165-168`, `:198-206`. The facts at `:85-89` describe the code before Revision 3. Mark them so.
   - Group 4 is still last after groups 5-8 (prior finding 3).
   Fix: add "(narrowed in Revision 3)" to D3 and D8, or edit them. Make Revision 3 an H2. Add the anchors. Check 8.4 after the edits. Renumber Verification, or move it.

10. **Nit.** `openspec/changes/terminal-inline-images/tasks.md:60-61`.
    Expectation broken: a walk row must have a clear pass condition. 4.7 does not check the console (prior finding 4). 4.8 says "a loop of `chafa -f iterm` over 30 photos ... memory bounded". chafa sends uncompressed TIFF, and a loop scrolls each image off the screen. Thus the row does not test the attack from prior finding 1 (small compressed files that decode large, many visible at once), and "bounded" has no number.
    Fix: 4.7: "the webview console shows no 'IPC custom protocol failed' and no 'non-ArrayBuffer' warning". 4.8: "a script prints 30 distinct deflate TIFFs of about 5000x5000 side by side (`doNotMoveCursor=1`). The app stays usable and `ps -o rss= -p $(pidof nergal)` stays below about 1 GB". Add a row for new finding 1: "toggle the quake 10 times over 2 large TIFFs. The images come back within about 2 s".

## Summary

- Prior findings: 9 resolved, 3 partial (3, 4, 12), 1 not acted on by decision (13). Both prior Important findings are resolved in code.
- New findings: Important 1 (finding 1), Nit 9 (findings 2-10).
- The backend bound is correct and tested. The format allowlist is correct. The CSP change is narrow and pinned by tests.
- The frontend fetch cap is not a real cap. `releaseAll` resets the count while requests are still live. Thus normal hide/show cycles stack orphan decodes on the global FIFO semaphore and delay images in every terminal.
- Top 3 priority fixes: (1) do not reset `inFlight` in `releaseAll`, and decrement it on every settle (finding 1); (2) skip queued hashes that are no longer visible (finding 2); (3) close task 8.4: correct D3/D8, add the Revision 3 anchors, and make walk rows 4.7/4.8 measurable (findings 9, 10).

VERDICT: FAIL
