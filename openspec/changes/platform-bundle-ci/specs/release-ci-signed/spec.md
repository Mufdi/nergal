## MODIFIED Requirements

### Requirement: CI workflow triggered on v* tags builds and publishes Linux artifacts

A GitHub Actions workflow at `.github/workflows/release.yml` SHALL trigger on `push` events for tags matching the pattern `v*`. On trigger, it SHALL build Linux desktop artifacts (`.deb`, `.rpm`, `.AppImage`) via `pnpm tauri build` on `ubuntu-22.04` **and** macOS desktop artifacts (`.dmg`, `.app.tar.gz`) via `pnpm tauri build` on `macos-latest`, then publish all artifacts as assets on a new GitHub release for the triggering tag.

The workflow SHALL use a three-job structure: `build-linux` and `build-macos` run in parallel; the `publish` job depends on both, assembles `latest.json`, creates the release, and (for non-prerelease tags only) updates the previous-release banner. The `build-linux` job SHALL no longer call `gh release create` directly.

#### Scenario: Tag push triggers the workflow
- **WHEN** a tag matching `v*` is pushed to origin (e.g., from `pnpm release patch`)
- **THEN** the workflow SHALL trigger
- **AND** SHALL check out the repo at the tag's commit in each job

#### Scenario: Workflow builds Linux bundles with required system deps
- **WHEN** the `build-linux` job runs
- **THEN** it SHALL install: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`
- **AND** SHALL run `pnpm install --frozen-lockfile`
- **AND** SHALL run `pnpm tauri build` with `GSTREAMER_PLUGINS_DIR=/usr/lib/x86_64-linux-gnu/gstreamer-1.0` and signing env vars set
- **AND** SHALL produce `.deb`, `.rpm`, `.AppImage`, and `.AppImage.sig` artifacts under `src-tauri/target/release/bundle/`
- **AND** SHALL upload these artifacts and a `linux-platform.json` fragment via `actions/upload-artifact`

#### Scenario: Workflow builds macOS bundles without Linux-specific deps
- **WHEN** the `build-macos` job runs on `macos-latest`
- **THEN** it SHALL NOT run `apt-get` or set `GSTREAMER_PLUGINS_DIR`
- **AND** SHALL run `pnpm install --frozen-lockfile`
- **AND** SHALL run `pnpm tauri build` with signing env vars set
- **AND** SHALL produce `.dmg`, `.app.tar.gz`, and `.app.tar.gz.sig` under `src-tauri/target/release/bundle/`
- **AND** SHALL upload these artifacts and a `macos-platform.json` fragment via `actions/upload-artifact`

#### Scenario: Workflow caches cargo artifacts across runs per OS
- **WHEN** the workflow runs and a previous cache exists keyed by `${{ runner.os }}-cargo-${{ hashFiles('src-tauri/Cargo.lock') }}`
- **THEN** `~/.cargo/registry`, `~/.cargo/git`, and `src-tauri/target` SHALL be restored before the build step on that runner
- **AND** the Linux and macOS caches SHALL never share keys (keyed by `runner.os`)

#### Scenario: Publish job assembles release with all artifacts
- **WHEN** both `build-linux` and `build-macos` succeed
- **THEN** the `publish` job SHALL download all uploaded artifacts
- **AND** merge `linux-platform.json` and `macos-platform.json` into a single `latest.json`
- **AND** invoke `gh release create <tag>` with the merged `latest.json`, all Linux artifacts, and all macOS artifacts as assets
- **AND** update the previous release's banner

#### Scenario: Workflow creates GH release with all artifacts
- **WHEN** the `publish` job runs for a tag with no `-` (e.g. `v0.4.2`)
- **THEN** the workflow SHALL invoke `gh release create <tag>` with `--title "Nergal <tag>"`, the release body extracted from CHANGELOG.md, and all artifacts as assets
- **AND** the release SHALL be public (not a draft, not a prerelease)
- **AND** the previous-release banner SHALL be updated

#### Scenario: Pre-release tags do not become the live updater target
- **GIVEN** a tag containing a `-` is pushed (semver pre-release convention, e.g. the smoke-test tag `v0.0.0-test1` or a future `v1.0.0-rc.1`)
- **WHEN** the `publish` job runs
- **THEN** `gh release create` SHALL be invoked with `--prerelease`
- **AND** the previous-release banner update SHALL be SKIPPED
- **AND** because GitHub `/releases/latest` excludes pre-releases, the existing release SHALL remain the target of the updater endpoint and of `check_app_update()` throughout — a smoke test never exposes live users to the throwaway build

#### Scenario: Publish resolves fragments from downloaded artifacts
- **GIVEN** `build-linux` and `build-macos` uploaded their platform fragments via `actions/upload-artifact`
- **WHEN** the `publish` job runs `actions/download-artifact` with an explicit `path`
- **THEN** it SHALL read the fragments from the per-artifact download directories (not the build jobs' local `/tmp` paths, which do not survive the artifact round-trip)
- **AND** pass those resolved paths to `merge-latest-json.mjs`
