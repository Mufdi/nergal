# release-ci-signed Specification

## Purpose
TBD - created by archiving change release-ci-signed. Update Purpose after archive.
## Requirements
### Requirement: CI workflow triggered on v* tags builds and publishes Linux artifacts

A GitHub Actions workflow at `.github/workflows/release.yml` SHALL trigger on `push` events for tags matching the pattern `v*`. On trigger, it SHALL build Linux desktop artifacts (`.deb`, `.rpm`, `.AppImage`) via `pnpm tauri build` on `ubuntu-22.04`, then publish them as assets on a new GitHub release for the triggering tag.

#### Scenario: Tag push triggers the workflow
- **WHEN** a tag matching `v*` is pushed to origin (e.g., from `pnpm release patch`)
- **THEN** the workflow SHALL trigger
- **AND** SHALL check out the repo at the tag's commit

#### Scenario: Workflow builds bundles with required system deps
- **WHEN** the workflow runs
- **THEN** it SHALL install: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`
- **AND** SHALL run `pnpm install --frozen-lockfile`
- **AND** SHALL run `pnpm tauri build` with `GSTREAMER_PLUGINS_DIR=/usr/lib/x86_64-linux-gnu/gstreamer-1.0` and signing env vars set
- **AND** SHALL produce `.deb`, `.rpm`, `.AppImage`, and `.AppImage.sig` artifacts under `src-tauri/target/release/bundle/`

#### Scenario: Workflow caches cargo artifacts across runs
- **WHEN** the workflow runs and a previous cache exists keyed by `${{ hashFiles('src-tauri/Cargo.lock') }}`
- **THEN** `~/.cargo/registry`, `~/.cargo/git`, and `src-tauri/target` SHALL be restored before the build step
- **AND** the build SHALL be substantially faster than a cold build (~5-7 min vs ~10-15 min)

#### Scenario: Workflow creates GH release with artifacts
- **WHEN** the build succeeds
- **THEN** the workflow SHALL invoke `gh release create <tag>` with `--title "Nergal <tag>"`, the release body extracted from CHANGELOG.md, and the 4 artifacts as assets
- **AND** the release SHALL be public (not a draft, not a prerelease)

### Requirement: Bundles signed with Tauri signer key from GH repo secrets

Tauri's built-in signer SHALL produce a detached `.AppImage.sig` file alongside the `.AppImage` bundle during `pnpm tauri build`. The signing key (private) SHALL come from GitHub repo secret `TAURI_SIGNING_PRIVATE_KEY`, and the password from `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The corresponding public key SHALL live in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey` (safe to commit, public by design). Private key material SHALL NEVER be committed to the repo, exposed in workflow logs, or stored in any artifact.

#### Scenario: Signed AppImage produced when secrets are available
- **GIVEN** both `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are configured in repo secrets
- **WHEN** the build step runs
- **THEN** the workflow SHALL produce `Nergal_<version>_amd64.AppImage` AND `Nergal_<version>_amd64.AppImage.sig` in `src-tauri/target/release/bundle/appimage/`

#### Scenario: Missing signing secrets fail the build
- **GIVEN** either secret is missing or empty
- **WHEN** the build step runs
- **THEN** `pnpm tauri build` SHALL fail with a clear error from the Tauri signer
- **AND** the workflow SHALL NOT proceed to GH release creation

#### Scenario: Private key never appears in logs
- **WHEN** the workflow runs
- **THEN** GitHub Actions SHALL mask `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` values in all log output
- **AND** no echo, env-dump, or set-output command SHALL include the secret values verbatim

### Requirement: Release body auto-derived from CHANGELOG.md

A helper script `scripts/extract-release-body.mjs` SHALL read `CHANGELOG.md`, extract the section for a given version (header line `## v<version> — <date>`), strip the header line, and print the body bullets to stdout. The CI workflow SHALL invoke this helper with the current tag's version, capture stdout to a temp file, and pass that file to `gh release create --notes-file`.

The helper SHALL share the `extractChangelogSection` logic with `scripts/release.mjs` (via an `import`) so a CHANGELOG format change updates both at once.

#### Scenario: Helper extracts body for an existing version
- **GIVEN** `CHANGELOG.md` contains `## v0.1.4 — 2026-05-22\n\n* Added X\n* Fixed Y\n\n## v0.1.3 — ...`
- **WHEN** `node scripts/extract-release-body.mjs 0.1.4` is invoked
- **THEN** stdout SHALL contain exactly:
  ```
  * Added X
  * Fixed Y
  ```
