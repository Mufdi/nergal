# Review: walk 1 fixes (Revision 2, D7/D8)

Scope: the uncommitted delta on `mufdidev/fri-611-terminal-images` (`tauri.conf.json` CSP, `image_fetch.rs` decode, `Cargo.toml`/`Cargo.lock`, `csp_tests.rs`, `image_fetch_tests.rs`, change artifacts and spec deltas).

## Checks that passed

- Root cause D7 holds for Tauri 2.11.1 on Linux. `ipc-protocol.js:25-69` tries `fetch(ipc://localhost/<cmd>)` first. On any failure it sets the page-wide `customProtocolIpcFailed` flag and uses `postMessage` for every later call. On that path `protocol.rs:337-339` sets `can_use_channel_for_response = false`, and `protocol.rs:390-403` formats a `Raw` body with `format_result` as a JSON number array. Tauri does not add `ipc:` to the CSP by itself (`manager/mod.rs:53-107` only adds nonces and hashes). `connect-src 'self'` on `tauri://localhost` does not match the `ipc:` scheme. On Linux `convertFileSrc` builds `ipc://localhost/...` (`core.js:16-18`), so `ipc:` is the source that matters there.
- The switch to the custom protocol does not change other commands in this repo. JSON responses parse the same on both paths. The only raw response is `terminal_image` (`pty.rs:2465`). The only Channel is `Channel<GridUpdate>` (`pty.rs:2430`). It sends JSON, and its large messages go through `plugin:__TAURI_CHANNEL__|fetch` on both paths. `@tauri-apps/api` 2.11.0 reorders Channel messages by index (`core.js:78-103`). Events (`PtyOutput.data: number[]`) go through `eval` and do not change. Request bodies are JSON on both paths, because every `invoke` passes an object (`process-ipc-message-fn.js:7-45`). wry 0.55.1 needs WebKitGTK 2.40 (`webkit2gtk/v2_40`), so POST bodies reach the handler.
- The CSP change does not widen access for framed browser tabs or injected script. A cross-origin iframe has its own document CSP, and the parent's `connect-src` never governed it. Tauri injects the IPC script and the invoke key into the main frame only (`manager/webview.rs:159-164`). Script in the main document could already call every command through `postMessage`. `ipc:` reaches only the local Tauri handler. It is not an exfiltration path.
- Feature narrowing in Cargo is not possible. Cargo unifies features for the single `image 0.24.9` package. wezterm-term enables the default features, so `default-features = false` in `src-tauri/Cargo.toml` removes nothing. Narrowing must happen at runtime (finding 5).
- The guard drop is correct. The `image.data()` temporary ends at the `;` of `let bytes = match ...` (`image_fetch.rs:94-109`). The decode runs without the image mutex.
- `openspec validate terminal-inline-images` passes. Only the existing warnings about long requirement text remain.

## Findings

1. **Important.** `src-tauri/src/pty.rs:2489`, `src-tauri/src/terminal/image_fetch.rs:131-150`, `src/components/terminal/imageCache.ts:142-159` and `:208-213`.
   Expectation broken: a PTY writer must not be able to exhaust the app's memory with a small input. Before D8, a fetch copied at most what wezterm already held, so the cost grew with the input. D8 adds a large amplification: a deflate TIFF of about 100 KB declares 5000x5000 and decodes to 100 MB of RGBA. The peak per decode is about 2-3x that (finding 7). wezterm only reads the dimensions of a non-downscaled image, so placing it costs little. An attacker sets the display size to at least the native size, so wezterm does not downscale it. The attacker then uses `do_not_move_cursor` and cursor moves to leave a strip of many distinct images visible at once. The frontend starts one fetch per new hash with no limit (`get` -> `startFetch`). Each fetch runs a decode in `spawn_blocking`, and Tokio allows up to 512 blocking threads. The frontend 256/512 MB gate (`overCeiling`) runs only after the bytes arrive (`onFetched`), so it cannot bound the backend. Thirty such images need about 6-9 GB. The OOM killer then ends Nergal and every session in it.
   Fix: limit concurrent decodes in the backend, and acquire the limit before `spawn_blocking`:
   ```rust
   static IMAGE_DECODES: std::sync::LazyLock<tokio::sync::Semaphore> =
       std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(2));

   let _permit = IMAGE_DECODES.acquire().await.map_err(|e| e.to_string())?;
   let payload = tauri::async_runtime::spawn_blocking(move || payload_for(&image))
   ```
   Also cap concurrent fetches in `imageCache.ts`, for example two in flight and a queue for the rest. Add a risk line to design D8 and `implementation.md:77`. Also add a walk row: a script that prints 30 distinct large compressed TIFFs keeps the app alive and below about 1 GB RSS.

