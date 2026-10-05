# platform-distribution Specification

## Purpose
TBD - created by archiving change platform-bundle-ci. Update Purpose after archive.
## Requirements
### Requirement: macOS bundle configuration declares a valid .app/.dmg target

`src-tauri/tauri.conf.json` SHALL contain a `bundle.macOS` section specifying `minimumSystemVersion` (≥ "12.0"). It SHALL NOT contain a `category` key: the Tauri 2 `MacConfig` schema is `additionalProperties: false` and has no `category` member, so adding one fails config-schema validation and breaks `pnpm tauri build` on every platform (including the Linux release path). The macOS app category SHALL continue to derive from the existing top-level `bundle.category`. The `bundle.targets` value "all" SHALL remain, allowing Tauri to build macOS targets on a macOS runner without a separate targets override. The existing `bundle.linux` section SHALL be preserved unchanged.

#### Scenario: macOS section present and valid
- **WHEN** `src-tauri/tauri.conf.json` is read on a macOS CI runner running `pnpm tauri build`
- **THEN** Tauri SHALL produce at least `Nergal.app` (`.app` bundle) and `Nergal_<version>_aarch64.dmg` under `src-tauri/target/release/bundle/`
- **AND** Tauri SHALL also produce `Nergal.app.tar.gz` and `Nergal.app.tar.gz.sig` (updater artifacts) under `src-tauri/target/release/bundle/macos/`

#### Scenario: Linux build path unchanged
- **WHEN** `pnpm tauri build` runs on an Ubuntu runner
- **THEN** the Linux artifacts (`.deb`, `.rpm`, `.AppImage`, `.AppImage.sig`) SHALL be produced at their existing paths
- **AND** no macOS-related config SHALL alter the Linux build

### Requirement: CI build matrix runs Linux and macOS in parallel

The GitHub Actions workflow at `.github/workflows/release.yml` SHALL use a **three-job structure** on `v*` tag pushes:

