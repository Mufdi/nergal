# Spike outcome — Tauri secondary WebView (Phase 0)

> **Status: COMPLETE — pivot decision applied**.
> **Verdict: PARTIAL → architectural pivot to iframe.**

## Summary

The Phase 0 spike validated whether Tauri 2's `unstable` secondary WebView API could host the live-preview browser as a child of cluihud's main window. The compile-time path validated cleanly. The visual validation (user running `pnpm tauri dev` and exercising the spike control panel) revealed two **bloqueante** issues that made the secondary WebView unsuitable for cluihud's panel-in-dock requirement.

The change pivoted to **iframe-in-React-panel** during Mode B execution. All proposal/design/tasks/spec docs have been rewritten accordingly.

## Findings (compile-time)

### F1. `unstable` feature flag required

Tauri 2.10.3 gates the multi-webview API behind `cfg(feature = "unstable")`:
- `tauri::WebviewBuilder` → unstable
- `tauri::Window::add_child` → unstable + desktop
- `tauri::Manager::get_window`, `windows`, `get_webview` → unstable

Was applied during spike (`features = ["unstable"]` in Cargo.toml). **Reverted post-pivot** — iframe approach does not need it.

### F2. API surface is `Window`, not `WebviewWindow`

`add_child` lives on `Window` (gated), not `WebviewWindow`. Use `app.get_window("main")` (also gated) → `Window::add_child(builder, position, size)`. Reverted post-pivot.

### F3. No `core:webview:*` capabilities required

Custom Tauri commands wrapping the unstable Rust API do **not** require `core:webview:*` permissions in `default.json`. This invalidated Round-1 iprev finding #1 about exact permission strings — the strings were never part of our IPC surface.

This finding **carries through to the iframe pivot**: still no Tauri capabilities needed.

### F4. `url` crate added

Added `url = "2"` to Cargo.toml for `WebviewUrl::External(url::Url)` in spike code. **Kept post-pivot** for `validate_url` in the new `src-tauri/src/browser.rs`.

## Findings (visual — user-validated)

User started `pnpm tauri dev`, started `python3 -m http.server 5173`, clicked the SPIKE panel buttons.

### F5. 🔴 BLOQUEANTE — Webview captures keyboard focus

**Observed**: with the secondary WebView present, **all cluihud shortcuts stop working**:
- Cmd+K command palette does not open
- ctrl+shift+0 expand panel does not respond
- alt+left/right module navigation broken
- No React UI keyboard input reaches the main webview

**Root cause**: WebKitGTK secondary webview steals window-level focus. The Tauri main webview's React UI loses keyboard event delivery whenever the child webview is rendered.

**Mitigation considered**: programmatic focus management via `WebviewWindow::set_focus` on every keypress event — but this requires intercepting raw window events at the Tauri runtime level, which is itself an unstable area, and would be a significant ongoing maintenance cost. Rejected.

### F6. 🔴 BLOQUEANTE — Z-order: webview always above React UI

**Observed**: the secondary WebView renders above any React element within its bounding box. React modals (command palette, dropdowns, tooltips) appear behind the webview.

**Root cause**: secondary WebView is a native widget (WebKitGTK GtkBox child) layered above the GTK main webview by the windowing system. Pure-DOM z-index has no effect across this boundary.

**Mitigation considered**: programmatic hide-on-modal — listen for "modal-opened" atom changes and call `webview.hide()` for each one. Rejected: brittle, requires listing every potential modal source, jarring UX (browser disappears every time user opens any palette/dropdown).

### F7. ⚠️ Bug — bbox sync commands accepted but not visually applied

**Observed**: `spike_set_webview_bounds` returns `Ok(())` but the webview doesn't move/resize visually. Move/resize buttons (+x, +y, +w, +h) appear to no-op.

**Root cause** (suspected, not deeply diagnosed since blocked by F5/F6): possible LogicalPosition vs PhysicalPosition mismatch on HiDPI scale, or unstable runtime not propagating coords correctly to WebKitGTK on Linux.

**Decision**: not investigating further — F5+F6 already disqualify the approach.

### F8. ⚠️ DevTools open as separate window placed by WM

**Observed**: clicking "devtools" opens an inspector window outside the cluihud window, positioned by the GTK window manager (often below or beside).

**Root cause**: Tauri's `webview.open_devtools()` for secondary WebView in WebKitGTK uses the WebKit inspector which is always a separate top-level window.

**Acceptability**: not blocking by itself, but compounds the integration issues.

### F9. Other observed issues

- The user's `localhost:5173` was already occupied by Vite (cluihud's own frontend dev server), causing the iframe to load cluihud's frontend recursively ("loading cluihud..." in the webview area). Confirmed the webview did load — just the wrong target. Not a finding against the spike, just a setup nuance.
- `top -p $(pgrep cluihud)` failed because `pgrep` returned multiple PIDs separated by newlines, breaking top's `-p` arg. Use `top -p $(pgrep -d, cluihud)` instead. Trivial.

## Decision

**Pivot to iframe-in-React-panel.**

Rationale:
- F5 (focus capture) and F6 (z-order) are architecturally fundamental to WebKitGTK secondary WebView. Workarounds exist but are brittle and high maintenance for a personal-scale project.
- iframe natively avoids both issues: scoped DOM focus, normal z-order with React modals.
- For cluihud's actual use case (localhost dev preview integrated to a right panel like diff/git/plan), iframe is the simpler and more correct primitive.
- X-Frame-Options is rarely a problem with localhost dev servers (Vite, Next, Astro, Python http.server, Django, Flask). If it bites in practice, follow-up `browser-csp-proxy` adds a Rust proxy that strips the headers.
- Design mode (Orca-style click-element → snippet) was already deferred and is now **completely descartado** per user decision. Cross-origin same-origin policy makes it impossible without alternate architectures.

## Reverts post-pivot

The following spike artifacts were removed before Phase 1:
- `src-tauri/src/spike_browser.rs` (deleted)
- `mod spike_browser` and 4 invoke_handler entries (removed from `lib.rs`)
- `tauri = features = ["unstable"]` (reverted to `features = []` in Cargo.toml)
- `SpikeBrowserPanel` component + `useState` import in `App.tsx` (reverted)
- `openspec/spikes/tauri-secondary-webview.yaml` (deleted — spike completed, no longer needed as YAML)
- `openspec/changes/live-preview-browser/handoff/fallback-iframe.md` (deleted — content rolled into design.md as the now-primary path)

Kept post-pivot:
- `url = "2"` in Cargo.toml (still useful for `validate_url` in iframe path)

## Knock-on effects on the design

Original (pre-spike) Round-1 iprev critical findings revisited:
- #1 (capabilities permission strings inconsistency) → moot, no capabilities needed.
- #2 (dual-state history sync) → moot, React owns history (no backend webview state).
- #3 (URL scheme validation) → still applicable, `validate_url` in `browser.rs`.
- #4 (session switch wiring) → simplified, React lifecycle handles it (no webview hide/show).
- #5 (fallback dead end) → resolved differently — iframe became primary, proxy variant deferred to follow-up.
- #6-#12 (bbox sync, idempotent create, etc.) → most moot in iframe path; what remains (port hysteresis, devtools gating, Cmd+L scope) preserved in updated docs.
