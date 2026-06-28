# Tasks — platform-bundle-ci

Phased per the implementation plan. Each phase is independently verifiable. Implement in a fresh session; read `implementation.md` first.

## 1. macOS bundle config

- [x] 1.1 In `src-tauri/tauri.conf.json`, add a `bundle.macOS` block after the closing `}` of `bundle.linux`, containing ONLY `minimumSystemVersion: "12.0"`. Do NOT add `category` — the `MacConfig` schema is `additionalProperties: false` with no `category` key (the macOS app category is already set by the existing top-level `bundle.category: "DeveloperTool"`); an unknown key would fail `tauri.conf.json` schema validation and break `pnpm tauri build` on Linux too. Preserve all existing keys unchanged. — Added `"macOS": { "minimumSystemVersion": "12.0" }` inside `bundle`, after `linux` close.
- [x] 1.2 Verify the config is valid: `node -e "require('./src-tauri/tauri.conf.json')"` for JSON syntax AND confirm schema conformance — the macOS block must contain no key outside the `MacConfig` allow-list (`minimumSystemVersion` is the only one used here). The authoritative gate is the macOS CI build (`pnpm tauri build`), which validates against the Tauri schema; a passing JSON `require()` alone is NOT sufficient. — JSON valid; no disallowed keys; authoritative gate is CI.

## 2. Rust — InstallSource + updater fields

