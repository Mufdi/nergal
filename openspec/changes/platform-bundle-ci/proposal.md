## Why

Nergal is Linux-only today: CI builds on a single `ubuntu-22.04` runner and ships `.deb`/`.rpm`/`.AppImage`. The multiplatform port scoping (2026-06-25) identified macOS as the first non-Linux target because the IPC seam change (Unix sockets → all platforms) is trivial and the runner/signing hurdle is lowest. This change adds macOS packaging and a CI build matrix so the project can ship to Apple Silicon Macs; Windows is scoped out of the executable tasks and handled in a later iteration.

## What Changes

- New `bundle.macOS` section in `src-tauri/tauri.conf.json` (minimumSystemVersion, category, DMG window size); existing `bundle.linux` unchanged.
- CI `release.yml` becomes a **matrix workflow**: parallel `build-linux` and `build-macos` jobs feed into a single `publish` job that assembles the multi-platform `latest.json` and creates the GitHub release.
- `scripts/generate-latest-json.mjs` generalised to merge platform fragments produced by each runner; new `scripts/merge-latest-json.mjs` helper in the publish job.
- `src-tauri/src/updater.rs`: `InstallSource` gains a `MacApp` variant; `detect_install_source()` gains a macOS path probe; `UpdateCheckResult` gains `dmg_asset_url`/`dmg_asset_size` fields; `check_app_update()` finds the `.dmg` asset for macOS users.
- macOS `.dmg` + `.app.tar.gz` + `.app.tar.gz.sig` published as release assets alongside the existing Linux artifacts.
- `latest.json` gains a `darwin-aarch64` platform entry so the Tauri updater can offer in-app update notifications on macOS.
- **Deferred (human gate)**: Apple code signing + notarization (requires $99/yr Apple Developer account + cert). Windows bundling and code signing. These are documented as a follow-up, NOT in this change's executable tasks.

## Capabilities

### New Capabilities

- `platform-distribution`: Cross-platform bundle configuration and release CI matrix. Covers macOS `bundle.macOS` config, the three-job CI strategy (build-linux / build-macos / publish), multi-platform `latest.json` assembly, macOS install-source detection in the updater, and the deferred signing gate. Windows is part of the long-term contract but out of scope for the executable tasks.

### Modified Capabilities

- `release-ci-signed`: The CI workflow structure changes from a single `build-linux` job to a three-job matrix. The `latest.json` manifest gains a macOS platform entry. The existing Linux build path, signing behavior, artifact names, and banner-update logic are unchanged.

## Impact

- **Config**: `src-tauri/tauri.conf.json` — `bundle.macOS` block added.
- **CI**: `.github/workflows/release.yml` — restructured into three jobs.
- **Scripts**: `scripts/generate-latest-json.mjs` — platform-fragment emission mode; new `scripts/merge-latest-json.mjs`.
- **Rust**: `src-tauri/src/updater.rs` — `InstallSource` enum, `detect_install_source`, `UpdateCheckResult`, `check_app_update`.
- **Frontend** (none): the About UI already branches on `InstallSource`; adding `MacApp` surfaces the same download-and-reveal flow already used for `.deb`, so no new frontend component is needed.
- **Dependencies**: no new Cargo crate or npm package required. macOS runner on GitHub Actions is available on the free/paid tiers without additional secrets.
- **Out of scope**: macOS code signing (entitlements, codesign, notarytool), Windows MSI/NSIS bundle, Flatpak/AUR/Snap, self-hosted runners, beta channels.
