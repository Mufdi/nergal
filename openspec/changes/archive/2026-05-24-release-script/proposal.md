## Why

Cutting a release today is 7 manual steps: bump versions in 3 files, refresh `Cargo.lock`, hand-write a `CHANGELOG.md` section, commit, tag, push branch, push tag, then build artifacts + create the GitHub release + edit the previous release's banner. The follow-up `Projects/nergal/follow-ups/release-automation.md` documented the friction post-v0.1.1; v0.1.2 and v0.1.3 absorbed the cost again.

The mechanical operations (version bumps, Cargo.lock refresh, guards, commit/tag/push) are deterministic and trivially scriptable. The CHANGELOG section is NOT mechanical — historical entries are verbose, contextual, user-facing prose that a prefix-rewriting script would never match (validated empirically by comparing commit subjects vs the v0.1.3 CHANGELOG bullets in the repo). Auto-generation would be a quality regression.

The right split: script handles the deterministic parts; Claude generates the CHANGELOG section at ship time within the same session that orchestrates the vault ship ritual (see `Projects/nergal/Bug Workflow.md` step 6). Claude reads `git log <prev-tag>..HEAD` + relevant diffs + the BUG-NN entries from the just-archived working file, writes a contextual user-facing section, and prepends it to `CHANGELOG.md`. The script then runs, refusing to proceed if the section is missing — fail-loud rather than auto-fall-back to mechanical generation.

The second half of the release flow (build, GH release, signing, banner update) requires CI + signer key and is deferred to a future Change B (`release-ci-signed`).

## What Changes

- New `scripts/release.mjs` (Node, zero external deps — uses only `node:` stdlib):
  - Accepts `patch | minor | major | <explicit-version>` as the bump argument
  - Pre-flight guards: clean tree except `CHANGELOG.md`, on `main`, prev tag exists, new tag doesn't exist locally or on origin, **AND** `CHANGELOG.md` has a `## v<new>` section at top
  - Bumps `version` in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
  - Refreshes `src-tauri/Cargo.lock` via `cargo check`
  - Echoes the existing CHANGELOG section to stdout for visibility (read, not generated)
  - Stages 5 files, commits `chore(release): v<new>`, tags `v<new>`, pushes `main` + tag
  - Stops there
- `package.json` gains `"release"` and `"release:dry"` script entries
- Flags: `--dry-run` (no mutations), `--no-push` (local commit+tag only, for testing)

Claude's responsibility (orchestrated by the bug-workflow ship recommendation, NOT by this script) is to write the CHANGELOG section into the file before the script runs. The 2-step ship-of-the-repo flow is documented in `Bug Workflow.md` step 6 and in `bug-workflow/SKILL.md`.

## Capabilities

### New Capabilities

- `release-script`: defines the script's input contract (bump argument + flags), pre-flight guards (including the changelog-presence guard), version-bumping logic, Cargo.lock refresh, commit/tag/push sequence, stdout echo of the CHANGELOG section, error handling, and the explicit out-of-scope boundary (CHANGELOG content generation is Claude's responsibility, NOT the script's).

### Modified Capabilities

None.

## Impact

- **New files**:
  - `scripts/release.mjs` (~150 LOC, down from ~250 because CHANGELOG generation is out of scope)
  - `scripts/release.test.mjs` (`node --test` covering version computation + section extraction + presence guard)
- **Modified files (one-time)**:
  - `package.json` — adds `"release"` and `"release:dry"` entries to `scripts`
  - `CLAUDE.md` (cluihud) — adds "Release commands" section documenting the 2-step ship flow (Claude writes section → `pnpm release <bump>`)
  - `Bug Workflow.md` (vault) — ship ritual step 6 updated for the 2-step flow
  - `bug-workflow/SKILL.md` — rule about CHANGELOG responsibility updated (Claude writes at ship, not during resolve)
  - `reference_bug_workflow.md` (memory) — corresponding line updated
- **Modified files (at every release)**:
  - `package.json` (`version`)
  - `src-tauri/Cargo.toml` (`[package].version`)
  - `src-tauri/tauri.conf.json` (`version`)
  - `src-tauri/Cargo.lock` (refreshed)
  - `CHANGELOG.md` (section prepended by Claude before script runs)
- **Documentation cleanup on archive**:
  - `Projects/nergal/follow-ups/release-automation.md` → `Records/Projects/nergal/release-automation follow-up.md` with `type: record` + `subtype: plan-completed` + `date: <archive>` + closing header note linking to this change.
- **Out of scope (Change B `release-ci-signed`)**:
  - `pnpm tauri build` automation
  - GitHub release creation and asset upload
  - Bundle signing (Tauri signer key, `tauri-plugin-updater` integration)
  - Banner update on previous release
- **Out of scope (Claude's responsibility, not scriptable)**:
  - Generating the CHANGELOG section content. Read `git log <prev-tag>..HEAD` + relevant diffs + BUG-NN entries from the just-archived working file, write a contextual user-facing section, prepend to CHANGELOG.md. Happens in the orchestrating Claude session BEFORE the script runs.
- **Not affected**:
  - Bug-workflow during resolve — still does NOT touch CHANGELOG. CHANGELOG is touched only at ship time, by Claude.
  - Vault `Changelog.md` — symlink to repo file; reflects Claude's writes automatically.
  - Existing CHANGELOG content (v0.1.0-v0.1.3) — preserved byte-for-byte.
- **Risk surface**:
  - If Claude is unavailable at ship (no session, API down), the script's CHANGELOG-presence guard fails and aborts. The user can either write the section manually then re-run, or fall back to the 7-step manual process. No silent degradation.
  - Failure mid-run (commit fails, push fails) leaves modified files. The script prints recovery hints; no auto-rollback. Blast radius small.
  - Non-determinism: Claude's prose will vary between sessions for the same commits. Acceptable tradeoff for quality (matches the user's historical hand-written changelogs).
