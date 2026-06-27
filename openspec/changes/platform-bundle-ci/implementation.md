# Implementation Plan: platform-bundle-ci

> Grounded in current codebase, symbols verified 2026-06-26. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- `src-tauri/tauri.conf.json:28-29` — `"targets": "all"` is the current value. Tauri 2 builds only host-OS-appropriate targets when `targets: "all"` is set, so no cross-compilation concern.
- `src-tauri/tauri.conf.json:46-72` — `bundle.linux` block exists with `appimage`, `deb`, `rpm` sub-keys. No `bundle.macOS` key anywhere in the file. Icon list at `:37-45` includes `icons/icon.icns` and `icons/icon.ico` (already present).
- `src-tauri/tauri.conf.json:82-89` — `plugins.updater` block with `active: true`, `endpoints`, `dialog: false`, `pubkey`. Endpoint points to `https://github.com/Mufdi/nergal/releases/latest/download/latest.json`.
- `.github/workflows/release.yml:11-93` — single job `build-linux` on `ubuntu-22.04`. Creates the GH release directly in this job (`:78-88`). Calls `scripts/generate-latest-json.mjs` at `:75` and `scripts/update-previous-banner.mjs` at `:93`.
- `scripts/generate-latest-json.mjs:10-11` — `APPIMAGE_DIR` is hardcoded to `src-tauri/target/release/bundle/appimage`. `findOne()` at `:12-16` reads from that dir on the local filesystem.
- `scripts/generate-latest-json.mjs:35-46` — manifest structure: `{version, notes, pub_date, platforms: {"linux-x86_64": {signature, url}}}`. Only `linux-x86_64` entry; no `darwin-*` or `windows-*`.
- `src-tauri/src/updater.rs:17-24` — `InstallSource` enum: `Deb`, `Appimage`, `Dev`, `Unknown`. No `MacApp` variant.
- `src-tauri/src/updater.rs:26-47` — `detect_install_source()`: checks `APPIMAGE` env var (`:27-29`), `.AppImage` suffix (`:34-36`), `/usr/bin/nergal` | `/usr/local/bin/nergal` exact match (`:38-42`), `/target/release/` or `/target/debug/` contains check (`:43-45`). Falls through to `Unknown`.
- `src-tauri/src/updater.rs:96-109` — `UpdateCheckResult` struct: `deb_asset_url: Option<String>`, `deb_asset_size: Option<u64>`, `appimage_asset_url: Option<String>`, `appimage_asset_size: Option<u64>`. No `dmg_*` fields.
- `src-tauri/src/updater.rs:186-193` — `check_app_update()` asset search: `ends_with(".deb") && contains("amd64")` for deb (`:188-189`); `ends_with(".AppImage")` for appimage (`:192-193`). No `.dmg` search.
- `src-tauri/src/updater.rs:111` — `GITHUB_RELEASES_URL = "https://api.github.com/repos/Mufdi/nergal/releases/latest"`.
- `src-tauri/src/updater.rs:18` — `#[serde(rename_all = "snake_case")]` on `InstallSource` — serialized as `"mac_app"` for the new variant (consistent with existing `snake_case` convention).
- `scripts/extract-release-body.mjs` — exports `buildReleaseBody(changelog, version)` which `generate-latest-json.mjs` already imports at line 6.
- `scripts/release.mjs` — contains `extractChangelogSection` (imported by `extract-release-body.mjs`). `release:test` script tests the release helpers.

## Execution order

1. `src-tauri/tauri.conf.json` — add `bundle.macOS` block (macOS config required before CI can build it)
2. `src-tauri/src/updater.rs` — add `MacApp` variant + pure `install_source_for_path` classifier + macOS detection + `dmg_asset_url` fields
2B. `src/components/settings/SettingsPanel.tsx` — extend the About panel for `mac_app` (union, `UpdateCheckResult` dmg fields, `sourceLabel`, deb-style download gate). NOT zero-change; consumes the new Rust fields.
3. `scripts/generate-latest-json.mjs` — add `emitFragment(platform, sigFilePath, artifactUrl, tag, outPath)` function for CI fragment emission while preserving the existing `main()` path
4. `scripts/merge-latest-json.mjs` — new script: reads two fragment files, merges `platforms` maps, writes final `latest.json`
5. `.github/workflows/release.yml` — restructure to three-job layout (this is the capstone; all prior steps must be in place)
6. `CLAUDE.md` — update "Release commands" section + add "Deferred: Apple notarization" subsection

