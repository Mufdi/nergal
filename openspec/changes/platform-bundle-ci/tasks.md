# Tasks — platform-bundle-ci

Phased per the implementation plan. Each phase is independently verifiable. Implement in a fresh session; read `implementation.md` first.

## 1. macOS bundle config

- [ ] 1.1 In `src-tauri/tauri.conf.json`, add a `bundle.macOS` block after the closing `}` of `bundle.linux`, containing ONLY `minimumSystemVersion: "12.0"`. Do NOT add `category` — the `MacConfig` schema is `additionalProperties: false` with no `category` key (the macOS app category is already set by the existing top-level `bundle.category: "DeveloperTool"`); an unknown key would fail `tauri.conf.json` schema validation and break `pnpm tauri build` on Linux too. Preserve all existing keys unchanged.
- [ ] 1.2 Verify the config is valid: `node -e "require('./src-tauri/tauri.conf.json')"` for JSON syntax AND confirm schema conformance — the macOS block must contain no key outside the `MacConfig` allow-list (`minimumSystemVersion` is the only one used here). The authoritative gate is the macOS CI build (`pnpm tauri build`), which validates against the Tauri schema; a passing JSON `require()` alone is NOT sufficient.

## 2. Rust — InstallSource + updater fields

- [ ] 2.1 In `src-tauri/src/updater.rs`, add `MacApp` variant to the `InstallSource` enum (after `Appimage`, before `Dev`). The `#[serde(rename_all = "snake_case")]` derive will serialize it as `"mac_app"`.
- [ ] 2.2 Extract a PURE, testable classifier so the new branch can be unit-tested without depending on the test binary's real `current_exe()`. Add `fn install_source_for_path(exe_str: &str, appimage_env: bool) -> InstallSource` containing the existing decision logic (APPIMAGE env → `Appimage`; `.AppImage` suffix → `Appimage`; **`.app/Contents/MacOS/` substring → `MacApp`** inserted before the `/usr/bin/nergal` match; the `/usr/bin/nergal` | `/usr/local/bin/nergal` exact match → `Deb`; `/target/release/` | `/target/debug/` substring → `Dev`; else `Unknown`). Rewrite `detect_install_source()` to gather `env::var_os("APPIMAGE").is_some()` + `current_exe()` and delegate to `install_source_for_path`. Behavior for existing sources MUST be unchanged.
- [ ] 2.3 In `UpdateCheckResult`, add `pub dmg_asset_url: Option<String>` and `pub dmg_asset_size: Option<u64>` fields.
- [ ] 2.4 In `check_app_update()`, add a `dmg` asset search (`ends_with(".dmg") && contains("aarch64")`) and populate the new fields in the `Ok(UpdateCheckResult { ... })` constructor.
- [ ] 2.5 Add tests in `mod tests` exercising the pure `install_source_for_path`: (a) `install_source_recognizes_mac_app_bundle_path` — `/Applications/Nergal.app/Contents/MacOS/nergal` → `MacApp`; (b) a regression assertion that `/usr/bin/nergal` → `Deb`, an `.AppImage` path → `Appimage`, and a `/target/release/` path → `Dev` (the new `.app` probe must not perturb existing classification). Mirror the existing dev-path test at `:480-483`.
- [ ] 2.6 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`. All must pass.

## 2B. Frontend — About panel `mac_app` support (NOT zero-change)

The About panel in `src/components/settings/SettingsPanel.tsx` is Linux-only and would otherwise leave macOS users with no update affordance and an `undefined` source label. Mirror the `deb` download-and-reveal flow; do NOT route `mac_app` through `handleAppImageUpdate()` (the `@tauri-apps/plugin-updater` auto-install path) — auto-install on an un-notarized build re-triggers Gatekeeper.

- [ ] 2B.1 Add `"mac_app"` to the `InstallSource` TS union (`:2000`).
- [ ] 2B.2 Add `dmgAssetUrl: string | null` and `dmgAssetSize: number | null` to the `UpdateCheckResult` interface (`:2002`).
- [ ] 2B.3 Add a `mac_app` entry to the `sourceLabel` map (`:2174`), e.g. `mac_app: ".app (macOS)"`.
- [ ] 2B.4 Broaden the download flow so `mac_app` follows the `deb`-style path reading `dmgAssetUrl`/`dmgAssetSize` (the staged-download check at `:2064` and the download/render branches at `:2443`+). Reuse the existing `deb` rendering branch; render its Download button for both `deb` and `mac_app`. Do NOT add a `mac_app` case to the `appimage` plugin-updater branch.
- [ ] 2B.5 Run `npx tsc --noEmit` — must pass; verify no `installSource === "deb"`-only assumption silently excludes `mac_app`.

## 3. Script — platform fragment emission

- [ ] 3.1 In `scripts/generate-latest-json.mjs`, add a `--fragment` CLI mode. When `process.argv[2] === "--fragment"`, route to a new `emitFragment(platform, sigFilePath, artifactUrl, tag, outPath)` function that writes a single-platform JSON fragment (keys: `version`, `notes`, `platform`, `signature`, `url`). **Do NOT include `pub_date` in the fragment** — it is generated once at merge time (see 4.1), because per-runner `new Date()` values diverge. `version` MUST equal the raw `tag` (v-prefixed), matching the existing `main()` representation at `:36`; `notes` MUST be computed from the bare version via the existing `buildReleaseBody` import, exactly as `main()` does. The existing `main()` path (no `--fragment`) must be unchanged.
- [ ] 3.2 Run `pnpm release:test` — all existing tests must still pass.

## 4. Script — merge-latest-json.mjs

- [ ] 4.1 Create `scripts/merge-latest-json.mjs`: accepts N fragment paths + one output path as positional args, merges `platforms` entries from all fragments into a valid Tauri updater `latest.json` manifest, writes to output. Shared fields `version` and `notes` are taken from the first fragment; ABORT (non-zero exit) if `version` differs across fragments (a hard mismatch means the runners built different tags — must not silently ship); warn-only if `notes` differ. `pub_date` is GENERATED by this script (`new Date().toISOString()`) — it is not read from fragments. Each `platforms[<platform>]` entry contains exactly `{ signature, url }`. For `darwin-aarch64`, `url` MUST be the `.app.tar.gz` download URL and `signature` the `.app.tar.gz.sig` content (the `.dmg` never appears in `latest.json`).
- [ ] 4.2 Create `scripts/merge-latest-json.test.mjs` with `node --test`: test that two fragments with different platform keys produce a correct merged manifest with a generated `pub_date` and both platform entries; test that mismatched `version` fields cause a non-zero exit; test that the merged `darwin-aarch64.url` ends with `.app.tar.gz` (not `.dmg`).
- [ ] 4.3 Add `package.json` script: `"release:merge-json": "node scripts/merge-latest-json.mjs"`.
- [ ] 4.4 Run `node --test scripts/merge-latest-json.test.mjs` — all pass.

## 5. CI — three-job release workflow

- [ ] 5.1 In `.github/workflows/release.yml`, split the existing single job into three jobs:
  - **`build-linux`**: keep all existing build steps; remove the "Extract release body", "Generate latest.json", "Create GitHub release", and "Update previous-release banner" steps; add a "Emit Linux platform fragment" step (`--fragment linux-x86_64 ...`) that writes `./linux-platform.json` into the workspace (NOT `/tmp`); upload via TWO `actions/upload-artifact@v4` entries — `linux-artifacts` (the bundles) and `linux-platform` (the single fragment file), so the fragment's downloaded path is deterministic (`artifacts/linux-platform/linux-platform.json`) and not buried by v4's least-common-ancestor path retention.
  - **`build-macos`** (new): `runs-on: macos-latest`; steps: checkout, pnpm, Node, Rust, Cargo cache (key includes `runner.os`), `pnpm install --frozen-lockfile`, `pnpm tauri build` (signing env vars, no GStreamer env), "Emit macOS platform fragment" — `--fragment darwin-aarch64 <.app.tar.gz.sig path> <**.app.tar.gz** download URL> "$GITHUB_REF_NAME" ./macos-platform.json`, deriving the `.app.tar.gz` filename dynamically (`ls *.app.tar.gz | xargs basename`, symmetric with Linux) rather than hardcoding it. The fragment `url` MUST be the `.app.tar.gz` updater artifact, NOT the `.dmg` (the `.dmg` is published as a release asset and surfaced only via `check_app_update().dmg_asset_url`). Upload via TWO `actions/upload-artifact@v4` entries — `macos-artifacts` (`.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`) and `macos-platform` (the single fragment file).
  - **`publish`** (new, `needs: [build-linux, build-macos]`): `runs-on: ubuntu-22.04`, `permissions: contents: write`; steps: checkout, pnpm + Node, `pnpm install --frozen-lockfile`, `actions/download-artifact@v4` with explicit `path: artifacts` — each artifact lands under `artifacts/<artifact-name>/...` (fragments at `artifacts/<name>-platform/<name>-platform.json`), NOT the build jobs' local paths, so resolve from the download dirs; extract release body; `merge-latest-json.mjs artifacts/linux-platform/linux-platform.json artifacts/macos-platform/macos-platform.json <out>` to produce `latest.json`; `gh release create` with all artifacts (globbed from the download dirs) — pass `--prerelease` WHEN `$GITHUB_REF_NAME` contains a `-` (semver pre-release; protects the smoke-test tag from becoming the live `/releases/latest`); run `update-previous-banner.mjs` ONLY for non-prerelease tags (skip when the tag contains `-`).
- [ ] 5.2 Verify the workflow YAML is syntactically valid: `actionlint .github/workflows/release.yml` (install via `brew install actionlint` on macOS or `go install` if available) or push to a test branch and let GH validate.
- [ ] 5.3 Smoke-test with a throwaway PRE-RELEASE tag (the `-` ensures D7's `--prerelease` guard fires, so it never becomes the live updater target): `git tag v0.0.0-test1 && git push origin v0.0.0-test1`. Monitor https://github.com/Mufdi/nergal/actions — both build jobs must succeed and the publish job must fire. Confirm the created release is marked **pre-release** and that `update-previous-banner.mjs` was skipped. Inspect the release for all expected assets (`.deb`, `.rpm`, `.AppImage`, `.AppImage.sig`, `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`, `latest.json`). Inspect `latest.json` for both `linux-x86_64` and `darwin-aarch64` entries and confirm `darwin-aarch64.url` ends with `.app.tar.gz`.
- [ ] 5.4 Clean up smoke-test: `gh release delete v0.0.0-test1 --yes && git push origin :v0.0.0-test1`. Because the tag was a pre-release, `/releases/latest` and the previous banner were never touched, so no banner restore is needed (confirm the real latest release's banner is intact).

## 6. Documentation

- [ ] 6.1 In `CLAUDE.md`, update the "Release commands" section: note that CI now builds both Linux and macOS artifacts in parallel.
- [ ] 6.2 In `CLAUDE.md`, add a "Deferred: Apple notarization (macOS OS-level signing)" subsection listing the six human-gate steps from the design document (Apple Developer account, cert, five GH secrets, codesign + notarytool CI step, upgrade About UI to auto-install).

## N. Verification

- [ ] N.1 Full local check green: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`. (Note: `tsc --noEmit` passing does NOT prove the `mac_app` UI path works — `get_install_source` is a runtime string cast to the union; verify the Phase 2B edits by code review + N.4, not by tsc alone.)
- [ ] N.2 Script tests green: `pnpm release:test && node --test scripts/merge-latest-json.test.mjs`.
- [ ] N.3 CI smoke-test completes (task 5.3) with all expected artifacts in the GitHub release, the release marked pre-release, and the banner script skipped.
- [ ] N.4 On a macOS machine (or ask a macOS user): mount the `.dmg`, right-click → Open the app, confirm it launches. Open Settings → About and verify (a) the install-source label is `.app (macOS)` (NOT blank/`undefined`), and (b) the update check shows a "Download" affordance (not auto-update) that reads `dmgAssetUrl` and points to the `.dmg` release asset. Confirm the `appimage` plugin auto-install path is NOT taken for `mac_app`.
- [ ] N.5 Verify `latest.json` for `darwin-aarch64`: `url` ends with `.app.tar.gz` (the updater artifact, not `.dmg`), and `signature` is non-empty and matches the `.app.tar.gz.sig` file content (`diff` the release asset against the JSON signature string).
- [ ] N.6 Verify the Linux release path is unchanged: on an existing Linux install (AppImage or deb), the About update check still resolves the `linux-x86_64` asset correctly.

## Deferred (not tasks for this change)

The following are documented human-gated follow-ups — NOT to be implemented here:

- Apple Developer Program enrollment ($99/yr) + Developer ID Application cert generation
- `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ID_PASSWORD` GH secrets
- `codesign` + `notarytool submit --staple` step in `build-macos` CI job
- Upgrading the About UI macOS path from "download .dmg" to `tauri-plugin-updater` auto-install
- Windows bundling (`build-windows` CI job, `bundle.windows` config, NSIS/MSI, `windows-x86_64` in `latest.json`, EV code-signing cert)
- Intel macOS support (`macos-13` runner, `darwin-x86_64` platform entry)