1. **`build-linux`** on `ubuntu-22.04`: identical to the current job except it no longer creates the GH release itself. It SHALL upload Linux artifacts and a Linux-platform JSON fragment as GitHub Actions artifacts.
2. **`build-macos`** on `macos-latest`: installs Rust stable, pnpm + Node, runs `pnpm install --frozen-lockfile`, runs `pnpm tauri build` with `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. It SHALL upload macOS artifacts and a macOS-platform JSON fragment as GitHub Actions artifacts.
3. **`publish`** (`needs: [build-linux, build-macos]`): downloads all artifacts, merges platform fragments into `latest.json`, creates the GitHub release with all assets, updates the previous-release banner.

The macOS job SHALL NOT require `apt-get` or GStreamer env vars (those are Linux-only). The macOS runner has WebKit as part of the OS SDK; no additional system dependency installation is required.

#### Scenario: Both jobs succeed — publish fires once
- **WHEN** both `build-linux` and `build-macos` succeed and upload their artifacts
- **THEN** `publish` SHALL run exactly once
- **AND** the GitHub release SHALL contain `.deb`, `.rpm`, `.AppImage`, `.AppImage.sig`, `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`, and `latest.json`

#### Scenario: macOS build fails — no partial release
- **WHEN** `build-macos` fails (e.g., compilation error, missing Rust target)
- **THEN** `publish` SHALL NOT run
- **AND** no GitHub release SHALL be created for the tag
- **AND** the Linux artifacts SHALL be discarded (GitHub Actions artifact TTL handles cleanup)

#### Scenario: Linux build fails — no partial release
- **WHEN** `build-linux` fails
- **THEN** `publish` SHALL NOT run regardless of macOS build status

#### Scenario: Cargo cache shared per OS
- **WHEN** a cached Cargo build exists for the current `Cargo.lock` hash on a given runner OS
- **THEN** `~/.cargo/registry`, `~/.cargo/git`, and `src-tauri/target` SHALL be restored before the build step on that runner
- **AND** the cache key SHALL include `runner.os` so Linux and macOS caches never collide

### Requirement: latest.json covers all distribution platforms

The `latest.json` updater manifest published as a GitHub release asset SHALL contain a `platforms` object with entries for every actively supported distribution platform. After this change, it SHALL include at minimum `linux-x86_64` and `darwin-aarch64`. Each entry SHALL contain `url` and `signature` forming a CONSISTENT pair: `url` points to the **updater artifact** whose `.sig` content is in `signature` — the `.AppImage` (+`.AppImage.sig`) for `linux-x86_64` and the `.app.tar.gz` (+`.app.tar.gz.sig`) for `darwin-aarch64`. The `.dmg` SHALL NOT appear in `latest.json` (it is a manual-download asset surfaced only via `check_app_update().dmg_asset_url`); pairing a `.dmg` URL with a `.app.tar.gz` signature would fail updater verification.

The merge process SHALL be deterministic: platform fragments emitted by each runner are downloaded by the `publish` job and merged via `scripts/merge-latest-json.mjs`. The `version` and `notes` fields SHALL be identical across fragments (keyed from the same tag + CHANGELOG); the merge SHALL abort if `version` differs. `pub_date` SHALL be generated once by the merge step (NOT carried in fragments, since per-runner clocks diverge). Fragments SHALL NOT contain `pub_date`.

#### Scenario: Merged latest.json validates against Tauri updater schema
- **GIVEN** a Linux fragment containing `linux-x86_64` and a macOS fragment containing `darwin-aarch64`
- **WHEN** `merge-latest-json.mjs` combines them
- **THEN** the output SHALL be a valid Tauri updater manifest with both platform entries and a merge-generated `pub_date`
- **AND** each entry SHALL have a non-empty `url` (GitHub asset download URL) and `signature` (`.sig` file contents)
- **AND** the `darwin-aarch64.url` SHALL end with `.app.tar.gz` (the updater artifact), never `.dmg`

#### Scenario: Tauri plugin updater on macOS resolves darwin-aarch64 entry
- **GIVEN** the user is running Nergal on an Apple Silicon Mac
- **WHEN** `tauri-plugin-updater` fetches `latest.json` from the configured endpoint
- **THEN** it SHALL resolve the `darwin-aarch64` entry and compare versions
- **AND** if a newer version is available, it SHALL report the update to the in-app check surface

#### Scenario: Tauri plugin updater on Linux resolves linux-x86_64 entry unchanged
- **GIVEN** the user is running Nergal as an AppImage on Linux
- **WHEN** `tauri-plugin-updater` fetches `latest.json`
- **THEN** it SHALL resolve the `linux-x86_64` entry, unchanged from the pre-matrix behavior

### Requirement: Updater detects macOS install source

`src-tauri/src/updater.rs` SHALL add a `MacApp` variant to `InstallSource`. `detect_install_source()` SHALL return `InstallSource::MacApp` when the running executable's path contains `.app/Contents/MacOS/` (the standard macOS `.app` bundle layout). The existing detection logic for `Appimage`, `Deb`, and `Dev` SHALL be unchanged and remain evaluated first.

The `check_app_update()` command SHALL add `dmg_asset_url: Option<String>` and `dmg_asset_size: Option<u64>` fields to `UpdateCheckResult`, populated by searching the GitHub release assets for a file ending `.dmg` and containing `aarch64` (for Apple Silicon). The existing `deb_asset_url` and `appimage_asset_url` fields SHALL remain.

The About UI (`src/components/settings/SettingsPanel.tsx`) branches on `InstallSource` to decide which download flow to offer, but its TypeScript `InstallSource` union, `UpdateCheckResult` interface, `sourceLabel` map, and download gate (`installSource === "deb"`) are currently Linux-only. This change SHALL extend them: add `"mac_app"` to the union, add `dmgAssetUrl`/`dmgAssetSize` to `UpdateCheckResult`, add a `mac_app` entry to `sourceLabel`, and broaden the download gate so `mac_app` follows the `deb`-style "download to ~/Downloads/ and reveal in Finder" flow reading `dmgAssetUrl`. `mac_app` SHALL NOT be routed through the `appimage` `tauri-plugin-updater` auto-install path — fully automated in-app update is deferred until Apple notarization is in place (auto-install on an un-notarized build re-triggers Gatekeeper).

#### Scenario: detect_install_source on macOS .app install
- **GIVEN** Nergal runs from `/Applications/Nergal.app/Contents/MacOS/nergal`
- **WHEN** `detect_install_source()` is called
- **THEN** it SHALL return `InstallSource::MacApp`

#### Scenario: check_app_update returns dmg_asset_url when available
- **GIVEN** the latest GitHub release contains a `.dmg` asset with `aarch64` in its name
- **WHEN** `check_app_update()` is called on a macOS install
- **THEN** `UpdateCheckResult.dmg_asset_url` SHALL be `Some(<download URL>)`
- **AND** `UpdateCheckResult.dmg_asset_size` SHALL be `Some(<byte count>)`

#### Scenario: detect_install_source unchanged on Linux paths
- **GIVEN** Nergal runs from `/usr/bin/nergal` (deb) or with `APPIMAGE` set (AppImage) or from `/target/release/` (dev)
- **WHEN** `detect_install_source()` is called
- **THEN** it SHALL return `Deb`, `Appimage`, or `Dev` respectively — unchanged from pre-change behavior

### Requirement: Apple notarization and Windows signing are documented human-gated follow-ups

The spec, design, and CLAUDE.md SHALL document that **macOS bundles are unsigned at the OS level** (no `codesign`/`notarytool`) and **Windows bundles are unsigned with Authenticode** (no code-signing certificate). On macOS, users see a Gatekeeper warning (right-click → Open to bypass). On Windows, users see a SmartScreen "Windows protected your PC" warning on first install/update of the unsigned NSIS installer (More info → Run anyway to bypass); because `tauri-plugin-updater` runs the installer in place, auto-update still works through that one-time prompt.

The **Tauri signer (minisign)** provides artifact integrity for the updater on all platforms; this is orthogonal to OS-level signing (Apple notarization / Windows Authenticode).

The macOS follow-up gate SHALL be documented as: (a) Apple Developer Program ($99/yr), (b) Developer ID Application cert, (c) `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` + `APPLE_SIGNING_IDENTITY` + `APPLE_ID` + `APPLE_TEAM_ID` + `APPLE_ID_PASSWORD` GH secrets, (d) codesign + notarize steps in `build-macos`, (e) cut a release.

The Windows follow-up gate SHALL be documented as: (a) obtain an Authenticode code-signing certificate (OV ~$200/yr, or EV for immediate SmartScreen reputation), (b) configure the cert + password as GH secrets (e.g. `WINDOWS_CERTIFICATE` (base64 `.pfx`) + `WINDOWS_CERTIFICATE_PASSWORD`), (c) add a `signtool` / Tauri `bundle.windows.certificateThumbprint`-or-`signCommand` step to `build-windows`, (d) cut a release — at which point SmartScreen warnings subside (immediately for EV, with reputation buildup for OV).

#### Scenario: Unsigned app opens with the OS bypass on each platform

- **WHEN** a user downloads the unsigned `.dmg` (macOS 13+) or runs the unsigned `.msi`/NSIS installer (Windows)
- **THEN** macOS SHALL show a Gatekeeper "unidentified developer" dialog (bypass: right-click → Open) and Windows SHALL show a SmartScreen "Windows protected your PC" dialog (bypass: More info → Run anyway), and after the one-time bypass the app SHALL launch normally

#### Scenario: Tauri updater integrity works without OS-level signing

- **WHEN** `tauri-plugin-updater` checks for an update and downloads a new `.app.tar.gz` (macOS), `.AppImage` (Linux), or `*-setup.nsis.zip` (Windows)
- **THEN** the minisign signature in `latest.json` SHALL be verified against the pubkey in `tauri.conf.json`, and a verified update SHALL be staged/installed regardless of Apple/Authenticode signing status (subject, on Windows, to the one-time SmartScreen prompt while unsigned)

### Requirement: Windows bundle, CI job, and updater entry

The distribution pipeline SHALL produce and publish Windows artifacts. `src-tauri/tauri.conf.json`'s `bundle.targets: "all"` + `createUpdaterArtifacts: true` SHALL yield, on a Windows runner, a `.msi` (WiX), a `.exe`/`*-setup.exe` (NSIS), and the NSIS updater artifacts (`*-setup.nsis.zip` + `.nsis.zip.sig`). No new `identifier` or deep-link scheme config is required — `com.nergal.app` supplies the toast AppUserModelID and `plugins.deep-link.desktop.schemes: ["nergal"]` supplies the `nergal://` registration that the installer applies.

