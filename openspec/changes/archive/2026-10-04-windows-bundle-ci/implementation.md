# Implementation — windows-bundle-ci

No SQLite schema changes. CI + config + a Rust enum variant + a TS UI branch + docs.

## Verified codebase facts (do not re-assume)

Verified against current source 2026-06-28:

- **`tauri.conf.json`**: `bundle.targets: "all"`, `createUpdaterArtifacts: true`, `identifier: "com.nergal.app"` (→ toast AUMID), `plugins.deep-link.desktop.schemes: ["nergal"]`, no `bundle.windows` section yet. The updater endpoint is `…/releases/latest/download/latest.json`, pubkey pinned.
- **`release.yml`** (3-job, already read): `build-linux` (ubuntu-22.04, GStreamer env) + `build-macos` (macos-latest, no apt) + `publish` (`needs: [build-linux, build-macos]`). Each build job: `generate-latest-json.mjs --fragment <platform> <sig> <url> <ref> <out>` → uploads `<platform>-platform.json`. `publish`: `download-artifact`, `merge-latest-json.mjs <linux-frag> <macos-frag> /tmp/latest.json`, `find artifacts/linux-artifacts artifacts/macos-artifacts -type f` → `gh release create`. Pre-release tags (contain `-`) → `--prerelease`, skip the banner.
- **`generate-latest-json.mjs`**: takes the platform name as a CLI arg (`--fragment <name>`) — platform-agnostic, so `windows-x86_64` works without a script change (confirm). **`merge-latest-json.mjs`**: variadic (`<frag1> [frag2 …] <out>`), `version` must match across all, `pub_date` generated once — already handles a 3rd fragment.
- **`updater.rs`**: `enum InstallSource { Deb, Appimage, MacApp, Dev, Unknown }`; `install_source_for_path(exe_str, appimage_env)` checks `.AppImage` / `.app/Contents/MacOS/` / `/usr/bin/nergal` / `/target/release/`|`/target/debug/` (forward-slash — Windows paths use backslash, so the dev check needs a Windows branch).
- **`SettingsPanel.tsx`**: `type InstallSource = "deb" | "appimage" | "mac_app" | "dev" | "unknown"` (`:2000`); branches `installSource === "deb"` (download+reveal), `"mac_app"` (download `.dmg`), `"appimage"` (auto-install). `sourceLabel` map (`:2205`).

## Edit plan