- [x] 2.1 In `src-tauri/src/updater.rs`, add `MacApp` variant to the `InstallSource` enum (after `Appimage`, before `Dev`). The `#[serde(rename_all = "snake_case")]` derive will serialize it as `"mac_app"`. — Done.
- [x] 2.2 Extract a PURE, testable classifier so the new branch can be unit-tested without depending on the test binary's real `current_exe()`. Add `fn install_source_for_path(exe_str: &str, appimage_env: bool) -> InstallSource` containing the existing decision logic (APPIMAGE env → `Appimage`; `.AppImage` suffix → `Appimage`; **`.app/Contents/MacOS/` substring → `MacApp`** inserted before the `/usr/bin/nergal` match; the `/usr/bin/nergal` | `/usr/local/bin/nergal` exact match → `Deb`; `/target/release/` | `/target/debug/` substring → `Dev`; else `Unknown`). Rewrite `detect_install_source()` to gather `env::var_os("APPIMAGE").is_some()` + `current_exe()` and delegate to `install_source_for_path`. Behavior for existing sources MUST be unchanged. — Done; pure fn + thin wrapper.
- [x] 2.3 In `UpdateCheckResult`, add `pub dmg_asset_url: Option<String>` and `pub dmg_asset_size: Option<u64>` fields. — Done.
- [x] 2.4 In `check_app_update()`, add a `dmg` asset search (`ends_with(".dmg") && contains("aarch64")`) and populate the new fields in the `Ok(UpdateCheckResult { ... })` constructor. — Done.
- [x] 2.5 Add tests in `mod tests` exercising the pure `install_source_for_path`: (a) `install_source_recognizes_mac_app_bundle_path` — `/Applications/Nergal.app/Contents/MacOS/nergal` → `MacApp`; (b) a regression assertion that `/usr/bin/nergal` → `Deb`, an `.AppImage` path → `Appimage`, and a `/target/release/` path → `Dev` (the new `.app` probe must not perturb existing classification). Mirror the existing dev-path test at `:480-483`. — Added `install_source_recognizes_mac_app_bundle_path` + `install_source_classifier_regression`; existing dev-path test rewritten to exercise pure fn.
- [x] 2.6 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`. All must pass. — clippy: no issues; test: 699 passed; fmt: clean.

## 2B. Frontend — About panel `mac_app` support (NOT zero-change)

The About panel in `src/components/settings/SettingsPanel.tsx` is Linux-only and would otherwise leave macOS users with no update affordance and an `undefined` source label. Mirror the `deb` download-and-reveal flow; do NOT route `mac_app` through `handleAppImageUpdate()` (the `@tauri-apps/plugin-updater` auto-install path) — auto-install on an un-notarized build re-triggers Gatekeeper.

- [x] 2B.1 Add `"mac_app"` to the `InstallSource` TS union (`:2000`). — Done.
- [x] 2B.2 Add `dmgAssetUrl: string | null` and `dmgAssetSize: number | null` to the `UpdateCheckResult` interface (`:2002`). — Done.
- [x] 2B.3 Add a `mac_app` entry to the `sourceLabel` map (`:2174`), e.g. `mac_app: ".app (macOS)"`. — Done.
- [x] 2B.4 Broaden the download flow so `mac_app` follows the `deb`-style path reading `dmgAssetUrl`/`dmgAssetSize` (the staged-download check at `:2064` and the download/render branches at `:2443`+). Reuse the existing `deb` rendering branch; render its Download button for both `deb` and `mac_app`. Do NOT add a `mac_app` case to the `appimage` plugin-updater branch. — Added `dmgFilename()`, `handleDownloadDmg()`, staged-download check for `mac_app`, `onDownloadDmg` prop, and Download button in `renderUpdateButton` `"available"` case. `mac_app` never routes to `handleAppImageUpdate`.
- [x] 2B.5 Run `npx tsc --noEmit` — must pass; verify no `installSource === "deb"`-only assumption silently excludes `mac_app`. — TypeScript: no errors found.

## 3. Script — platform fragment emission

- [x] 3.1 In `scripts/generate-latest-json.mjs`, add a `--fragment` CLI mode. When `process.argv[2] === "--fragment"`, route to a new `emitFragment(platform, sigFilePath, artifactUrl, tag, outPath)` function that writes a single-platform JSON fragment (keys: `version`, `notes`, `platform`, `signature`, `url`). **Do NOT include `pub_date` in the fragment** — it is generated once at merge time (see 4.1), because per-runner `new Date()` values diverge. `version` MUST equal the raw `tag` (v-prefixed), matching the existing `main()` representation at `:36`; `notes` MUST be computed from the bare version via the existing `buildReleaseBody` import, exactly as `main()` does. The existing `main()` path (no `--fragment`) must be unchanged. — Done; `emitFragment` exported; `--fragment` routes to it; no `pub_date` in fragment; existing `main()` path intact.
- [x] 3.2 Run `pnpm release:test` — all existing tests must still pass. — 25 passed.

## 4. Script — merge-latest-json.mjs

- [x] 4.1 Create `scripts/merge-latest-json.mjs`: accepts N fragment paths + one output path as positional args, merges `platforms` entries from all fragments into a valid Tauri updater `latest.json` manifest, writes to output. Shared fields `version` and `notes` are taken from the first fragment; ABORT (non-zero exit) if `version` differs across fragments (a hard mismatch means the runners built different tags — must not silently ship); warn-only if `notes` differ. `pub_date` is GENERATED by this script (`new Date().toISOString()`) — it is not read from fragments. Each `platforms[<platform>]` entry contains exactly `{ signature, url }`. For `darwin-aarch64`, `url` MUST be the `.app.tar.gz` download URL and `signature` the `.app.tar.gz.sig` content (the `.dmg` never appears in `latest.json`). — Done.
- [x] 4.2 Create `scripts/merge-latest-json.test.mjs` with `node --test`: test that two fragments with different platform keys produce a correct merged manifest with a generated `pub_date` and both platform entries; test that mismatched `version` fields cause a non-zero exit; test that the merged `darwin-aarch64.url` ends with `.app.tar.gz` (not `.dmg`). — Done; 3 tests pass.
- [x] 4.3 Add `package.json` script: `"release:merge-json": "node scripts/merge-latest-json.mjs"`. — Done.
- [x] 4.4 Run `node --test scripts/merge-latest-json.test.mjs` — all pass. — 3/3 passed.

## 5. CI — three-job release workflow

- [x] 5.1 In `.github/workflows/release.yml`, split the existing single job into three jobs: `build-linux` (keep all build steps, remove release/banner steps, emit fragment into workspace, upload TWO artifacts), `build-macos` (new, `macos-latest`, dynamic tarball basename, fragment, TWO artifact uploads), `publish` (new, `needs: [build-linux, build-macos]`, downloads all, merges JSON, `gh release create` with `--prerelease` guard on `-` in tag, banner only for non-prerelease). — Done.
- [~] 5.2 Verify the workflow YAML is syntactically valid: `actionlint .github/workflows/release.yml` — actionlint not available locally; unverified locally. GitHub Actions will validate on first push.
- [~] 5.3 Smoke-test with a throwaway PRE-RELEASE tag — deferred to human/CI. Requires pushing a tag which triggers a real billed macos-latest CI run.
- [~] 5.4 Clean up smoke-test — deferred to human (follows 5.3).

## 6. Documentation

- [x] 6.1 In `CLAUDE.md`, update the "Release commands" section: note that CI now builds both Linux and macOS artifacts in parallel. — Updated CI description to three-job layout.
- [x] 6.2 In `CLAUDE.md`, add a "Deferred: Apple notarization (macOS OS-level signing)" subsection listing the six human-gate steps from the design document (Apple Developer account, cert, five GH secrets, codesign + notarytool CI step, upgrade About UI to auto-install). — Done.

## N. Verification

- [x] N.1 Full local check green: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`. — All pass: clippy clean, 699 tests pass, fmt clean, tsc no errors.
- [x] N.2 Script tests green: `pnpm release:test && node --test scripts/merge-latest-json.test.mjs`. — 25 + 3 = 28 tests pass.
- [~] N.3 CI smoke-test completes (task 5.3) — deferred to human/CI.
- [~] N.4 On a macOS machine: mount .dmg, verify install-source label and Download affordance — deferred to human/macOS.
- [~] N.5 Verify `latest.json` `darwin-aarch64.url` ends with `.app.tar.gz` on a real build — deferred to human/CI.
- [x] N.6 Verify the Linux release path is unchanged: macOS cross-check `cargo check --target aarch64-apple-darwin` Finished (no errors).

## Deferred (not tasks for this change)

The following are documented human-gated follow-ups — NOT to be implemented here:

- Apple Developer Program enrollment ($99/yr) + Developer ID Application cert generation
- `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ID_PASSWORD` GH secrets
- `codesign` + `notarytool submit --staple` step in `build-macos` CI job
- Upgrading the About UI macOS path from "download .dmg" to `tauri-plugin-updater` auto-install
- Windows bundling (`build-windows` CI job, `bundle.windows` config, NSIS/MSI, `windows-x86_64` in `latest.json`, EV code-signing cert)
- Intel macOS support (`macos-13` runner, `darwin-x86_64` platform entry)
