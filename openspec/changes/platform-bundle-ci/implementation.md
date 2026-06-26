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
2. `src-tauri/src/updater.rs` — add `MacApp` variant + macOS detection + `dmg_asset_url` fields (pure Rust, no downstream cascade)
3. `scripts/generate-latest-json.mjs` — add `emitFragment(platform, bundleDir, sigSuffix, tag, outPath)` function for CI fragment emission while preserving the existing `main()` path
4. `scripts/merge-latest-json.mjs` — new script: reads two fragment files, merges `platforms` maps, writes final `latest.json`
5. `.github/workflows/release.yml` — restructure to three-job layout (this is the capstone; all prior steps must be in place)
6. `CLAUDE.md` — update "Release commands" section + add "Deferred: Apple notarization" subsection

## Plan

### Phase 1: tauri.conf.json — macOS bundle config

**File**: `src-tauri/tauri.conf.json`

Add a `bundle.macOS` object after the existing `bundle.linux` object:
```json
"macOS": {
  "minimumSystemVersion": "12.0",
  "category": "DeveloperTool"
}
```

Notes:
- `category` must match `bundle.category` value (`"DeveloperTool"`). Tauri 2 uses this for the app's `NSAppCategory` (Finder/Spotlight).
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
The `#[serde(rename_all = "snake_case")]` attribute serializes this as `"mac_app"`. No frontend changes needed — the About panel already handles unknown `InstallSource` variants gracefully (falls through to disabled state).

**Change 2**: Add macOS path probe to `detect_install_source()` (`:26-47`), after the `APPIMAGE` env check and before the `/usr/bin/nergal` check:
```rust
if exe_str.contains(".app/Contents/MacOS/") {
    return InstallSource::MacApp;
}
```
This matches any `.app` bundle layout regardless of installation path (e.g., `/Applications/Nergal.app/Contents/MacOS/nergal` or `~/Applications/Nergal.app/Contents/MacOS/nergal`).

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

**Tests** (add to `mod tests` at `:463`):
- `install_source_recognizes_mac_app_bundle_path` — verify `.app/Contents/MacOS/` detect returns `MacApp`
- Existing test `install_source_recognizes_dev_build_path` remains unchanged; the new check fires before the dev-path check only for `.app` paths, not for `/target/release/` paths.

### Phase 3: generate-latest-json.mjs — fragment emission mode

**File**: `scripts/generate-latest-json.mjs`

Add a new exported function `emitFragment(platform, sigFilePath, artifactUrl, tag, outPath)`:
- Reads the `.sig` file at `sigFilePath`, trims whitespace.
- Reads `CHANGELOG.md` for `notes` (reuse existing `buildReleaseBody` import).
- Writes a fragment:
  ```json
  {
    "version": "<tag>",
    "notes": "<changelog body>",
    "pub_date": "<ISO timestamp>",
    "platform": "<platform key>",
    "signature": "<sig content>",
    "url": "<artifact GitHub download URL>"
  }
  ```
- Used by CI as: `node scripts/generate-latest-json.mjs --fragment <platform> <sig-path> <url> <tag> <out-path>`

Keep the existing `main()` path intact (invoked without `--fragment`) so local use and `pnpm release:test` are unaffected. The `--fragment` flag routes to `emitFragment`.

### Phase 4: merge-latest-json.mjs — new script

**File**: `scripts/merge-latest-json.mjs` (new)

- Takes N fragment file paths as positional arguments: `node scripts/merge-latest-json.mjs <frag1.json> <frag2.json> ... <out.json>`
- Reads each fragment, validates that `version` and `pub_date` are present.
- Merges: uses `version`, `notes`, `pub_date` from the first fragment (all fragments should match; log a warning if they differ).
- Assembles `platforms` map: `{[fragment.platform]: {signature: fragment.signature, url: fragment.url}, ...}`.
- Writes final `latest.json`.

Add `package.json` script: `"release:merge-json": "node scripts/merge-latest-json.mjs"`.

### Phase 5: release.yml — three-job restructure

**File**: `.github/workflows/release.yml`

