# Tasks — windows-bundle-ci

Depends on `windows-compile`/`-ipc`/`-proc`/`-desktop` (the crate must build + run on Windows before bundling). Last change of the port — the 3-platform release cuts after this.

## 1. build-windows CI job

- [x] 1.1 `.github/workflows/release.yml` — added `build-windows` on `windows-latest` mirroring `build-macos` (checkout, pnpm, node+cache, rust stable, cargo cache keyed by `runner.os`, `pnpm install --frozen-lockfile`, `pnpm tauri build` with the signing secrets). No apt/GStreamer.
- [x] 1.2 Emit the Windows fragment via a `shell: pwsh` step (`Get-ChildItem …/nsis/*-setup.nsis.zip` → `generate-latest-json.mjs --fragment windows-x86_64 <zip.sig> <url> <ref> windows-platform.json`). Upload `windows-artifacts` (`.msi`, `*-setup.exe`, `*-setup.nsis.zip`, `.nsis.zip.sig`) with `if-no-files-found: error`, and upload `windows-platform`. **Exact `bundle/nsis/`+`bundle/msi/` subdirs confirmed only on the first real CI run (smoke-test 7.1) — `if-no-files-found: error` turns any drift into a hard failure.**

## 2. publish job → four jobs

- [x] 2.1 `publish` — `needs: [build-linux, build-macos, build-windows]`; added `artifacts/windows-platform/windows-platform.json` to the `merge-latest-json.mjs` call; added `artifacts/windows-artifacts` to the `find -type f` asset glob. (Both scripts already variadic/platform-agnostic — no script change.)

## 3. InstallSource::Windows (updater.rs)

- [x] 3.1 Added `Windows` to `enum InstallSource` (with `#[cfg_attr(not(windows), allow(dead_code))]` — only constructed under `#[cfg(windows)]`, so off-Windows it would trip `-D dead-code`). In `install_source_for_path`, the backslash `\target\release\`/`\target\debug\` forms classify as `Dev` **unconditionally** (Linux test `install_source_recognizes_windows_dev_build_path` covers it); the installed → `Windows` verdict is `#[cfg(windows)]`-gated (non-Windows falls through to `Unknown`). Unix checks unchanged + first. Walk-verified half has a `#[cfg(windows)]` test (`install_source_installed_windows_is_windows`).

## 4. About UI (SettingsPanel.tsx)

- [x] 4.1 Added `"windows"` to the `InstallSource` union + `sourceLabel` (`"Windows (installer)"`). Routed `windows` through the `onUpdateAppImage`/`handleAppImageUpdate` auto-install action (broadened the `case "available"` gate to `installSource === "appimage" || installSource === "windows"`). Added a **separate `installSource === "windows"` copy block** with the SmartScreen note (did NOT broaden the AppImage-worded text); made the `appimage_downloading` status string say "installer" vs "AppImage" via `installSource === "windows"`. Updated the stale `updater.rs` module doc comment (appimage branch IS the live auto-install path, shared by windows). `npx tsc --noEmit` clean.

## 5. bundle.windows config (only if needed)

- [x] 5.1 OPTIONAL — SKIPPED (defaults suffice). `bundle.targets: "all"` + `createUpdaterArtifacts: true` already produce `.msi`/NSIS/`.nsis.zip`+`.sig`. No `bundle.windows` section, no identifier/scheme change (AUMID `com.nergal.app` + `nergal://` already declared). Revisit only if the smoke-test reveals a WebView2/NSIS-mode need.

## 6. CLAUDE.md release docs

- [x] 6.1 Updated "Release commands": CI = four parallel build jobs + publish; assets add `.msi`/`*-setup.exe`/`*-setup.nsis.zip`(+`.sig`). Added the "Deferred: Windows Authenticode" follow-up beside the Apple notarization section (cert → GH secrets `WINDOWS_CERTIFICATE`/`WINDOWS_CERTIFICATE_PASSWORD` → Tauri `signCommand`/`signtool` step in `build-windows`; notes the About-UI windows branch already uses auto-install, unlike the mac_app step).

## 7. Verification

- [ ] 7.1 **Pre-release smoke-test** — push a throwaway `v0.0.0-winsmoke` tag; confirm all four jobs green, the release has the Windows `.msi`/`*-setup.exe`/`.nsis.zip`/`.sig`, and `latest.json` has `windows-x86_64` → `.nsis.zip` (pre-release → no banner clobber). Delete the test release/tag after.
- [ ] 7.2 **Linux + macOS unchanged** — their jobs still green; `latest.json` keeps `linux-x86_64` + `darwin-aarch64`.
- [x] 7.3 **`pnpm release:test`** ✅ — 28/28 pass (no regression to the release tooling). Plus Linux full check: `cargo clippy -- -D warnings` clean, `cargo test` 700 passed (the new `install_source_recognizes_windows_dev_build_path` test), `cargo fmt --check` clean, `npx tsc --noEmit` clean.
- [ ] 7.4 **User Windows-machine walk (UNVERIFIED-pending)** — install `.msi`/`.exe` (bypass SmartScreen), launch, confirm toast notifications + `nergal://` deep link, trigger an in-app update → auto-install path runs.
