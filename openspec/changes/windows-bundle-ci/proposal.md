## Why

`windows-compile`/`-ipc`/`-proc`/`-desktop` make Nergal build and run on Windows; this change **ships** it — a Windows bundle (`.msi` + NSIS `.exe`), a fourth CI job that builds + signs it, a `windows-x86_64` entry in the updater manifest so Windows gets in-app auto-update, and the install-source/About-UI plumbing. It completes the 3-platform release the whole port has been building toward (no platform ships alone). It also satisfies the install-time prerequisites `windows-desktop` flagged: Tauri derives the toast **AppUserModelID** from `identifier` (`com.nergal.app`), and the NSIS/WiX installer registers the `nergal://` deep-link scheme (already declared in `plugins.deep-link.desktop.schemes`).

Unlike macOS (whose auto-install is deferred until Apple notarization because Gatekeeper blocks an un-notarized `.app`), **Windows gets real auto-update now**: `tauri-plugin-updater` downloads the minisign-verified NSIS installer and runs it. Windows SmartScreen will warn on the unsigned installer (one-time "More info → Run anyway") until the OS-level **Authenticode** code-signing cert is configured — a documented, human-gated follow-up mirroring the Apple notarization gate.

## What Changes

- **Windows bundle config** — `bundle.targets: "all"` already produces `.msi` (WiX) + `.exe` (NSIS) + updater artifacts (`createUpdaterArtifacts: true`) on a Windows runner. Add a `bundle.windows` section only as needed (WebView2 install mode = the default `downloadBootstrapper`; NSIS install mode supporting the updater's silent run). No new identifier/scheme config — `com.nergal.app` (AUMID) and `plugins.deep-link.desktop.schemes: ["nergal"]` already cover the install-time prerequisites.
- **Fourth CI job `build-windows`** (`windows-latest`) in `.github/workflows/release.yml` — Rust stable + pnpm + Node, `pnpm install --frozen-lockfile`, `pnpm tauri build` with `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (minisign, same as Linux/macOS — artifact integrity, orthogonal to Authenticode). No `apt-get`/GStreamer (Windows uses bundled WebView2). Uploads the Windows bundles (`.msi`, `.exe`, `.nsis.zip`, `.nsis.zip.sig`) + a `windows-platform.json` fragment.
- **`publish` job extends to four** — `needs: [build-linux, build-macos, build-windows]`; the `find -type f` asset glob adds `artifacts/windows-artifacts`; the `merge-latest-json.mjs` call adds the Windows fragment (the script is already variadic). A Windows build failure blocks the release (no partial publish), same as the other platforms.
- **`latest.json` gains `windows-x86_64`** — pointing at the NSIS updater artifact (`*-setup.nsis.zip`) + its `.sig` (the consistent pair, mirroring AppImage/`.app.tar.gz`). The `.msi` is a manual-download asset only (like the `.dmg`), never in `latest.json`.
- **Updater install-source + About UI** — add `InstallSource::Windows` (Rust) and `"windows"` (TS union + `sourceLabel`). `install_source_for_path` detects a Windows install (dev = `\target\release\`/`\target\debug\`; otherwise installed). The About UI routes `"windows"` through the **auto-install** path (`tauri-plugin-updater`, the same branch as `appimage`), NOT the manual `mac_app` download flow — Windows auto-update works today. Note the SmartScreen caveat in the UI copy until signing lands.
- **CLAUDE.md release docs** — update the "Release commands" section: the CI is now **four** parallel build jobs + publish; the release assets add `.msi`, `.exe`/`*-setup.nsis.zip`(+`.sig`); the deferred-signing section gains the Windows Authenticode gate alongside Apple notarization.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `platform-distribution`: extends distribution to Windows. **Adds** the Windows bundle + `build-windows` CI job + `windows-x86_64` updater entry + Windows install-source/auto-install, and **modifies** the "Apple notarization and Windows signing are documented human-gated follow-ups" requirement to detail the Windows Authenticode gate (the existing requirement already names Windows signing as a follow-up — this fills it in).

## Impact

- **`src-tauri/tauri.conf.json`**: optional `bundle.windows` (WebView2/NSIS) if defaults need tuning; no identifier/scheme change.
- **`.github/workflows/release.yml`**: new `build-windows` job; `publish` `needs` + asset glob + merge call extended.
- **`scripts/generate-latest-json.mjs`**: invoked with `--fragment windows-x86_64` + the `.nsis.zip.sig` + URL (no script change if already platform-agnostic; confirm).
- **`src-tauri/src/updater.rs`**: `InstallSource::Windows` variant + `install_source_for_path` Windows branch.
- **`src/components/settings/SettingsPanel.tsx`**: `"windows"` in the `InstallSource` union + `sourceLabel`; route through the `appimage` auto-install branch; SmartScreen note.
- **`CLAUDE.md`**: release section → 4-job CI + Windows assets + Windows Authenticode follow-up.
- **Deferred (human-gated follow-up)**: Windows Authenticode code-signing (cert + GH secrets + a signing step in `build-windows`) — until then SmartScreen warns on first install/update.
- **Out of scope**: the other four Windows changes (compile/ipc/proc/desktop).