## Plan

### Phase 1: tauri.conf.json — macOS bundle config

**File**: `src-tauri/tauri.conf.json`

Add a `bundle.macOS` object after the existing `bundle.linux` object:
```json
"macOS": {
  "minimumSystemVersion": "12.0"
}
```

Notes:
- **No `category` key here.** The Tauri 2 `MacConfig` schema (`@tauri-apps/cli@2.11.1/config.schema.json`) is `additionalProperties: false`; its allowed keys are `frameworks, files, bundleVersion, bundleName, minimumSystemVersion, exceptionDomain, signingIdentity, hardenedRuntime, providerShortName, entitlements, infoPlist, dmg`. `category` is NOT among them — it lives only at top-level `bundle.category` (already `"DeveloperTool"` in this file). Adding `category` under `bundle.macOS` fails schema validation and breaks `pnpm tauri build` on EVERY platform (config parsing is platform-agnostic), including the Linux release path. The macOS app category is therefore already covered by the existing top-level key.
- No `dmg` sub-key needed at this level — Tauri 2 generates DMG from the `.app` automatically when `targets: "all"` on macOS.
- `icons/icon.icns` is already listed in `bundle.icon` (`:43`) so the macOS app icon is covered.
- `minimumSystemVersion: "12.0"` — macOS Monterey; safe floor for Apple Silicon Macs and modern WebKit.
- Do NOT add `entitlements` or `provisioningProfile` — those are only needed for notarization (deferred).

### Phase 2: updater.rs — MacApp variant + dmg asset fields

**File**: `src-tauri/src/updater.rs`

**Change 1**: Add `MacApp` variant to `InstallSource` enum (`:17-24`):
```rust
MacApp,
```
The `#[serde(rename_all = "snake_case")]` attribute serializes this as `"mac_app"`. **Frontend changes ARE required** (see Phase 2B below) — the About panel's download gate is a hard `installSource === "deb"` equality reading `debAssetUrl`, and `sourceLabel` is a fixed map with no `mac_app` key; an un-handled `mac_app` would get no affordance and render `undefined`.

