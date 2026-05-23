## 1. One-time prereqs (user-driven, documented)

- [ ] 1.1 Generate Tauri signer keypair: `pnpm tauri signer generate -w ~/.tauri/cluihud-updater.key`. Set a strong password during the interactive prompt; this is needed for the GH secret.
- [ ] 1.2 Open the keyfile, copy the **public key** line (after `untrusted comment: minisign public key ...`). It's a base64-encoded blob starting with `RW...`.
- [ ] 1.3 Paste the pubkey into `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
- [ ] 1.4 Add GitHub repo secrets at https://github.com/Mufdi/nergal/settings/secrets/actions:
  - `TAURI_SIGNING_PRIVATE_KEY` — entire contents of `~/.tauri/cluihud-updater.key` (including comments)
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password from step 1.1
- [ ] 1.5 Verify the keyfile is gitignored. Add `~/.tauri/` to a global `.gitignore` (or rely on it being outside the repo). Confirm with `git status` that the keyfile is invisible.

## 2. Tauri updater plugin (app side)

- [ ] 2.1 Add to `src-tauri/Cargo.toml` under `[dependencies]`:
  ```toml
  tauri-plugin-updater = "2"
  ```
- [ ] 2.2 Refresh `src-tauri/Cargo.lock`: `cd src-tauri && cargo check`.
- [ ] 2.3 Initialize the plugin in `src-tauri/src/lib.rs::run()`:
  ```rust
  .plugin(tauri_plugin_updater::Builder::new().build())
  ```
  Add the `.plugin(...)` call to the existing `tauri::Builder` chain, alongside the other plugins.
- [ ] 2.4 Configure the updater in `src-tauri/tauri.conf.json`:
  ```json
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": ["https://github.com/Mufdi/nergal/releases/latest/download/latest.json"],
      "dialog": false,
      "pubkey": "<pubkey from step 1.3>"
    }
  }
  ```
  Rationale: `dialog: false` because the v0.1.3 Settings → About Update button is already the UX surface; we don't want a second prompt.
- [ ] 2.5 Wire the existing About Update button (currently calls the `.deb` download flow) to branch on install source:
  - If running from `.AppImage` → invoke `tauri_plugin_updater` check/download flow
  - If running from `.deb` → preserve existing behavior (download to `~/Downloads/` + reveal in file manager)
  - If running from dev → preserve existing amber banner / disabled behavior
- [ ] 2.6 Verify the app builds + runs locally with the new plugin: `cd src-tauri && cargo check && cargo clippy -- -D warnings && cd .. && pnpm tauri dev`. Open Settings → About and confirm the UI doesn't regress.

## 3. Release body extractor

- [ ] 3.1 Refactor `scripts/release.mjs` to ensure `extractChangelogSection` is exported (already is per Change A — verify) and reusable.
- [ ] 3.2 Create `scripts/extract-release-body.mjs`:
  - Imports `extractChangelogSection` from `./release.mjs`
  - Takes a version arg: `node scripts/extract-release-body.mjs 0.1.4`
  - Reads `CHANGELOG.md`, extracts the section
  - Strips the `## v0.1.4 — DATE` header line (GH release UI shows version + date already)
  - Prints body to stdout
  - If section missing, exit 1 with error to stderr
- [ ] 3.3 Create `scripts/extract-release-body.test.mjs` with `node --test`:
  - Extracts body with header stripped
  - Missing section → exit 1 / null
  - Real-CHANGELOG-shape fixture
- [ ] 3.4 Add `package.json` script: `"release:body": "node scripts/extract-release-body.mjs"`.
- [ ] 3.5 Run `pnpm release:test` + `node --test scripts/extract-release-body.test.mjs` — all pass.

## 4. CI workflow

