## Why

OpenSpec change `release-script` (Change A) automated the deterministic half of the release flow — versions, Cargo.lock, commit, tag, push. After the tag pushes, the user still does 3 manual things: run `pnpm tauri build` (~3-5 min), `gh release create v<X> --title ... --notes-file <body.md> <artifacts>`, and `gh release edit v<previous>` to add a banner. The user has been doing these manually across v0.1.1, v0.1.2, and v0.1.3 — the friction is known.

The wider goal is **hands-off release**: after `pnpm release <bump>` pushes the tag, GitHub Actions takes over and the user is done. This unlocks: (a) ship from any machine with push access, not just the dev's Linux box; (b) consistent build environment (no host-deps drift); (c) **signed AppImage bundles** enabling `tauri-plugin-updater` for in-app auto-updates. The `.deb` path stays manual download per Linux package-manager convention, but AppImage users get update notifications from the app itself.

The pending signing follow-up (`memory/project_v0_1_4_appimage_signing.md` — user authorized key generation post-v0.1.3) overlaps fully with the signing piece of this change and gets absorbed.

## What Changes

- New `.github/workflows/release.yml` triggered on `v*` tags:
  - Builds on `ubuntu-22.04` (WebKitGTK 4.1)
  - Installs system deps (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`)
  - Caches `~/.cargo/registry`, `~/.cargo/git`, `src-tauri/target` keyed on `Cargo.lock`
  - `pnpm install --frozen-lockfile` + `pnpm tauri build` with signing env vars
  - Bundles signed via Tauri's built-in signer (`TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from GH repo secrets)
  - Extracts release body from `CHANGELOG.md` section for the current tag (via new `scripts/extract-release-body.mjs`)
  - `gh release create <tag>` with `.deb`, `.rpm`, `.AppImage`, and `.AppImage.sig` as assets
  - Updates the previous release's body to prepend a "Latest release →" banner (template at `scripts/release-banner.tpl.md`)
- `tauri-plugin-updater` integrated in the app:
  - Added to `src-tauri/Cargo.toml`
  - Initialized in `src-tauri/src/lib.rs::run()`
  - Configured in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey` + endpoint pattern, `dialog: false` since the v0.1.3 About Update button is the UX surface)
- New helpers in `scripts/`:
  - `extract-release-body.mjs` — pure helper, reuses `extractChangelogSection` from `release.mjs`
  - `update-previous-banner.mjs` — `gh api` based, strips any existing banner before prepending the new one (no stacking)
  - `release-banner.tpl.md` — template content
- One-time prereqs (user-driven, documented; NOT scripted because key generation is interactive):
  - `pnpm tauri signer generate -w ~/.tauri/cluihud-updater.key`
  - Paste pubkey into `tauri.conf.json`
  - Add `TAURI_SIGNING_PRIVATE_KEY` (file contents) + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to GH repo secrets
- `scripts/release.mjs` final stdout message updated: instead of "Build + GH release remain manual", it says "CI workflow will now build, sign, and publish. Monitor at https://github.com/Mufdi/nergal/actions". Small modification to Change A's spec.

## Capabilities

### New Capabilities

- `release-ci-signed`: CI workflow triggered by tag push; signed bundle artifacts; release body extracted from CHANGELOG; banner update of previous release; app-side `tauri-plugin-updater` integration for auto-update verification.

### Modified Capabilities

- `release-script`: the "Out-of-scope operations explicitly deferred to Change B" requirement is reformulated — operations are now handled by `release-ci-signed`, not deferred. The script's final stdout message changes from manual-steps reminder to a CI-handoff message.

## Impact

- **New files**:
  - `.github/workflows/release.yml`
  - `scripts/extract-release-body.mjs`
  - `scripts/extract-release-body.test.mjs`
  - `scripts/update-previous-banner.mjs`
  - `scripts/release-banner.tpl.md`
- **Modified files (one-time)**:
  - `src-tauri/Cargo.toml` (add `tauri-plugin-updater` dep)
  - `src-tauri/Cargo.lock` (refresh)
  - `src-tauri/src/lib.rs` (init updater plugin in `run()`)
  - `src-tauri/tauri.conf.json` (add `plugins.updater` config + pubkey)
  - `scripts/release.mjs` (refactor `extractChangelogSection` for reuse; update final stdout message)
  - `package.json` (add `release:body` and `release:banner` scripts)
  - `CLAUDE.md` (cluihud) — "Release commands" section updated: post-script is automated; add "First-time signing key setup" + "Key rotation" subsections
  - `Bug Workflow.md` (vault) — step 6 final note updated (post-script = wait for CI)
  - `bug-workflow/SKILL.md` — corresponding rule update
- **External configuration (one-time, user-driven, NOT in repo)**:
  - GitHub repo secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  - Local key file: `~/.tauri/cluihud-updater.key` (NEVER committed)
- **Documentation cleanup on archive**:
  - `Projects/nergal/follow-ups/release-automation.md` (pending from Change A) → `Records/Projects/nergal/release-automation follow-up.md` with frontmatter and closing note linking BOTH this change AND Change A.
  - `memory/project_v0_1_4_appimage_signing.md` → update with final outcome (key location, integration shape) OR delete if fully absorbed by this change's record.
- **Out of scope**:
  - Non-Linux platforms (Windows, macOS) — would need separate jobs in a build matrix
  - Flatpak / AUR / Snap publishing — separate Change C if needed someday
  - Self-hosted runners (using GitHub-hosted ubuntu-22.04)
  - Beta / preview release channels (single stable channel only)
  - Notarization for the AppImage (Tauri signing is sufficient; no Apple notarization needed on Linux)
- **Risk surface**:
  - CI breakage on tag push leaves the tag on origin but no GH release. User inspects logs, can retry by deleting tag + re-running `pnpm release` with the same explicit version (`pnpm release 0.1.4` — re-uses the section already in CHANGELOG).
  - Signing key compromise: if `TAURI_SIGNING_PRIVATE_KEY` leaks, an attacker could publish "valid" signed updates. Mitigation: key stored only in encrypted GH secrets, never in repo. Rotation procedure documented in CLAUDE.md (regenerate key, replace secret, push new pubkey via a version bump — old AppImages can't verify new updates after rotation, expected).
  - `tauri-plugin-updater` behavior: configured with `dialog: false` so the existing About Update button drives UX. No second prompt, no auto-restart, no behavior regression vs v0.1.3.
  - First CI run cost (full build, no cache): ~10-15 min. Subsequent runs with cache: ~5-7 min. Acceptable for a release cadence of 1-2 per week.