- **AND** the script SHALL exit 0
- **AND** the version header line SHALL NOT be in stdout (it's redundant in the GH release UI)

#### Scenario: Helper fails when section missing
- **GIVEN** `CHANGELOG.md` does NOT contain a section for `0.1.4`
- **WHEN** the helper is invoked with `0.1.4`
- **THEN** the script SHALL exit non-zero with an error message identifying the missing version
- **AND** stdout SHALL be empty

### Requirement: Previous release body updated with "Latest release →" banner

After creating the new GH release, the workflow SHALL update the PREVIOUS release's body to prepend a banner block linking to the new release. The banner SHALL use the template at `scripts/release-banner.tpl.md`. If the previous release's body already contains a banner block from a prior release cycle, that block SHALL be stripped before prepending the new one (no stacking).

#### Scenario: Banner prepended to previous release
- **GIVEN** the previous release `v0.1.3` has body `<original content>`
- **WHEN** the workflow creates `v0.1.4` and runs the banner step
- **THEN** `v0.1.3`'s body SHALL become:
  ```
  > **Latest release →** [v0.1.4](https://github.com/Mufdi/nergal/releases/tag/v0.1.4)

  ---

  <original content>
  ```

#### Scenario: Existing banner replaced, not stacked
- **GIVEN** the previous release `v0.1.3` already has a banner pointing to `v0.1.4` (from a prior cycle)
- **WHEN** the workflow runs the banner step for a new `v0.1.5`
- **THEN** the old banner block matching `^> \*\*Latest release →\*\*.*\n\n---\n\n` SHALL be stripped first
- **AND** the new banner pointing to `v0.1.5` SHALL be prepended
- **AND** the result SHALL have exactly one banner block, pointing to the most recent release

### Requirement: App-side updater plugin enables in-app update checks

The Rust app SHALL integrate `tauri-plugin-updater` to enable signed update verification for AppImage installs. The plugin SHALL be configured via `src-tauri/tauri.conf.json`:

- `plugins.updater.active`: `true`
- `plugins.updater.endpoints`: `["https://github.com/Mufdi/nergal/releases/latest/download/latest.json"]`
- `plugins.updater.dialog`: `false` (the v0.1.3 About Update button is the UX surface)
- `plugins.updater.pubkey`: the public key generated in prereqs step 1

The existing About Update button (Settings → About, introduced v0.1.3) SHALL branch on install source:
- AppImage install → invoke the updater plugin's check/download/install flow
- `.deb` install → preserve v0.1.3 behavior (download to `~/Downloads/` + reveal in file manager)
- Dev build → preserve v0.1.3 behavior (amber banner, disabled action)

#### Scenario: AppImage user clicks Update — plugin invoked
- **GIVEN** the user is running the signed AppImage build
- **WHEN** they open Settings → About and click "Update"
- **THEN** the handler SHALL invoke `tauri_plugin_updater::Updater::check`
- **AND** if a newer version is available, the handler SHALL download + verify the signed AppImage + prompt the user to apply

#### Scenario: .deb user clicks Update — existing flow preserved
- **WHEN** a `.deb` user opens Settings → About and clicks "Update"
- **THEN** the handler SHALL preserve the existing v0.1.3 flow (download .deb to `~/Downloads/` + reveal in file manager)
- **AND** SHALL NOT attempt to use the updater plugin

#### Scenario: Updater dialog disabled — no second prompt
- **WHEN** the updater plugin detects an update
- **THEN** it SHALL NOT show its own native dialog
- **AND** the in-app About surface SHALL be the only prompt the user sees

### Requirement: Workflow failure does not silently degrade

If any CI step fails, the workflow SHALL exit with non-zero status and the failure SHALL surface via the standard GitHub Actions failure mechanisms (red X, optional email / webhook notifications). No silent partial release is permitted.

#### Scenario: Build fails — no release created
- **WHEN** `pnpm tauri build` fails (e.g., dependency resolution issue, signer error)
- **THEN** the workflow SHALL exit non-zero before reaching `gh release create`
- **AND** no GH release SHALL exist for the tag
- **AND** the user MAY retry by deleting the tag (`git push origin :v<X>`) and re-running `pnpm release <X> ...`

#### Scenario: Release creation succeeds but banner update fails
- **WHEN** the new release is created successfully but `gh release edit <prev>` fails
- **THEN** the workflow SHALL exit non-zero
- **AND** the new release SHALL be intact (it was created first; not rolled back)
- **AND** the user MAY manually update the previous release's banner via `gh release edit <prev>` or by re-running just the banner step

#### Scenario: First-time run with empty cache
- **GIVEN** no cache exists yet for `Cargo.lock`
- **WHEN** the workflow runs
- **THEN** the cache step SHALL be a no-op (no restore)
- **AND** the build SHALL proceed (slower) and create a fresh cache for subsequent runs

### Requirement: Key rotation procedure documented and supported

The signing key MAY be rotated. The procedure SHALL be: (1) generate a new keypair, (2) replace the GH secret `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, (3) update `tauri.conf.json` `plugins.updater.pubkey` with the new public key, (4) commit the pubkey change and cut a release. Users running an AppImage signed with the OLD key SHALL NOT be able to verify updates signed with the NEW key — they will need to manually download the AppImage from the new release.

#### Scenario: Rotation procedure cuts an unverifiable-from-old-keys release
- **GIVEN** the signing key has been rotated and the new pubkey committed
- **WHEN** a release is cut with the new key
- **AND** a user running an AppImage signed with the OLD key clicks About → Update
- **THEN** the updater plugin SHALL fail verification with a "pubkey mismatch" or equivalent error
- **AND** the user MAY manually download the new AppImage from the GH release page to recover

#### Scenario: Rotation is documented in CLAUDE.md
- **WHEN** a developer reads `CLAUDE.md` "Release commands" section
- **THEN** there SHALL be a "Key rotation" subsection describing the 4-step procedure above
- **AND** the trade-off (old AppImages cannot auto-update) SHALL be explicitly noted

