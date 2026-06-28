## ADDED Requirements

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

## MODIFIED Requirements

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