**Job `build-linux`** (modified from existing):
- `runs-on: ubuntu-22.04` (unchanged)
- All existing steps preserved (deps, pnpm, Rust, cache, build)
- Remove the "Extract release body", "Generate latest.json", "Create GitHub release", and "Update previous-release banner" steps — these move to `publish`
- Add step: "Emit Linux platform fragment":
  ```
  node scripts/generate-latest-json.mjs --fragment linux-x86_64 \
    src-tauri/target/release/bundle/appimage/Nergal_*.AppImage.sig \
    "https://github.com/Mufdi/nergal/releases/download/$GITHUB_REF_NAME/$(ls src-tauri/target/release/bundle/appimage/*.AppImage | xargs basename)" \
    "$GITHUB_REF_NAME" /tmp/linux-platform.json
  ```
- Add step: `actions/upload-artifact@v4` with name `linux-artifacts`, path including the `.deb`, `.rpm`, `.AppImage`, `.AppImage.sig`, and `/tmp/linux-platform.json`

**Job `build-macos`** (new):
- `runs-on: macos-latest`
- Steps: `actions/checkout@v4`, `pnpm/action-setup@v3`, `actions/setup-node@v4`, `dtolnay/rust-toolchain@stable`, `actions/cache@v4` (same paths, key includes `runner.os`), `pnpm install --frozen-lockfile`, `pnpm tauri build` (with signing env vars, NO GStreamer env), "Emit macOS platform fragment" (analogous to Linux, using `.app.tar.gz.sig` and `.dmg` URL), `actions/upload-artifact@v4` for macOS artifacts.
- macOS artifact paths under `src-tauri/target/release/bundle/macos/`: `Nergal.app.tar.gz`, `Nergal.app.tar.gz.sig`, `Nergal_<version>_aarch64.dmg`.

**Job `publish`** (new, `needs: [build-linux, build-macos]`):
- `runs-on: ubuntu-22.04`
- `permissions: contents: write`
- Steps:
  1. `actions/checkout@v4`
  2. `pnpm/action-setup@v3` + `actions/setup-node@v4`
  3. `pnpm install --frozen-lockfile` (for the scripts)
  4. `actions/download-artifact@v4` (download both artifact sets)
  5. Extract release body: `node scripts/extract-release-body.mjs "${GITHUB_REF_NAME#v}" > /tmp/release-body.md`
  6. Merge JSON: `node scripts/merge-latest-json.mjs /tmp/linux-platform.json /tmp/macos-platform.json /tmp/latest.json`
  7. `gh release create "$GITHUB_REF_NAME" --title "Nergal $GITHUB_REF_NAME" --notes-file /tmp/release-body.md` with all artifacts as positional args
  8. `node scripts/update-previous-banner.mjs "$GITHUB_REF_NAME"`

Note on macOS artifact naming: Tauri 2 names `.dmg` as `Nergal_<version>_aarch64.dmg` on ARM and the `.app.tar.gz` updater artifact is always named from `productName`. Verify on first CI run and adjust the glob if needed.

### Phase 6: CLAUDE.md — documentation update

**File**: `CLAUDE.md` (repo root)

In the "Release commands" section, add a note under "First-time signing key setup" that acknowledges the deferred Apple signing gate. Add a new subsection "Deferred: Apple notarization (macOS OS-level signing)" summarizing the six steps from the design document.

## Per-phase risk

**[Phase 1: tauri.conf.json]** → Low risk. Additive JSON key; Linux build path is unaffected. The `minimumSystemVersion` value can be adjusted without breaking existing installs.

**[Phase 2: updater.rs]** → Low risk. New enum variant + new struct fields in an API only consumed by the frontend About panel. The frontend receives `InstallSource` as a string and the existing unknown-variant handling keeps it non-breaking. New `dmg_*` fields in `UpdateCheckResult` are optional; the frontend can read them without a breaking change. **Verify**: `cargo clippy -- -D warnings` must pass; `cargo test` must include the new `install_source_recognizes_mac_app_bundle_path` test.

**[Phase 3: generate-latest-json.mjs]** → Medium risk. The existing `main()` path must be preserved exactly. Validate with `pnpm release:test` before touching the CI YAML.

**[Phase 4: merge-latest-json.mjs]** → Low risk. New file; no existing code touched. Add a basic unit test (`node --test scripts/merge-latest-json.test.mjs`) for the merge logic.

**[Phase 5: release.yml]** → High risk (release pipeline). Mitigation: test with a `v0.0.0-test1` tag on a throwaway branch (`pnpm release 0.0.0-test1 --no-push && git push origin v0.0.0-test1`), verify both build jobs succeed and the publish job fires. Inspect macOS artifact paths in the job logs. Clean up: `gh release delete v0.0.0-test1 --yes && git push origin :v0.0.0-test1`.

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