The CI workflow `.github/workflows/release.yml` SHALL add a fourth job `build-windows` on `windows-latest` (Rust stable + pnpm + Node, `pnpm install --frozen-lockfile`, `pnpm tauri build` with `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), uploading the Windows bundles + a `windows-platform.json` fragment. It SHALL NOT run `apt-get`/GStreamer steps (Windows uses bundled WebView2). The `publish` job SHALL `needs: [build-linux, build-macos, build-windows]`, include `artifacts/windows-artifacts` in its `find -type f` asset glob, and pass the Windows fragment to `merge-latest-json.mjs`. A `build-windows` failure SHALL block the release (no partial publish), identical to the other platforms.

`latest.json` SHALL gain a `windows-x86_64` entry whose `url` is the NSIS updater artifact (`*-setup.nsis.zip`) and whose `signature` is that artifact's `.sig` — a consistent pair, mirroring the AppImage/`.app.tar.gz` pattern. The `.msi` SHALL NOT appear in `latest.json` (manual-download asset only, like the `.dmg`). The merge SHALL stay deterministic (`version` identical across all three fragments, `pub_date` generated once at merge).

#### Scenario: Windows bundle is built and published

- **WHEN** a `v*` tag is pushed and `build-windows` runs `pnpm tauri build` on `windows-latest`
- **THEN** it SHALL produce a `.msi`, a `*-setup.exe`, and `*-setup.nsis.zip` + `.nsis.zip.sig` under `src-tauri/target/release/bundle/`, and the GitHub release SHALL contain the `.msi`, the `.exe`/`*-setup.exe`, the `.nsis.zip` + `.sig`, alongside the Linux + macOS assets

#### Scenario: latest.json includes windows-x86_64 pointing at the NSIS artifact

- **WHEN** `merge-latest-json.mjs` combines the Linux, macOS, and Windows fragments
- **THEN** the output SHALL contain `linux-x86_64`, `darwin-aarch64`, and `windows-x86_64`, each with a consistent `url`/`signature` pair, and `windows-x86_64.url` SHALL end with `.nsis.zip` (the updater artifact), never `.msi`

#### Scenario: A Windows build failure blocks the whole release

- **WHEN** `build-windows` fails
- **THEN** `publish` SHALL NOT run and no GitHub release SHALL be created for the tag

---

### Requirement: Windows install-source detection and in-app auto-update

`src-tauri/src/updater.rs` SHALL add an `InstallSource::Windows` variant. `install_source_for_path` SHALL classify a Windows dev build (path containing `\target\release\` or `\target\debug\`) as `Dev` and an installed Windows build as `Windows`; the existing `Appimage`/`MacApp`/`Deb`/`Dev` classification SHALL be unchanged (evaluated first). The About UI SHALL add `"windows"` to its `InstallSource` union and `sourceLabel`, and SHALL route `"windows"` through the **auto-install** flow (`tauri-plugin-updater`, the same branch as `appimage`) — NOT the manual `mac_app` download flow — because the minisign-verified NSIS updater installs in place on Windows. The UI copy SHALL note that Windows SmartScreen may warn on the unsigned installer until Authenticode signing is configured.

#### Scenario: Windows install routed to auto-update

- **GIVEN** Nergal runs from an installed Windows location (not `\target\…\`)
- **WHEN** `detect_install_source()` is called and the About UI checks for an update
- **THEN** `detect_install_source()` SHALL return `InstallSource::Windows`, and the UI SHALL offer the `tauri-plugin-updater` auto-install path (resolving the `windows-x86_64` entry), not a manual download

#### Scenario: Windows dev build classified as Dev

- **GIVEN** Nergal runs from `…\target\release\nergal.exe`
- **WHEN** `detect_install_source()` is called
- **THEN** it SHALL return `InstallSource::Dev` (Windows path separators handled), unchanged-in-spirit from the Unix dev detection

---

