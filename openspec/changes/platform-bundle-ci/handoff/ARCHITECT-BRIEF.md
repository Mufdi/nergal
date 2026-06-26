# Architect Brief — platform-bundle-ci

**Project mission**: Nergal — Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. **Now porting to macOS + Windows, macOS first.**

## Control metadata
- **Tier** L · **ceremony** deep · **risk** medium · **files** ~5 · **visibility** public
- **Capabilities**: `platform-distribution` (new) + `release-ci-signed` (modified delta)
- **Port sequence**: 5 of 5 — **last; depends on the crate actually working on macOS.**

## What this change owns
Cross-platform packaging + release CI: `bundle.macOS` (`.dmg`/`.app`/`.app.tar.gz.sig`) in `tauri.conf.json`, a CI build matrix (ubuntu + macos runners), a multi-target `latest.json`, and updater macOS asset resolution (`InstallSource::MacApp` + `.app/Contents/MacOS/` probe + `dmg_asset_url`). `darwin-aarch64` only for iteration 1.

## ⚠️ Human gate — signing deferred
The user has **no Apple Developer account and no Windows code-signing cert**. macOS bundles ship **UNSIGNED** (Gatekeeper warning on first open; documented workaround). The Tauri updater key still signs update artifacts for integrity. **Apple notarization ($99/yr) + Windows signing are a human-gated follow-up** — out of this change's executable scope. Everything else in C ships without them.

## Dependencies & sequencing
- **Depends on**: platform-compile + platform-proc + platform-desktop + platform-ipc (the app must run on macOS before you package it).
- **Blocks**: nothing — terminal node of the port.

## Gating decision (Mode B)
- iprev **triaged** (L + deep + files ≥ 5) → recommended. Reviewer: single-sequential (deps-reviewer if Cargo/CI deps shift). Gates: compile + test + lint.

## Reference skills (A6)
- `tauri-app-dev` — bundling/distribution + auto-update config for macOS targets.
- Prior art: archived changes `release-script` + `release-ci-signed` (the existing Linux pipeline this extends).

## Constraint
Linux release path must remain unchanged. Acceptance = `.dmg`/`.app` builds on a macOS runner, `latest.json` includes the macOS target, the updater resolves it. Unsigned is acceptable for this change.
