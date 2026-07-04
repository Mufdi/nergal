## 1. Enumerate legitimate origins (do first)

- [x] 1.1 Walk each surface and record what it loads: bundled asset/IPC scheme, `data:` images (confirm the Linear proxy is the only image path), bundled fonts (`@fontsource*`), the updater endpoint, external-link opening, the in-app browser panel.
- [x] 1.2 Note any inline-style requirement from Tailwind/React that would need a `style-src` exception (hashed or documented `'unsafe-inline'` for style only — never script).

## 2. Add the CSP

- [x] 2.1 Add `app.security.csp` to `tauri.conf.json`: `default-src 'self'`, `script-src 'self'`, `connect-src 'self'`, plus the enumerated `img-src`/`font-src`/`style-src` entries proven necessary, and `frame-src http: https:` for the in-app browser (design Decision 4). Justifications live in design/proposal — `tauri.conf.json` is JSON and takes NO inline comments.
- [x] 2.2 Set a mirrored `devCsp` (or plan verification against a built bundle) so the policy is actually enforced during the walk (design Decision 5).

## 3. Verify each surface (no regression) — against the enforced policy

- [ ] 3.1 Markdown-rendered Linear/ClickUp/PR content renders.
- [ ] 3.2 Confirm dialog renders.
- [ ] 3.3 In-app browser tabs (cross-origin `<iframe>`) load under `frame-src http: https:`.
- [ ] 3.4 External-link opening works; updater flow works.

## 4. Verification (cross-engine — CLAUDE.md invariant)

- [x] 4.1 `cd src-tauri && cargo check` (config parses) + app boots.
- [ ] 4.2 Walk all surfaces in section 3 against a **built bundle** (or the mirrored `devCsp`), NOT plain `pnpm tauri dev`; devtools shows no CSP violation on legitimate actions (especially browser tabs).
- [ ] 4.3 Confirm a non-allowlisted `connect-src`/`script-src` origin is blocked (spot check via devtools), and that a framed `https:` site cannot reach app `connect-src`.
- [ ] 4.4 Repeat the surface walk on **each webview engine** (WebKitGTK/Linux, WKWebView/macOS, WebView2/Windows) — CSP enforcement + `frame-src` iframe behavior differ per engine (design Decision 6). Where an OS walk is not yet possible, record it explicitly as unverified — no "works on Linux ⇒ shipped".

## Notes (orchestrator)

- img-src policy = `'self' data: https:` (user-decided; scheme-wide https for images, zero
  regression — remote avatars/thumbnails/markdown images already load from variable hosts).
  Follow-up to tighten: retrofit `gateRemoteImages` onto ClickUp/Plan/Spec/Obsidian, then
  `img-src 'self' data:`.
- Hardening added beyond the proposal: `base-uri 'self'; form-action 'self'` (security
  reviewer rec; free — no <form>/<base> today).
- Separate user request (commit 172adb7, not this change): Linear images auto-load
  (dropped the click-to-reveal gate; uploads.linear.app proxy preserved).
- Tasks 3.1-3.4 (surface walk) + 4.2-4.4 (built-bundle, cross-engine WebKitGTK/WKWebView/
  WebView2) remain UNVERIFIED — manual walk is the final ship gate per D6.
