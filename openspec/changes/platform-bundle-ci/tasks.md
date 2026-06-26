# Tasks — platform-bundle-ci

Phased per the implementation plan. Each phase is independently verifiable. Implement in a fresh session; read `implementation.md` first.

## 1. macOS bundle config

- [ ] 1.1 In `src-tauri/tauri.conf.json`, add a `bundle.macOS` block after the closing `}` of `bundle.linux`, containing `minimumSystemVersion: "12.0"` and `category: "DeveloperTool"`. Preserve all existing keys unchanged.
- [ ] 1.2 Verify `src-tauri/tauri.conf.json` parses as valid JSON (e.g., `node -e "require('./src-tauri/tauri.conf.json')" && echo ok`).

## 2. Rust — InstallSource + updater fields

- [ ] 2.1 In `src-tauri/src/updater.rs`, add `MacApp` variant to the `InstallSource` enum (after `Appimage`, before `Dev`). The `#[serde(rename_all = "snake_case")]` derive will serialize it as `"mac_app"`.
- [ ] 2.2 In `detect_install_source()`, add a `.app/Contents/MacOS/` path probe after the `APPIMAGE` env-var check and before the `/usr/bin/nergal` exact-match check.
- [ ] 2.3 In `UpdateCheckResult`, add `pub dmg_asset_url: Option<String>` and `pub dmg_asset_size: Option<u64>` fields.
- [ ] 2.4 In `check_app_update()`, add a `dmg` asset search (`ends_with(".dmg") && contains("aarch64")`) and populate the new fields in the `Ok(UpdateCheckResult { ... })` constructor.
- [ ] 2.5 Add test `install_source_recognizes_mac_app_bundle_path` in `mod tests` — assert that a path containing `.app/Contents/MacOS/` returns `MacApp` from a manual string probe (mirror the existing dev-path test at line `:480-483`).
- [ ] 2.6 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`. All must pass.

## 3. Script — platform fragment emission

- [ ] 3.1 In `scripts/generate-latest-json.mjs`, add a `--fragment` CLI mode. When `process.argv[2] === "--fragment"`, route to a new `emitFragment(platform, sigFilePath, artifactUrl, tag, outPath)` function that writes a single-platform JSON fragment (keys: `version`, `notes`, `pub_date`, `platform`, `signature`, `url`). The existing `main()` path (no `--fragment`) must be unchanged.
- [ ] 3.2 Run `pnpm release:test` — all existing tests must still pass.

## 4. Script — merge-latest-json.mjs

- [ ] 4.1 Create `scripts/merge-latest-json.mjs`: accepts N fragment paths + one output path as positional args, merges `platforms` entries from all fragments into a valid Tauri updater `latest.json` manifest, writes to output. Shared fields (`version`, `notes`, `pub_date`) taken from the first fragment; warn if they differ across fragments.
- [ ] 4.2 Create `scripts/merge-latest-json.test.mjs` with `node --test`: test that two fragments with different platform keys produce a correct merged manifest; test that mismatched `version` fields emit a warning but do not abort.
- [ ] 4.3 Add `package.json` script: `"release:merge-json": "node scripts/merge-latest-json.mjs"`.
- [ ] 4.4 Run `node --test scripts/merge-latest-json.test.mjs` — all pass.

## 5. CI — three-job release workflow

- [ ] 5.1 In `.github/workflows/release.yml`, split the existing single job into three jobs:
  - **`build-linux`**: keep all existing build steps; remove the "Extract release body", "Generate latest.json", "Create GitHub release", and "Update previous-release banner" steps; add a "Emit Linux platform fragment" step (`--fragment linux-x86_64 ...`); upload Linux artifacts + `linux-platform.json` via `actions/upload-artifact@v4`.
  - **`build-macos`** (new): `runs-on: macos-latest`; steps: checkout, pnpm, Node, Rust, Cargo cache (key includes `runner.os`), `pnpm install --frozen-lockfile`, `pnpm tauri build` (signing env vars, no GStreamer env), "Emit macOS platform fragment" (`--fragment darwin-aarch64 ...`), upload macOS artifacts + `macos-platform.json` via `actions/upload-artifact@v4`.
  - **`publish`** (new, `needs: [build-linux, build-macos]`): `runs-on: ubuntu-22.04`, `permissions: contents: write`; steps: checkout, pnpm + Node, `pnpm install --frozen-lockfile`, `actions/download-artifact@v4` for both artifact sets, extract release body, `merge-latest-json.mjs` to produce `latest.json`, `gh release create` with all artifacts, `update-previous-banner.mjs`.
- [ ] 5.2 Verify the workflow YAML is syntactically valid: `actionlint .github/workflows/release.yml` (install via `brew install actionlint` on macOS or `go install` if available) or push to a test branch and let GH validate.
- [ ] 5.3 Smoke-test with a throwaway tag: `git tag v0.0.0-test1 && git push origin v0.0.0-test1`. Monitor https://github.com/Mufdi/nergal/actions — both build jobs must succeed and the publish job must fire. Inspect the resulting release for all expected assets (`.deb`, `.rpm`, `.AppImage`, `.AppImage.sig`, `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`, `latest.json`). Inspect `latest.json` contents for both `linux-x86_64` and `darwin-aarch64` entries.
- [ ] 5.4 Clean up smoke-test: `gh release delete v0.0.0-test1 --yes && git push origin :v0.0.0-test1`. Restore previous release's banner if it was clobbered (run `node scripts/update-previous-banner.mjs <actual-latest-tag>` manually).

## 6. Documentation

- [ ] 6.1 In `CLAUDE.md`, update the "Release commands" section: note that CI now builds both Linux and macOS artifacts in parallel.
- [ ] 6.2 In `CLAUDE.md`, add a "Deferred: Apple notarization (macOS OS-level signing)" subsection listing the six human-gate steps from the design document (Apple Developer account, cert, five GH secrets, codesign + notarytool CI step, upgrade About UI to auto-install).

## N. Verification

- [ ] N.1 Full local check green: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] N.2 Script tests green: `pnpm release:test && node --test scripts/merge-latest-json.test.mjs`.
- [ ] N.3 CI smoke-test completes (task 5.3) with all expected artifacts in the GitHub release.
- [ ] N.4 On a macOS machine (or ask a macOS user): mount the `.dmg`, right-click → Open the app, confirm it launches. Open Settings → About and verify install source shows a "Download" affordance (not auto-update), pointing to the `.dmg` URL.
- [ ] N.5 Verify `latest.json` signature field for `darwin-aarch64` is non-empty and matches the `.app.tar.gz.sig` file content (can be checked with `diff` between the release asset and the signature string in the JSON).
- [ ] N.6 Verify the Linux release path is unchanged: on an existing Linux install (AppImage or deb), the About update check still resolves the `linux-x86_64` asset correctly.

## Deferred (not tasks for this change)

The following are documented human-gated follow-ups — NOT to be implemented here:

- Apple Developer Program enrollment ($99/yr) + Developer ID Application cert generation
- `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ID_PASSWORD` GH secrets
- `codesign` + `notarytool submit --staple` step in `build-macos` CI job
- Upgrading the About UI macOS path from "download .dmg" to `tauri-plugin-updater` auto-install
- Windows bundling (`build-windows` CI job, `bundle.windows` config, NSIS/MSI, `windows-x86_64` in `latest.json`, EV code-signing cert)
- Intel macOS support (`macos-13` runner, `darwin-x86_64` platform entry)
