## Why

The `webview-csp` change shipped with a `style-src 'self' 'unsafe-inline'` policy that intentionally granted `'unsafe-inline'` for runtime-injected library styles. But `index.html` carried an inline `<style>` splash block, and Tauri auto-nonces every `<style>` element it finds — injecting a nonce-source into `style-src`, which per CSP spec **voids `'unsafe-inline'`** for that directive. This silently blocked every non-nonced runtime-injected stylesheet: Sileo toasts (raw unstyled text) and CodeMirror (file + scratchpad editors rendered as unusable non-editable rows). The `webview-csp` REVIEW.md had flagged the cross-engine check as UNVERIFIED (walk deferred); this regression is what that walk would have caught. The code fix (commit `fa01519`) removed the inline `<style>`; this change records the invariant in the spec so a future inline `<style>`/`<script>` in `index.html` does not silently reintroduce it.

## What Changes

- Broaden the CSP requirement's non-regression list to include toasts (Sileo) and the code editors (CodeMirror).
- Add the invariant: `index.html` (the shipped HTML Tauri scans) SHALL contain **no inline `<style>` or `<script>` elements**, because Tauri auto-nonces them and a nonce-source in a directive voids `'unsafe-inline'` for it — breaking runtime-injected library styles. Splash/critical CSS lives in the bundled external stylesheet instead.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `webview-content-security-policy`: add the no-inline-`<style>`/`<script>`-in-`index.html` invariant and extend the non-regression surface list.

## Impact

- Documentation-only (the code fix already landed in `fa01519`). No code changes in this change.
- **Out of scope**: the CSP policy string itself (unchanged); nonce-plumbing to libraries (rejected — the no-inline-element approach keeps `'unsafe-inline'` effective without per-library nonce work).