2. **Important.** `openspec/changes/terminal-inline-images/tasks.md:40`, `src-tauri/src/terminal/image_fetch_tests.rs` (no case), `openspec/changes/terminal-inline-images/specs/terminal-wezterm/spec.md:45`.
   Expectation broken: task 7.1 requires "PNG, JPEG and GIF still return kind 1", and the new requirement says kind 1 applies only to PNG, JPEG or GIF. The tests cover PNG only (`:138`, `:150`, `:162`, `:174`). No test covers a JPEG or GIF that is not downscaled and stays `EncodedFile`. The `.red-tests` lock certifies a test set that lacks two of the cases that 7.1 claims. If a change drops JPEG or GIF from `is_allowed_encoding` (`image_fetch.rs:152-157`), the image falls through to the Rust decoder and becomes kind 0. That breaks GIF animation for FRI-612, and no test fails.
   Fix: add `iterm2_jpeg_payload_is_kind_one_with_the_file_bytes` and `iterm2_gif_payload_is_kind_one_with_the_file_bytes`. Use small fixtures that fit `session_800x480` and are not downscaled. Assert `payload[0] == 1`, the native width and height, and that the body equals the file bytes. Re-lock `.red-tests`. You can also add a WebP case, because the spec and the proposal name WebP as a decoded format (`spec.md:45`, `proposal.md:10`).

3. **Nit.** `openspec/changes/terminal-inline-images/tasks.md:38-43`.
   Expectation broken: tasks must match the code. Tasks 7.2-7.4 are implemented, and `evidence.md` reports them done, but they stay `[ ]`. Group 7 also comes before `## 4. Verification`, so the numbers go 6, 7, 4.
   Fix: check 7.2-7.4 against the code. Check 7.1 after finding 2 is fixed. Move group 7 after group 4, or renumber Verification.

4. **Nit.** `src/components/terminal/imageCache.ts:161-167`.
   Expectation broken: a regression of the transport must be visible. If the custom protocol fails again (for example, a later CSP edit), `parseImageHeader` throws on a number array, the entry becomes `dead`, and nothing gets logged. That is the failure that walk 1 had to find by hand. Also, walk 1 did not observe the frontend half of the root cause. Revision 2 infers it from source.
   Fix: in `onFetched`, call `console.warn` once per cache when `!(buf instanceof ArrayBuffer)`, then mark the entry dead. Add this check to walk 4.7: the webview console shows no "IPC custom protocol failed" warning on the installed deb.

5. **Nit.** `src-tauri/src/terminal/image_fetch.rs:107-110` and `:138-144`, `src-tauri/Cargo.toml:168`.
   Expectation broken: the decoder surface should be the smallest that the feature needs. D8 decodes every format that image 0.24 enables by default: TIFF, BMP, WebP, ICO, PNM, TGA, HDR, DDS, farbfeld, OpenEXR and QOI. Only TIFF (chafa) is a shown need. The release profile sets `panic = "abort"`, so a decoder panic ends the process. `spawn_blocking` does not contain it, and the `JoinError` mapping at `pty.rs:2491` never runs in release. The exposure is not a new class. wezterm already runs `image::load_from_memory` on any downscaled image that is not GIF, PNG or WebP (`wezterm term/src/terminalstate/iterm.rs:104-118`), and an attacker controls the downscale. But D8 adds a second path, and that path runs again on every fetch.
   Fix: after `with_guessed_format`, allow only the formats that the spec names, and return `UnsupportedFormat` for the rest:
   ```rust
   match reader.format() {
       Some(image::ImageFormat::Tiff | image::ImageFormat::Bmp | image::ImageFormat::WebP) => {}
       _ => return Err(ImageFetchError::UnsupportedFormat),
   }
   ```
   Correct `design.md:95` and `implementation.md:77`. They imply that `spawn_blocking` isolates a decode failure. Under `panic = "abort"`, it does not.

6. **Nit.** `src-tauri/src/terminal/image_fetch.rs:132-137`.
   Expectation broken: the dimension pre-check should add protection. It does not. wezterm runs `check_image_dimensions` (the same `w*h*4 > 100_000_000` rule) before it places any iTerm2 image, so every registered image already passes. For larger native buffers, `max_alloc` rejects them in `load_inner` (`limits.reserve(total_bytes)`). For WebP, image 0.24 decodes the whole frame inside `WebPDecoder::new` (`codecs/webp/decoder.rs:131-136`), and `into_dimensions` builds that decoder. So the pre-check decodes a WebP fully and outside any limit, and then `decode()` decodes it again.
   Fix: remove the pre-check, and rely on wezterm's check and on `max_alloc`. Keep the over-limit test. It still passes through `reserve`.

