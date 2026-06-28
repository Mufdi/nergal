# Tasks — windows-bundle-ci

Depends on `windows-compile`/`-ipc`/`-proc`/`-desktop` (the crate must build + run on Windows before bundling). Last change of the port — the 3-platform release cuts after this.

## 1. build-windows CI job

- [ ] 1.1 `.github/workflows/release.yml` — add `build-windows` on `windows-latest` mirroring `build-macos` (checkout, pnpm, node+cache, rust stable, cargo cache keyed by `runner.os`, `pnpm install --frozen-lockfile`, `pnpm tauri build` with `TAURI_SIGNING_PRIVATE_KEY` + `…_PASSWORD`). NO apt/GStreamer.
- [ ] 1.2 Emit the Windows fragment: `generate-latest-json.mjs --fragment windows-x86_64 <nsis.zip.sig> <download-url> <ref> windows-platform.json` (PowerShell to resolve the `*-setup.nsis.zip` name). Upload `windows-artifacts` (`.msi`, `*-setup.exe`, `*-setup.nsis.zip`, `.nsis.zip.sig`) with `if-no-files-found: error`, and upload `windows-platform`. Confirm the exact `bundle/nsis/` + `bundle/msi/` subdirs on the first run.

## 2. publish job → four jobs

- [ ] 2.1 `publish` — `needs: [build-linux, build-macos, build-windows]`; add `artifacts/windows-platform/windows-platform.json` to the `merge-latest-json.mjs` call; add `artifacts/windows-artifacts` to the `find -type f` asset glob.

## 3. InstallSource::Windows (updater.rs)

- [ ] 3.1 Add `Windows` to `enum InstallSource`. In `install_source_for_path`, classify `\target\release\`/`\target\debug\` (and forward-slash forms) as `Dev` **unconditionally** (so a Linux `cargo test` covers it — iprev #1), else the installed-`Windows` verdict gated to the Windows target; keep the Unix checks first. Unit-test `…\target\release\nergal.exe` → `Dev` on the Linux job; the installed → `Windows` half is walk-verified (no CI runs Windows `cargo test`).

## 4. About UI (SettingsPanel.tsx)

- [ ] 4.1 Add `"windows"` to the `InstallSource` union + a `sourceLabel` entry. Route `windows` through the `handleAppImageUpdate` auto-install **action** (OS-agnostic `tauri-plugin-updater`, no AppImage coupling). Add a **separate `installSource === "windows"` copy block** (do NOT just broaden the AppImage-worded gates — iprev #1: `:2587` + the `state.kind`-keyed strings at `:2648`/`:2658` say "AppImage"); make download/installing status strings say "installer" for windows + add the SmartScreen note. Update the stale `updater.rs:6` doc comment (the appimage branch is the live auto-install path, shared by windows).

## 5. bundle.windows config (only if needed)

- [ ] 5.1 OPTIONAL — add a `bundle.windows` section only if the first build needs it (WebView2 mode, NSIS install mode). Default `targets: "all"` + `createUpdaterArtifacts` already produce `.msi`/NSIS/updater artifacts. No identifier/scheme change (AUMID + `nergal://` already declared).

## 6. CLAUDE.md release docs

- [ ] 6.1 Update "Release commands": CI = four parallel build jobs + publish; assets add `.msi`/`*-setup.exe`/`*-setup.nsis.zip`(+`.sig`); add the Windows Authenticode follow-up beside the Apple notarization section (cert → GH secrets `WINDOWS_CERTIFICATE`/`WINDOWS_CERTIFICATE_PASSWORD` → `signtool`/Tauri sign step in `build-windows`).

## 7. Verification

- [ ] 7.1 **Pre-release smoke-test** — push a throwaway `v0.0.0-winsmoke` tag; confirm all four jobs green, the release has the Windows `.msi`/`*-setup.exe`/`.nsis.zip`/`.sig`, and `latest.json` has `windows-x86_64` → `.nsis.zip` (pre-release → no banner clobber). Delete the test release/tag after.
- [ ] 7.2 **Linux + macOS unchanged** — their jobs still green; `latest.json` keeps `linux-x86_64` + `darwin-aarch64`.
- [ ] 7.3 **`pnpm release:test`** — the release-script helper suite still passes (no regression to the release tooling).
- [ ] 7.4 **User Windows-machine walk (UNVERIFIED-pending)** — install `.msi`/`.exe` (bypass SmartScreen), launch, confirm toast notifications + `nergal://` deep link, trigger an in-app update → auto-install path runs.