### Step 1 — `build-windows` CI job
Add to `release.yml`, mirroring `build-macos` (no apt/GStreamer):
```yaml
build-windows:
  runs-on: windows-latest
  permissions: { contents: read }
  steps:
    - checkout / setup pnpm / setup node (cache pnpm) / rust stable / cargo cache (key includes runner.os, already)
    - run: pnpm install --frozen-lockfile
    - name: Build signed bundles
      env: { TAURI_SIGNING_PRIVATE_KEY: …, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: … }
      run: pnpm tauri build
    - name: Emit Windows platform fragment
      run: |   # PowerShell on windows-latest
        $zip = (Get-ChildItem src-tauri/target/release/bundle/nsis/*-setup.nsis.zip).Name
        node scripts/generate-latest-json.mjs --fragment windows-x86_64 `
          "src-tauri/target/release/bundle/nsis/$zip.sig" `
          "https://github.com/Mufdi/nergal/releases/download/$env:GITHUB_REF_NAME/$zip" `
          "$env:GITHUB_REF_NAME" ./windows-platform.json
    - name: Upload Windows bundles
      uses: actions/upload-artifact@v4
      with:
        name: windows-artifacts
        if-no-files-found: error
        path: |
          src-tauri/target/release/bundle/msi/*.msi
          src-tauri/target/release/bundle/nsis/*-setup.exe
          src-tauri/target/release/bundle/nsis/*-setup.nsis.zip
          src-tauri/target/release/bundle/nsis/*-setup.nsis.zip.sig
    - name: Upload Windows platform fragment (name: windows-platform, path: windows-platform.json)
```
Confirm the exact bundle subdir on Windows (`bundle/nsis/` for NSIS, `bundle/msi/` for WiX) on the first CI run — `if-no-files-found: error` turns a path drift into a hard failure (the macOS `.dmg`-path lesson).

### Step 2 — `publish` extends to four
- `needs: [build-linux, build-macos, build-windows]`.
- Merge: `node scripts/merge-latest-json.mjs artifacts/linux-platform/linux-platform.json artifacts/macos-platform/macos-platform.json artifacts/windows-platform/windows-platform.json /tmp/latest.json`.
- Asset glob: `ASSETS=$(find artifacts/linux-artifacts artifacts/macos-artifacts artifacts/windows-artifacts -type f)`.

### Step 3 — `InstallSource::Windows` (updater.rs)
- Add `Windows` to the enum. In `install_source_for_path`, before the `Unknown` fallback, add a branch: treat `\target\release\` / `\target\debug\` (and the forward-slash forms) as `Dev`, else (on a Windows target, gated so non-Windows still returns `Unknown`) `Windows`. Keep the existing Unix checks first.
- **Make the dev-path detection host-agnostic so it is testable on the Linux CI job (iprev #1):** `install_source_for_path` is a pure string classifier — add the `\target\release\` check unconditionally (not behind `#[cfg(windows)]`), so a Linux `cargo test` can assert a `…\target\release\nergal.exe` string → `Dev`. Only the "installed → `Windows`" verdict is `#[cfg(windows)]`-dependent (no CI runs Windows `cargo test`, so that half is walk-verified). This splits the coverage: dev-detection tested on Linux, installed-detection on the user's machine.

### Step 4 — About UI (SettingsPanel.tsx)
- `type InstallSource = … | "windows" | …`. Add `windows` to `sourceLabel` (e.g. `"Windows (installer)"`).
- Route `windows` through the **auto-install action** (`handleAppImageUpdate` / `onUpdateAppImage`, which is the OS-agnostic `tauri-plugin-updater` `downloadAndInstall` path — confirmed no AppImage coupling): broaden the action gate to `installSource === "appimage" || installSource === "windows"`.
- **Do NOT blindly broaden the copy gates (iprev #1):** `:2587` and the state-keyed text (`:2648` "Downloading signed AppImage…", `:2658`) say "AppImage" and would mis-word for Windows (the state-keyed lines branch on `state.kind`, not `installSource`, so a condition broaden won't fix them). Add a **separate `installSource === "windows"` copy block** with Windows wording ("The installer updates Nergal in place; Windows SmartScreen may show a one-time prompt until the app is code-signed"), and make the download/installing status strings install-source-aware (say "installer" not "AppImage" when `installSource === "windows"`).
- **Update the stale doc comment** at `updater.rs:6` ("AppImage signed auto-install is a future addition; both paths share download-and-reveal") — it is already false (the appimage branch IS the live auto-install path); note Windows shares it.

### Step 5 — `bundle.windows` config (only if needed)
Leave defaults unless the first build shows a need; if WebView2 offline support is requested later, set `bundle.windows.webviewInstallMode`. No change to identifier/scheme.

### Step 6 — CLAUDE.md release docs
Update "Release commands": CI is now **four** parallel build jobs + publish; assets add `.msi`, `*-setup.exe`, `*-setup.nsis.zip`(+`.sig`); add the Windows Authenticode follow-up beside the Apple notarization section.

## Verification (maps to tasks.md ## N. Verification)

- A throwaway pre-release tag (e.g. `v0.0.0-winsmoke`) CI run is green across all four jobs; the release contains the Windows `.msi` + `*-setup.exe` + `.nsis.zip` + `.sig`; `latest.json` has `windows-x86_64` pointing at the `.nsis.zip`. (Pre-release → no banner clobber, mirroring the macOS smoke-test.)
- Linux + macOS jobs unchanged and green.
- `merge-latest-json.mjs` produces a 3-platform manifest (unit/CI check).
- User Windows-machine walk (UNVERIFIED-pending): install the `.msi`/`.exe`, launch (bypass SmartScreen), confirm toast notifications + `nergal://` deep link work, then trigger an in-app update and confirm the auto-install path runs.