7. **Nit.** `src-tauri/src/terminal/image_fetch.rs:140`, `:148`, `:111`, and `openspec/changes/terminal-inline-images/implementation.md:77`.
   Expectation broken: `implementation.md:77` says "`max_alloc` bounds memory". It bounds only the decoder's native output buffer. `into_rgba8` (`dynimage.rs:293-297`) allocates a second buffer of up to 100 MB for any non-RGBA8 source, such as RGB8 TIFF, which is chafa's case. `pack` then copies the RGBA into a third buffer while the second is still alive. The peak per decode is about 200 MB plus the decoder's working memory. The WebP decoding in the constructor is not bounded at all (finding 6).
   Fix: change the risk text to "about 2x the decoded size per decode, bounded in count by the decode semaphore" (finding 1). If you want to save one copy, reserve `HEADER_LEN` extra capacity in the RGBA `Vec`, push the header, and use `rotate_right(HEADER_LEN)`, or send the header as a trailer.

8. **Nit.** `openspec/changes/terminal-inline-images/specs/terminal-wezterm/spec.md:45` and `image_fetch.rs:140`.
   Expectation broken: the spec says that a decode above 100,000,000 bytes "of RGBA" is refused. The code refuses when the decoder's native buffer exceeds 100 MB. A 16-bit RGBA TIFF of 4000x4000 needs 128 MB native and 64 MB RGBA, so it fails, and so do float EXR/HDR images above about 6-8 MP. The safe behavior is the code's.
   Fix: change the spec to "refusing a decode whose decoder buffer or RGBA exceeds 100,000,000 bytes". You can also add a 16-bit case to the test.

9. **Nit.** `src-tauri/src/terminal/image_fetch_tests.rs:269`.
   Expectation broken: a test name must state what the test proves. `..._without_allocating_it` checks only that the call returns in less than 2 s. It does not measure allocation.
   Fix: rename it to `encoded_file_over_the_decode_limit_is_unsupported_quickly`, or drop the timing assertion and keep the result assertion.

10. **Nit.** `src/components/terminal/imageCache.ts:228-238`, `src-tauri/src/terminal/image_fetch.rs:110`.
    Expectation broken: CPU cost for each image view should stay low. Each fetch decodes again. `releaseAll` runs when the terminal is hidden, when the quake closes and when the session changes. A failed decode is retried two more times (`MAX_FETCH_RETRIES`). So a 100 MB TIFF decodes again each time the user opens the quake or comes back to the session, and a hostile file that fails decodes three times.
    Fix: accept this and write it down in D8, or keep the last 1-2 decoded payloads by hash in the backend registry under a small byte cap. Do not retry a fetch that failed with `unsupported image format`, because that result is final.

11. **Nit.** `src-tauri/tauri.conf.json:15-16`.
    Expectation broken: each CSP source must have a reason on the platforms it ships to. `http://ipc.localhost` is used only on Windows and Android (`core.js:16-17`). On Linux it is a real HTTP origin that resolves to loopback port 80. This is not a real widening, because main-document script already has the IPC. The spec scenario "no arbitrary connect-src origin" still holds.
    Fix: optional. Keep the source for the frozen Windows build and say why in D7, or move it to a `tauri.windows.conf.json` override.

12. **Nit.** `openspec/changes/terminal-inline-images/implementation.md:64`, `:77`, `openspec/changes/terminal-inline-images/design.md:95`.
    Expectation broken: the docs must match the code. `implementation.md:64` cites `image_fetch.rs:91-120` and `:122-126`. Those lines are now `:93-112` and `:152-157`. `design.md:95` says that the copy runs "under the image's own mutex", but the decode path now drops the mutex first. `implementation.md:77` says that "wezterm already parsed the same bytes for dimensions". In fact wezterm fully decodes downscaled non-PNG/GIF/WebP images (`iterm.rs:114`), and it fully decodes WebP even for dimensions.
    Fix: update the line references, state which path holds the mutex, and correct the wezterm statement.

13. **Nit.** `docs/shortcuts.md:139`.
    Expectation broken: each change in the delta must trace to the walk-1 fix. The vault path rename is not related to the walk-1 fix.
    Fix: commit it on its own.

## Summary

- Important: 2 (findings 1 and 2).
- Nit: 11.
- The IPC root cause and the CSP change are correct and narrow. They grant no new capability to frames or injected script.
- The new Rust decode path bounds memory for one image, but not across concurrent fetches. A PTY writer can turn small compressed files into many large concurrent decodes and crash the app.
- Top 3 priority fixes: (1) a decode semaphore in `terminal_image`, plus a fetch cap in `imageCache`; (2) JPEG and GIF kind-1 tests, then re-lock; (3) a runtime format allowlist, and removal of the WebP-doubling pre-check.

VERDICT: FAIL