- [ ] 4.1 Create `.github/workflows/release.yml`:
  - Trigger: `on: push: tags: ['v*']`
  - Job `build-linux` on `runs-on: ubuntu-22.04`
  - Permissions: `contents: write` (needed for `gh release create`)
  - Steps in order:
    1. `actions/checkout@v4`
    2. `pnpm/action-setup@v3` with version 10
    3. `actions/setup-node@v4` with node-version 22, cache pnpm
    4. `dtolnay/rust-toolchain@stable`
    5. `actions/cache@v4` for `~/.cargo/registry`, `~/.cargo/git`, `src-tauri/target` keyed on `${{ runner.os }}-cargo-${{ hashFiles('src-tauri/Cargo.lock') }}`
    6. Install system deps via `apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libappindicator3-dev librsvg2-dev patchelf gstreamer1.0-plugins-base gstreamer1.0-plugins-good`
    7. `pnpm install --frozen-lockfile`
    8. Build step with env vars `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `GSTREAMER_PLUGINS_DIR=/usr/lib/x86_64-linux-gnu/gstreamer-1.0`, command `pnpm tauri build`
    9. Extract release body: `node scripts/extract-release-body.mjs "${GITHUB_REF_NAME#v}" > /tmp/release-body.md`
    10. Create release: `gh release create "$GITHUB_REF_NAME" --title "Nergal $GITHUB_REF_NAME" --notes-file /tmp/release-body.md src-tauri/target/release/bundle/deb/*.deb src-tauri/target/release/bundle/rpm/*.rpm src-tauri/target/release/bundle/appimage/*.AppImage src-tauri/target/release/bundle/appimage/*.AppImage.sig`
    11. Update previous banner: `node scripts/update-previous-banner.mjs "$GITHUB_REF_NAME"`
  - All `gh` invocations get `env: GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`
- [ ] 4.2 Lint the workflow file with `actionlint` if installed locally (optional, GH Actions also validates on push).

## 5. Previous-release banner update

- [ ] 5.1 Create `scripts/release-banner.tpl.md`:
  ```markdown
  > **Latest release →** [{{NEW_TAG}}](https://github.com/Mufdi/nergal/releases/tag/{{NEW_TAG}})

  ---

  {{ORIGINAL_BODY}}
  ```
- [ ] 5.2 Create `scripts/update-previous-banner.mjs`:
  - Takes new tag as arg.
  - Uses `gh api repos/Mufdi/nergal/releases` to fetch all releases (or use `gh release list`).
  - Identifies the previous release (the one before the new tag; sort by `created_at` descending, skip the one matching `<new tag>`, take first).
  - Fetches its body via `gh api repos/Mufdi/nergal/releases/<id>`.
  - Strips any existing banner block matching `^> \*\*Latest release →\*\*.*\n\n---\n\n` (re-runs safely).
  - Reads `scripts/release-banner.tpl.md`, substitutes `{{NEW_TAG}}` and `{{ORIGINAL_BODY}}`.
  - Calls `gh release edit <prev-tag> --notes-file <rendered-file>`.
  - On failure, exit non-zero with diagnostic.
- [ ] 5.3 Add `package.json` script: `"release:banner": "node scripts/update-previous-banner.mjs"`.

## 6. Update Change A script's final message

- [ ] 6.1 In `scripts/release.mjs`, replace the final stdout block:
  ```
  Released ${newTag} (commit ...).
  CI workflow will now build, sign, and publish the release.
  Monitor at https://github.com/Mufdi/nergal/actions
  ```
  Remove the "Next manual steps" block (tauri build, gh release create) — they're CI now.
- [ ] 6.2 Update `--help` text to reflect the same change (drop "Build + GH release remain manual").

## 7. Documentation updates

- [ ] 7.1 Update cluihud `CLAUDE.md` "Release commands" section:
  - Remove the post-script manual build/release/banner block (CI handles it).
  - Add subsection "First-time signing key setup" with step 1 of this tasks file (key generation + secrets upload).
  - Add subsection "Key rotation" with the rotation procedure: regenerate key, swap GH secrets, update `tauri.conf.json` pubkey, cut a release. Note that AppImages signed with the old key cannot verify updates signed with the new key (expected behavior).
- [ ] 7.2 Update vault `Bug Workflow.md` step 6 — post-script the user just waits for CI; remove the manual build/release reminder.
- [ ] 7.3 Update `bug-workflow/SKILL.md` rule about post-script behavior correspondingly.
- [ ] 7.4 Update memory `project_v0_1_4_appimage_signing.md`:
  - If anything is still load-bearing post-archive (e.g., key file location), trim to that.
  - Otherwise mark for deletion (the OpenSpec record subsumes it).

## 8. Validation

- [ ] 8.1 Unit tests pass: `pnpm release:test` + `node --test scripts/extract-release-body.test.mjs`.
- [ ] 8.2 Local build with signing env vars set (from a local copy of the key — manually export them in shell, do NOT commit):
  ```bash
  TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/cluihud-updater.key)" \
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<password>" \
  GSTREAMER_PLUGINS_DIR=/usr/lib/x86_64-linux-gnu/gstreamer-1.0 \
    pnpm tauri build
  ```
  Verify `src-tauri/target/release/bundle/appimage/Nergal_*.AppImage` AND `Nergal_*.AppImage.sig` are produced.
- [ ] 8.3 CI smoke test: create a test branch, push a test tag like `v0.0.0-test1`. Verify the workflow:
  - Triggers
  - Builds successfully (~10-15 min cold cache; ~5-7 min warm)
  - Creates a GH release with all artifacts
  - Updates the previous release's banner
  
  Cleanup: delete the test release (`gh release delete v0.0.0-test1 --yes`) and the test tag (`git push origin :v0.0.0-test1`).
- [ ] 8.4 First real release: cut v0.1.X via the full flow (Claude writes CHANGELOG section → `pnpm release patch` → wait for CI). Verify:
  - CI fires within ~30s of tag push
  - GH release lands with `.deb`, `.rpm`, `.AppImage`, `.AppImage.sig` artifacts
  - Previous release's banner updates correctly
  - Download the AppImage on a test machine, run it, open Settings → About → Update, verify the check doesn't error with "pubkey verification failed" (no actual newer release exists at this point — just verifying the plugin can talk to the endpoint without crashing)
- [ ] 8.5 Test rotation procedure: regenerate a fresh key (NOT in production, in a test environment), swap secrets, cut a test release, verify the OLD AppImage cannot verify the NEW update (expected — proves the rotation works).

## 9. Archive cleanup on close

- [ ] 9.1 Move `Projects/nergal/follow-ups/release-automation.md` to `Records/Projects/nergal/release-automation follow-up.md`:
  ```bash
  mv ~/Documents/Obsidian23/Projects/nergal/follow-ups/release-automation.md \
     ~/Documents/Obsidian23/Records/Projects/nergal/"release-automation follow-up.md"
  ```
  Update frontmatter to `type: record` + `subtype: plan-completed` + `date: <archive date>`. Add closing header note linking BOTH this OpenSpec change (`release-ci-signed`) AND Change A (`release-script`) since both close the follow-up together.
- [ ] 9.2 Update or delete memory `project_v0_1_4_appimage_signing.md` based on whether anything is still load-bearing.
