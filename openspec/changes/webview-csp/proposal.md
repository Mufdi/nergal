## Why

The main webview has **no Content-Security-Policy**. `src-tauri/tauri.conf.json` has no `app.security.csp` key at all (verified), and Tauri's default when the key is absent is *no CSP*, not a safe default. The app renders react-markdown output from external sources (Linear/ClickUp/PR content) and has one raw-HTML sink (`ConfirmHost.tsx:79`, addressed by the sibling `shared-escape-html-util` change), and `capabilities/default.json` grants `opener:allow-open-url` / `shell:allow-open`.

A CSP is the standard containment layer for any HTML/script injection that reaches the webview — whether from an escaping regression, a future markdown-pipeline change, or a compromised dependency. Without one, a single injection has no additional barrier. This is defense-in-depth: it does not fix a specific injection, it bounds the blast radius of any that occurs.

## What Changes

- **Add a conservative CSP** under `app.security.csp` in `tauri.conf.json`, starting from `default-src 'self'` and explicitly allowlisting only the origins the app genuinely needs. Enumerate them first (below) so the policy is tight but does not break legitimate surfaces:
  - Tauri's asset/IPC scheme(s) for the bundled frontend.
  - `img-src` for any legitimately-remote images (e.g. the Linear image proxy already fetches server-side and hands back `data:` URLs — confirm whether the webview loads any remote image directly; if all go through the backend proxy, `img-src 'self' data:` suffices).
  - Font/style needs (fonts are bundled via `@fontsource*`, so `'self'`; confirm no inline-style requirement or add a hashed/`'unsafe-inline'` style exception only if unavoidable, documented).
  - **The in-app browser is a cross-origin `<iframe>` inside the MAIN webview** (`BrowserPanel.tsx:51`, `browser.rs:96-100`), so `default-src 'self'` with no `frame-src` breaks every browser tab. This change adds `frame-src http: https:` as a **deliberate, documented** allowance (framed origins run in their own origin and cannot script the app; `script-src`/`connect-src`/`default-src` stay `'self'`). Migrating the browser to a Tauri child webview to let `frame-src` tighten is recorded as a future follow-up (design Decision 4).
- **Verify against a BUILT bundle (not plain `pnpm tauri dev`)** — `app.security.csp` applies to production; dev uses `devCsp`. Set a mirrored `devCsp` or verify the built bundle so the walk validates the enforced policy (design Decision 5). Surfaces to walk: markdown renderers, confirm dialog, in-app browser tabs, external-link opening, updater.

## Capabilities

### New Capabilities

- `webview-content-security-policy` — the main webview enforces a Content-Security-Policy that defaults to `'self'` and allowlists only enumerated, justified origins, providing a containment layer against script/HTML injection.

## Impact

- **`src-tauri/tauri.conf.json`**: add `app.security.csp` with the enumerated policy.
- **Verification surfaces**: markdown-rendered Linear/ClickUp/PR content, `ConfirmHost` dialog, in-app browser panel (`browser.rs`), external-link opening (`opener`), updater. Each must be walked to confirm no regression.
- **Risk**: MED — an over-strict policy can break the in-app browser iframe or remote images; the enumerate-first step mitigates this. Ship after a manual walk of each surface.
- **Confidence**: HIGH that no CSP exists; the scope of what must be allowlisted is the work.
- **Out of scope**: the raw-HTML sink escaping (sibling `shared-escape-html-util`); the deep-link and fs-hardening changes. CSP complements them as the outer containment layer.