**Change 2**: Extract a pure classifier for testability, then probe for `.app`. `detect_install_source()` currently reads `env::var_os("APPIMAGE")` + `current_exe()` directly, which makes the `.app/Contents/MacOS/` branch untestable in CI (the test binary's real exe path is under `target/`). Extract:
```rust
fn install_source_for_path(exe_str: &str, appimage_env: bool) -> InstallSource {
    if appimage_env { return InstallSource::Appimage; }
    if exe_str.ends_with(".AppImage") { return InstallSource::Appimage; }
    if exe_str.contains(".app/Contents/MacOS/") { return InstallSource::MacApp; }
    if matches!(exe_str, "/usr/bin/nergal" | "/usr/local/bin/nergal") { return InstallSource::Deb; }
    if exe_str.contains("/target/release/") || exe_str.contains("/target/debug/") { return InstallSource::Dev; }
    InstallSource::Unknown
}
```
`detect_install_source()` becomes a thin wrapper that gathers the env/exe and delegates. The `.app` probe sits before the `/usr/bin/nergal` match and matches any `.app` layout (`/Applications/Nergal.app/Contents/MacOS/nergal`, `~/Applications/...`). Existing classification is unchanged.

**Change 3**: Add `dmg_asset_url` and `dmg_asset_size` to `UpdateCheckResult` (`:96-109`):
```rust
pub dmg_asset_url: Option<String>,
pub dmg_asset_size: Option<u64>,
```

**Change 4**: Add `.dmg` asset search in `check_app_update()` (after the `appimage` search at `:192-193`):
```rust
let dmg = release
    .assets
    .iter()
    .find(|a| a.name.ends_with(".dmg") && a.name.contains("aarch64"));
```
Then populate the new fields in the `Ok(UpdateCheckResult { ... })` constructor.

**Tests** (add to `mod tests` at `:463`, calling the pure `install_source_for_path`):
- `install_source_recognizes_mac_app_bundle_path` — `install_source_for_path("/Applications/Nergal.app/Contents/MacOS/nergal", false) == MacApp`.
- Regression: `/usr/bin/nergal` → `Deb`; a `.AppImage` path → `Appimage`; `appimage_env = true` → `Appimage`; a `/target/release/` path → `Dev`. Confirms the new `.app` probe does not perturb existing classification.
- Existing `install_source_recognizes_dev_build_path` remains (now exercising the wrapper or the pure fn).

### Phase 2B: SettingsPanel.tsx — About panel `mac_app` support

**File**: `src/components/settings/SettingsPanel.tsx`

The frontend is NOT zero-change (the original plan's assumption was wrong). Edits:
- `:2000` — `type InstallSource = "deb" | "appimage" | "mac_app" | "dev" | "unknown";`
- `:2002` `UpdateCheckResult` interface — add `dmgAssetUrl: string | null;` and `dmgAssetSize: number | null;` (camelCase, matching the serde `rename_all = "camelCase"` on the Rust struct).
- `:2174` `sourceLabel` map — add `mac_app: ".app (macOS)"`.
- `:2064` / `:2443`+ download flow — broaden so `mac_app` follows the `deb` download-and-reveal path reading `dmgAssetUrl`/`dmgAssetSize` (reuse the `deb` render branch and its Download button; a small helper `dmgFilename(result)` mirroring `debFilename`). Do NOT add `mac_app` to `handleAppImageUpdate()` (the `@tauri-apps/plugin-updater` `downloadAndInstall` path) — auto-install on an un-notarized macOS build re-triggers Gatekeeper (design D4).
- Verify with `npx tsc --noEmit`.

### Phase 3: generate-latest-json.mjs — fragment emission mode

**File**: `scripts/generate-latest-json.mjs`

Add a new exported function `emitFragment(platform, sigFilePath, artifactUrl, tag, outPath)`:
- Reads the `.sig` file at `sigFilePath`, trims whitespace.
- Reads `CHANGELOG.md` for `notes` (reuse existing `buildReleaseBody(changelog, version)` import, where `version = tag.replace(/^v/, '')` — identical to `main()`).
- Writes a fragment (NO `pub_date` — generated once at merge time, since per-runner `new Date()` diverges):
  ```json
  {
    "version": "<tag>",
    "notes": "<changelog body>",
    "platform": "<platform key>",
    "signature": "<sig content>",
    "url": "<updater-artifact GitHub download URL>"
  }
  ```
  `version` is the raw `tag` (v-prefixed), matching `main()`'s manifest at `:36`, so the Linux and macOS fragments are byte-identical on shared fields. **`url` is the updater artifact** — `.AppImage` for Linux, `.app.tar.gz` for macOS — NOT the `.dmg`.
- Used by CI as: `node scripts/generate-latest-json.mjs --fragment <platform> <sig-path> <url> <tag> <out-path>`

Keep the existing `main()` path intact (invoked without `--fragment`) so local use and `pnpm release:test` are unaffected. The `--fragment` flag routes to `emitFragment`.

### Phase 4: merge-latest-json.mjs — new script

**File**: `scripts/merge-latest-json.mjs` (new)

- Takes N fragment file paths as positional arguments: `node scripts/merge-latest-json.mjs <frag1.json> <frag2.json> ... <out.json>`
- Reads each fragment, validates that `version`, `platform`, `signature`, `url` are present.
- Shared fields: `version` and `notes` from the first fragment. **ABORT (non-zero exit) if `version` differs** across fragments (runners built different tags — must not ship); warn-only if `notes` differ.
- `pub_date`: GENERATED here as `new Date().toISOString()` (not read from fragments).
- Assembles `platforms` map: `{[fragment.platform]: {signature: fragment.signature, url: fragment.url}, ...}`.
- Writes final `latest.json` (`{version, notes, pub_date, platforms}`).

Add `package.json` script: `"release:merge-json": "node scripts/merge-latest-json.mjs"`.

### Phase 5: release.yml — three-job restructure

**File**: `.github/workflows/release.yml`

**Job `build-linux`** (modified from existing):
- `runs-on: ubuntu-22.04` (unchanged)
- All existing steps preserved (deps, pnpm, Rust, cache, build)
- Remove the "Extract release body", "Generate latest.json", "Create GitHub release", and "Update previous-release banner" steps — these move to `publish`
- Add step: "Emit Linux platform fragment" — write into the workspace (`./linux-platform.json`), NOT `/tmp`, so its upload path is predictable:
  ```
  node scripts/generate-latest-json.mjs --fragment linux-x86_64 \
    src-tauri/target/release/bundle/appimage/Nergal_*.AppImage.sig \
    "https://github.com/Mufdi/nergal/releases/download/$GITHUB_REF_NAME/$(ls src-tauri/target/release/bundle/appimage/*.AppImage | xargs basename)" \
    "$GITHUB_REF_NAME" ./linux-platform.json
  ```
- Upload artifacts as TWO separate `actions/upload-artifact@v4` entries so each download path is deterministic (avoids the v4 least-common-ancestor surprise where mixing `/tmp/...` + workspace paths in one artifact buries the fragment under a `tmp/` subdir):
  - name `linux-artifacts`, path = the `.deb`, `.rpm`, `.AppImage`, `.AppImage.sig` globs.
  - name `linux-platform`, path = `linux-platform.json` (single file → lands at `artifacts/linux-platform/linux-platform.json` after download).

**Job `build-macos`** (new):
- `runs-on: macos-latest`
- Steps: `actions/checkout@v4`, `pnpm/action-setup@v3`, `actions/setup-node@v4`, `dtolnay/rust-toolchain@stable`, `actions/cache@v4` (same paths, key includes `runner.os`), `pnpm install --frozen-lockfile`, `pnpm tauri build` (with signing env vars, NO GStreamer env), "Emit macOS platform fragment", then TWO `actions/upload-artifact@v4` entries — name `macos-artifacts` (the `.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`) and name `macos-platform` (`macos-platform.json`, single file).
- **Fragment URL = `.app.tar.gz`, NOT `.dmg`** (corrects the earlier draft). The fragment pairs the `.app.tar.gz.sig` signature with the `.app.tar.gz` download URL so the Tauri updater verifies a consistent artifact. Derive the filename dynamically (symmetric with the Linux `ls | xargs basename`, in case Tauri version-suffixes the name) and write the fragment into the workspace:
  ```
  TARBALL=$(ls src-tauri/target/release/bundle/macos/*.app.tar.gz | xargs basename)
  node scripts/generate-latest-json.mjs --fragment darwin-aarch64 \
    "src-tauri/target/release/bundle/macos/$TARBALL.sig" \
    "https://github.com/Mufdi/nergal/releases/download/$GITHUB_REF_NAME/$TARBALL" \
    "$GITHUB_REF_NAME" ./macos-platform.json
  ```
  The `.dmg` is uploaded as a release asset (manual download via `check_app_update().dmg_asset_url`) but never referenced in `latest.json`.
- macOS artifact paths under `src-tauri/target/release/bundle/macos/`: `Nergal.app.tar.gz`, `Nergal.app.tar.gz.sig`, `Nergal_<version>_aarch64.dmg`. Verify exact `.app.tar.gz`/`.dmg` names on the first CI run and adjust the globs (the `.dmg` may be `dmg/Nergal_<version>_aarch64.dmg`).

**Job `publish`** (new, `needs: [build-linux, build-macos]`):
- `runs-on: ubuntu-22.04`
- `permissions: contents: write`
- Steps:
  1. `actions/checkout@v4`
  2. `pnpm/action-setup@v3` + `actions/setup-node@v4`
  3. `pnpm install --frozen-lockfile` (for the scripts)
  4. `actions/download-artifact@v4` with `path: artifacts` — each uploaded artifact lands under `artifacts/<artifact-name>/...` (e.g. `artifacts/linux-artifacts/`, `artifacts/linux-platform/`, `artifacts/macos-artifacts/`, `artifacts/macos-platform/`). **The build jobs' local paths do NOT survive the round-trip**; resolve from the download dirs. Fragments were uploaded as single-file artifacts so each is at `artifacts/<name>-platform/<name>-platform.json`.
  5. Extract release body: `node scripts/extract-release-body.mjs "${GITHUB_REF_NAME#v}" > /tmp/release-body.md`
  6. Merge JSON from the downloaded fragments: `node scripts/merge-latest-json.mjs artifacts/linux-platform/linux-platform.json artifacts/macos-platform/macos-platform.json /tmp/latest.json`
  7. Pre-release guard: `if [[ "$GITHUB_REF_NAME" == *-* ]]; then PRERELEASE=--prerelease; fi` — then `gh release create "$GITHUB_REF_NAME" $PRERELEASE --title "Nergal $GITHUB_REF_NAME" --notes-file /tmp/release-body.md` with all artifacts globbed from `artifacts/**` + `/tmp/latest.json`.
  8. Banner only for real releases: `if [[ "$GITHUB_REF_NAME" != *-* ]]; then node scripts/update-previous-banner.mjs "$GITHUB_REF_NAME"; fi` (skip for pre-release/smoke-test tags so the live latest release's banner is never clobbered).

Note on macOS artifact naming: Tauri 2 names `.dmg` as `Nergal_<version>_aarch64.dmg` on ARM and the `.app.tar.gz` updater artifact is named from `productName`. Verify on first CI run and adjust the upload/glob paths if needed.

### Phase 6: CLAUDE.md — documentation update

**File**: `CLAUDE.md` (repo root)

In the "Release commands" section, add a note under "First-time signing key setup" that acknowledges the deferred Apple signing gate. Add a new subsection "Deferred: Apple notarization (macOS OS-level signing)" summarizing the six steps from the design document.

## Per-phase risk

**[Phase 1: tauri.conf.json]** → Low risk. Additive JSON key; Linux build path is unaffected. The `minimumSystemVersion` value can be adjusted without breaking existing installs.

**[Phase 2: updater.rs]** → Low risk. New enum variant + new struct fields in an API only consumed by the frontend About panel. The frontend receives `InstallSource` as a string and the existing unknown-variant handling keeps it non-breaking. New `dmg_*` fields in `UpdateCheckResult` are optional; the frontend can read them without a breaking change. **Verify**: `cargo clippy -- -D warnings` must pass; `cargo test` must include the new `install_source_recognizes_mac_app_bundle_path` test.

**[Phase 3: generate-latest-json.mjs]** → Medium risk. The existing `main()` path must be preserved exactly. Validate with `pnpm release:test` before touching the CI YAML.

**[Phase 4: merge-latest-json.mjs]** → Low risk. New file; no existing code touched. Add a basic unit test (`node --test scripts/merge-latest-json.test.mjs`) for the merge logic.

**[Phase 5: release.yml]** → High risk (release pipeline). Mitigation: test with a hyphenated `v0.0.0-test1` tag (`git tag v0.0.0-test1 && git push origin v0.0.0-test1`) — the hyphen triggers the `--prerelease` guard (D7), so the test release never becomes the live `/releases/latest` target and the banner script is skipped. Do NOT use `pnpm release 0.0.0-test1` here: that script's CHANGELOG pre-flight guard requires a `## v0.0.0-test1` section and would abort. Verify both build jobs succeed and the publish job fires; inspect macOS artifact paths in the job logs; confirm `latest.json` `darwin-aarch64.url` ends with `.app.tar.gz`. Clean up: `gh release delete v0.0.0-test1 --yes && git push origin :v0.0.0-test1`.

**[Phase 6: CLAUDE.md]** → Trivial.

## Verification

```bash
# Rust checks (no cargo run needed — planning only; run at implementation time)
cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check

# TS checks
npx tsc --noEmit

# Script tests
pnpm release:test
node --test scripts/merge-latest-json.test.mjs
```

macOS-specific manual checks (run on actual macOS hardware or CI):
- `pnpm tauri build` on macOS runner produces `Nergal.app`, `Nergal_<ver>_aarch64.dmg`, `Nergal.app.tar.gz`, `Nergal.app.tar.gz.sig` under `src-tauri/target/release/bundle/macos/`
- Install the `.dmg` on macOS; right-click → Open to bypass Gatekeeper; confirm app launches
- Verify `get_install_source` returns `"mac_app"` in the About panel
- `check_app_update()` returns `dmg_asset_url` pointing to the GitHub release `.dmg`
- `latest.json` in the release contains both `linux-x86_64` and `darwin-aarch64` entries with non-empty `signature` fields
