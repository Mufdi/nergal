## Context

Tauri does not apply a CSP unless `app.security.csp` is set; the key is absent, so the main webview runs with none. The app renders external markdown, has a raw-HTML confirm sink, and grants opener/shell-open capabilities. A CSP is the standard outer containment layer for injection that reaches the webview. The work is not "write a CSP" — it is enumerating exactly what the app legitimately loads so the policy is tight without breaking a feature.

## Goals

- A default-`'self'` policy that contains injection.
- Zero regression to legitimate surfaces (markdown, confirm dialog, in-app browser, external links, updater).
- A policy that is justified origin-by-origin, not a permissive catch-all.

## Decision 1: Enumerate-first, then tighten

**Chosen**: walk each surface and record what it loads (asset scheme, `data:` images, bundled fonts, updater endpoint) before writing the policy; start from `default-src 'self'` and add only what the walk proves is needed.

- **Alternative**: ship a known-good broad policy and loosen on breakage — rejected: a broad policy defeats the purpose; the enumerate-first path yields the tightest correct policy.

## Decision 2: Remote images

The Linear image proxy fetches server-side and returns `data:` URLs (`linear/client.rs` SSRF-guarded), so the webview likely needs only `img-src 'self' data:`. **Verify** no surface loads a remote image directly before finalizing; if one does, allowlist that exact host, not a wildcard.

## Decision 3: Inline styles / fonts

Fonts are bundled (`@fontsource*`), so `font-src 'self'`. Tailwind/React may inject inline styles; if `style-src 'self'` breaks rendering, prefer hashed styles or a narrowly-documented `'unsafe-inline'` for `style-src` only (never `script-src`), with the reason recorded.

## Decision 4: In-app browser is a cross-origin `<iframe>` in the MAIN webview — `frame-src` must be decided here (corrected in iprev round 1)

**Correction**: the first draft assumed the in-app browser "uses its own webview" and the main CSP "should not need to permit arbitrary origins for it." That is **false** — `BrowserPanel.tsx:51` renders an `<iframe src=…>` per tab *inside the main webview*, and `browser.rs:96-100` documents the "cross-origin iframe focus trap." Under `default-src 'self'` with no `frame-src`, **every browser tab breaks** (arbitrary `http:`/`https:` origins are blocked from framing).

This is a **design decision, not a walk-time discovery**. Two viable directions:
- **(a) Allow `frame-src http: https:`** — keeps the in-app browser working with a one-line addition, but materially widens the policy (any origin may be framed). Because framed content runs in its own origin (not `'self'`) and cannot script the app under the same-origin policy, this is an acceptable, bounded relaxation for a *browser* feature — but it must be a conscious, documented allowance, not an accident. `script-src`/`connect-src`/`default-src` stay `'self'`; only `frame-src` opens.
- **(b) Migrate the browser panel to a Tauri child webview** (out of the main document) — keeps the main CSP tight (`frame-src 'none'`/`'self'`), but is a non-trivial rework of `BrowserPanel`/`browser.rs` not budgeted in this change.

**Chosen for this change**: direction (a) — `frame-src http: https:` with `default-src/script-src/connect-src 'self'` — documented as the deliberate cost of shipping the CSP now without a browser rework. Direction (b) is recorded as a future follow-up that would let `frame-src` tighten. The tier/ceremony reflect that this is a real decision, not a config typo.

## Decision 5: dev vs. production CSP + JSON has no comments (iprev round 1)

- `app.security.csp` applies to the **production** build; `pnpm tauri dev` uses `devCsp` (or none). Verification MUST run against a built bundle, or set a mirrored `devCsp` so the dev walk validates the real policy — otherwise the walk passes against an unenforced policy.
- `tauri.conf.json` is JSON and cannot carry comments. The per-entry justification lives in **this design doc / the proposal**, not as an inline JSON comment. Task wording corrected accordingly.

## Decision 6: Cross-platform CSP enforcement (CLAUDE.md invariant)

`app.security.csp` is one config value Tauri applies on every platform, but the **enforcing engine differs** — WebKitGTK (Linux), WKWebView (macOS), WebView2/Chromium (Windows) — and they are not identical in how strictly they enforce a directive, how they report violations, and how a cross-origin `<iframe>` behaves under `frame-src`. A policy that passes the Linux walk is NOT proven on macOS/Windows.

**Chosen**: treat the policy as cross-platform surface. The enumerate-first walk and the per-surface verification (especially the in-app browser iframe under `frame-src`) MUST be repeated on all three engines before the change is considered done — or, where a walk on a given OS is not yet possible, the change explicitly records that platform as unverified (no silent "works on Linux ⇒ shipped"). This is config, not `#[cfg]`-gated code, so there is no compile seam — the invariant here is *verification coverage across engines*, not conditional compilation.

- **Note**: this is the only one of the audit's security changes whose behavior is webview-engine-dependent; the others (fs guard, escaping, context boundary) are engine-agnostic Rust/TS.

## Risks

- **Over-strict policy breaks a surface** (MED): mitigated by enumerate-first + the explicit `frame-src` decision above + a per-surface check against a built bundle (not dev). This is why the change is `standard` ceremony.
- **`frame-src http: https:` widens the policy** (accepted, direction (a)): bounded — framed origins cannot script the app; documented as deliberate; direction (b) tightens it later.
- **CSP is not a fix for injection, only containment** (accepted): paired with `shared-escape-html-util` (removes the known unescaped sink) and the markdown pipeline already not using `rehype-raw`.

## Migration / rollout

Config-only change. Verify against a **built bundle** (or a mirrored `devCsp`), not plain dev. No schema/data migration. Reversible (remove/relax the key) if a surface regresses, though the enumerate-first + framed-decision should prevent it.
