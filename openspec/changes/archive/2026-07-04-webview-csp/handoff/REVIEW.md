# REVIEW — webview-csp

## Reviewer: security-reviewer (single, sonnet) · 2026-07-04

**Verdict: PASS** — 0 blocking findings. `cargo check` parses the config.

- **script-src 'self'** — no unsafe-inline/eval (the load-bearing containment directive).
- **default-src/connect-src 'self'** — walked the frontend: the only non-invoke `fetch()`
  (`NergalLogo.tsx` growl sound) is a Vite-bundled asset ('self'); no WebSocket/EventSource/
  XHR; the updater + Tauri commands + opener/shell run Rust-side (not webview), so
  `connect-src 'self'` breaks nothing.
- **style-src 'unsafe-inline'** — style-only (D3, React inline styles + index.html `<style>`).
- **frame-src http: https:** — the accepted browser-iframe cost (D4).
- **img-src 'self' data: https:** — the user's decided policy. Reviewer confirmed the
  design doc's Decision 2 (`data:`-only "likely suffices") UNDERSELLS reality — Linear/
  ClickUp avatars + attachment thumbnails render `<img src={remoteUrl}>` directly (variable
  hosts, no proxy), so the shipped `https:` value is what's actually needed. Applied config
  correct; the design narrative was narrow. No action.
- **Enumeration complete**: fonts all bundled ('self'), agent icons bundled, confirm sink
  contained by script-src, no surface loads from an un-allowed origin.

## Non-blocking recommendation → APPLIED

Reviewer recommended `base-uri 'self'; form-action 'self'` (they don't fall back to
default-src; a missing base-uri lets an injected `<base>` hijack relative URLs, a missing
form-action lets an injected `<form>` POST anywhere). Zero `<form>`/`<base>` in the code
today, so free hardening. **Applied to both csp + devCsp**; `cargo check` re-parsed clean.

## Cross-engine (D6) — UNVERIFIED, honestly recorded

The per-surface walk against a BUILT bundle, repeated on WebKitGTK/WKWebView/WebView2, is
inexecutable from this session (needs the built app on 3 OSes). tasks 3.x/4.2-4.4 remain
the final gate before the user considers any platform shipped — NOT claimed done. Reviewer
confirmed the docs record this honestly (no "works on Linux ⇒ shipped").

## Gates

- Gate 1 (cargo check): PASS (config parses with the hardening). `pnpm vite build` green.
- Gate 4 (security): the change IS the containment layer; security-reviewer ran, policy
  validated tight, hardening added.
- Gate 3/4.x (surface walk, cross-engine): pending manual walk (documented).
