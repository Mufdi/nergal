=== Round 1 (claude) ===
# Independent Review: Live Preview Browser Plan

## Issues

1. **CRITICAL — Capabilities permission names are inconsistent**. `spec.md` and `design.md` list `core:webview:allow-create`, `allow-navigate`, `allow-set-bounds`, etc. The spike.yaml step 2 lists completely different names: `allow-create-webview`, `allow-set-webview-position`, `allow-set-webview-size`, `allow-webview-show`, `allow-webview-hide`. One of these sets is wrong, and using the wrong names will fail `tauri-build`. The spec and design documents should not claim exact permission strings until the spike verifies them. Currently, readers cannot trust the capabilities section in either document.

2. **CRITICAL — Dual-state history tracking with no sync mechanism**. Task 1.1 defines `BrowserWebview` with a history field in Rust. Task 2.1 defines `browserHistoryAtom` in React. Neither document explains how these stay synchronized. When backend does `browser_back` (Task 1.2), does it emit an event to update the React atom? When React calls `browser_navigate`, does the response include the updated history? This is a classic dual-state consistency bug waiting to happen. The design must pick one source of truth or define the sync protocol.

3. **HIGH — No URL scheme validation**. Task 1.2 `browser_navigate` accepts any URL string. The design.md security section mentions "log de navegaciones" as mitigation but tasks.md doesn't implement logging. More importantly, there's no allowlist for schemes. A compromised agent or malicious prompt injection could navigate to `file:///etc/passwd` or `javascript:alert(document.cookie)`. Minimum mitigation: validate scheme is `http` or `https` only, or explicitly localhost-only by default.

4. **HIGH — Session switch mechanism is unspecified**. Design.md D3 says "Al cambiar de sesión → hide webview activo" but tasks.md defines no command or event for this. How does the Rust backend know the active session changed? The frontend has `activeSessionIdAtom` but there's no task wiring a listener to call `browser_set_visible(sessionId, false/true)` when the active session changes. Without this, all webviews stay visible (overlapping) or the hide never happens.

5. **HIGH — Fallback path is a dead end**. Spike step 7 says "FAILED → STOP, present iframe fallback proposal to user." But there is no iframe fallback proposal. Design.md D1 mentions "fallback a iframe con CSP-aware proxy en backend" but no spec, no tasks, no time estimate. If the spike fails, the user cannot make an informed decision because the alternative is undefined. At minimum, write a 1-paragraph summary of iframe approach + rough effort estimate before executing the spike.

6. **MEDIUM — Bbox sync misses position-only changes from scrolling**. Task 2.4 uses ResizeObserver + IntersectionObserver. ResizeObserver fires on size changes, IntersectionObserver fires on visibility threshold changes. Neither fires on position-only changes caused by scrolling a container. If the right panel is scrollable and user scrolls, the anchor div's position changes but neither observer fires. The webview will be misaligned until the next resize. Consider using a scroll listener on ancestor containers or a MutationObserver on transform/offset changes.

7. **MEDIUM — No guard against double webview creation**. Task 2.4 says "First mount: `browser_create_webview` si no existe." React StrictMode (enabled in main.tsx per project structure) intentionally double-mounts components. Fast tab switching could also trigger multiple mount cycles. The task should explicitly check `BrowserState` before creating and return early or return the existing webview handle. Otherwise, you'll get orphaned webviews or errors.

8. **MEDIUM — Port scanner lacks debounce and chip flicker mitigation**. Design.md D4 says the scanner runs every 3s. A flaky service (e.g., a server restarting) could cause a port to flap: active → inactive → active within seconds. The StatusBar chips will appear/disappear/reappear, which is visually jarring. Add a debounce (e.g., port must be inactive for 2 consecutive scans before removal) to smooth the UX. This is explicitly mentioned in the reviewer focus areas but not addressed in design or tasks.

9. **MEDIUM — No automated tests for new functionality**. Phase 1 and 2 introduce ~10 new Tauri commands and several atoms. The verification section says `cargo test` but no new tests are written per tasks.md. For a "medium risk" feature touching IPC, capabilities, and a tokio task, there should be at least: unit tests for port scanner logic, unit test for URL validation (once added), and a test that `BrowserState` correctly tracks create/destroy lifecycle. Manual testing alone is insufficient for regression prevention.

10. **LOW — Cmd+L scope ambiguity with floating mode**. Shortcut spec says "cuando browser tab está activo" but floating mode means the browser can be visible while another tab is active. Should Cmd+L work when floating browser is visible but the active tab is `git`? The scenario "Cmd+L is scoped to browser" implies no, but that's confusing UX—user sees the floating browser, presses Cmd+L, nothing happens. Clarify the expected behavior.

11. **LOW — Spike cleanup not enforced**. Spike step 8 says "User decides whether to keep as reference or revert." The temporary spike code (`spike_browser.rs`, App.tsx button) should be explicitly deleted before Phase 1 to avoid shipping debug artifacts. Add a cleanup task at Phase 1 start.

12. **LOW — DevTools toggle condition is incomplete**. Spec says "in production builds the toggle SHALL be hidden." Task 2.3 says "DevTools toggle (dev builds only)" but doesn't specify the mechanism. Is this a compile-time cfg flag, a runtime config check, or Tauri's debug_assertions? The implementation could accidentally ship devtools to production if the gating is unclear.

---

VERDICT: REVISE


=== Round 2 (claude) ===
# Round 2 Independent Review

## Resolution status of Round 1 issues

| # | Severity | Issue | Resolution | Status |
|---|----------|-------|------------|--------|
| 1 | CRIT | Capability name inconsistency | D7 + spec "Permissions match spike outcome": design/spec deliberately no longer claim exact strings; spike records authoritative list in `handoff/spike-outcome.md` | ✅ |
| 2 | CRIT | Dual-state history sync | D8 makes backend canonical; `browser:history-changed` event carries `current_url`, `can_back`, `can_forward`; `did-navigate` callback handles in-page link clicks | ✅ |
| 3 | HIGH | URL scheme validation | `valid_url_scheme` helper rejects `file://`, `javascript:`, `data:`, `chrome://`; allows http/https/about:blank; navigation logging via `tracing::info!`; covered by `tests/browser_url_validation.rs` | ✅ |
| 4 | HIGH | Session switch wiring | D9 + Task 2.6 add explicit `useEffect` on `activeSessionIdAtom` calling `browser_set_visible(prev,false)`/`(curr,true)` | ✅ |
| 5 | HIGH | Fallback dead end | `handoff/fallback-iframe.md` documents iframe + localhost proxy, +4-5h estimate, explicit loss list (design mode, host devtools, per-origin cookies, WS upgrade), decision criteria, migration path | ✅ |
| 6 | MED | Bbox scroll sync | D11 adds capture-phase scroll listener on `window` + rAF poll comparing `getBoundingClientRect`; Task 2.4 references both | ✅ |
| 7 | MED | Idempotent create | D10 + Task 1.2: `entry().or_insert_with` covers StrictMode + races | ✅ |
| 8 | MED | Port flap debounce | D12 hysteresis: 1-scan add / 2-scan remove; spec has explicit "Port flap absorbed" scenario; covered by `tests/port_scanner_hysteresis.rs` | ✅ |
| 9 | MED | Automated tests | Task 1.6 adds 3 unit tests (URL validation, idempotent state lifecycle, hysteresis) | ✅ |
| 10 | LOW | Cmd+L scope | Task 5.1 + 3 spec scenarios clarify "browser visible" scope (dock active OR floating open) | ✅ |
| 11 | LOW | Spike cleanup | Task 1.0 added at top of Phase 1 — revert spike artifacts before continuing | ✅ |
| 12 | LOW | DevTools gating | D7 + Task 2.8 + spec scenarios specify double gate: `cfg!(debug_assertions)` (Rust runtime) + `import.meta.env.DEV` (compile-time tree-shake) | ✅ |

All 12 issues addressed substantively (not just acknowledged).

## New gaps introduced — minor

- **Mutex choice unspecified** (`Arc<Mutex<HashMap<...>>>` in 1.1): if `std::sync::Mutex`, async commands holding the guard across await points would block the tokio runtime. Use `tokio::sync::Mutex` or `parking_lot::Mutex` (no awaits while held). Worth a one-line note in 1.1; not blocking — implementer will catch via clippy or first await.
- **rAF poll battery drain**: D11 mentions "while browser tab is active" but doesn't specify the stop condition explicitly. Implied by mount/unmount; acceptable.
- **Navigation logging may include sensitive URLs**: `tracing::info!` of full URL could capture query-param tokens in dev-server preview redirects. Low risk for personal desktop app; flag for future redaction if multi-user scope ever appears.
- **`did-navigate` API existence**: explicitly listed in spike open question #3 — appropriately deferred to spike. ✅
- **Mode-switch UX of dock tab**: when user toggles to floating, plan doesn't say if the dock tab disappears, becomes empty, or shows "rendered in floating window" placeholder. Minor — implementer can pick a sensible default; not a correctness issue.

None of these rise to material risk; all are reversible UI choices or implementation hygiene that lint/test catches.

## Strengths of the revision

- D8/D9/D10/D11/D12 are clean orthogonal additions, not retrofits.
- Spec scenarios actually trace the new requirements (hysteresis, dual gate, in-page nav, scope clarity), so verification is testable.
- Fallback document has real decision criteria, not boilerplate.
- Spike outcome path is now load-bearing for capability strings rather than authoritative-by-guess.

VERDICT: APPROVED

