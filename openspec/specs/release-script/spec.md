# release-script Specification

## Purpose
TBD - created by archiving change release-script. Update Purpose after archive.
## Requirements
### Requirement: Single-command release for the deterministic flow

The repo SHALL expose `pnpm release <bump>` (backed by `scripts/release.mjs`) that performs version bumping, `Cargo.lock` refresh, commit, tag, and push as a single end-to-end operation with no interactive prompts. The CHANGELOG section content SHALL already exist in `CHANGELOG.md` before the script runs (written by Claude in the orchestrating session); the script does NOT generate it.

#### Scenario: pnpm release patch on clean main bumps to next patch and pushes
- **GIVEN** the current branch is `main`, the only dirty file is `CHANGELOG.md` (Claude prepended the new section in the same session), the previous tag is `v0.1.3`, and `CHANGELOG.md` contains a `## v0.1.4 — <date>` section at the top
- **WHEN** the user runs `pnpm release patch`
- **THEN** the script SHALL update `version` to `0.1.4` in `package.json`, `src-tauri/Cargo.toml` (under `[package]`), and `src-tauri/tauri.conf.json`
- **AND** SHALL refresh `src-tauri/Cargo.lock`
- **AND** SHALL commit the 5 modified files with message `chore(release): v0.1.4`
- **AND** SHALL create tag `v0.1.4`
- **AND** SHALL push `main` and `v0.1.4` to `origin`
- **AND** SHALL exit 0

#### Scenario: pnpm release minor / major bumps the corresponding semver component
- **GIVEN** the previous tag is `v0.1.3`
- **WHEN** the user runs `pnpm release minor`
- **THEN** the new version SHALL be `0.2.0`
- **WHEN** the user runs `pnpm release major`
- **THEN** the new version SHALL be `1.0.0`

#### Scenario: pnpm release with explicit version uses it verbatim
- **WHEN** the user runs `pnpm release 0.1.10` (or `v0.1.10`)
- **THEN** the new version SHALL be `0.1.10`

#### Scenario: pnpm release with invalid bump arg fails fast
- **WHEN** the user runs `pnpm release foo`
- **THEN** the script SHALL exit non-zero before any file modification
- **AND** print usage message listing accepted values

### Requirement: Pre-flight guards block unsafe runs

The script SHALL refuse to run if any of these conditions hold. All guards SHALL execute before any file is modified:

- Working tree has uncommitted changes OTHER than `CHANGELOG.md` (CHANGELOG.md may be dirty because Claude prepended the new section in the same session before invoking the script)
- Current branch is not `main`
- No previous tag exists
- The computed new tag exists locally (`git tag --list v<new>` non-empty)
- The computed new tag exists on origin
- `CHANGELOG.md` does NOT contain a `## v<new>` heading

#### Scenario: Dirty unrelated file blocks release
- **GIVEN** the user has uncommitted changes in `src/components/foo.tsx`
- **WHEN** the user runs `pnpm release patch`
- **THEN** the script SHALL exit non-zero with message containing "working tree has unrelated uncommitted changes"
- **AND** SHALL NOT modify any file

#### Scenario: CHANGELOG.md dirty but no other files is allowed
- **GIVEN** the only uncommitted change is `CHANGELOG.md` (Claude prepended the new section)
- **AND** all other guards pass
- **WHEN** the user runs `pnpm release patch`
- **THEN** the script SHALL proceed

#### Scenario: Wrong branch blocks release
- **WHEN** the user is on a feature branch and runs `pnpm release patch`
- **THEN** the script SHALL exit non-zero with message containing "release must run from main"

#### Scenario: Existing local tag blocks release
- **WHEN** computed new version is `0.1.4` and tag `v0.1.4` exists locally
- **THEN** the script SHALL exit non-zero with message containing "tag v0.1.4 already exists"

#### Scenario: Existing remote tag blocks release
- **WHEN** computed new tag doesn't exist locally but exists on origin
- **THEN** the script SHALL exit non-zero with message indicating remote tag conflict

#### Scenario: Missing CHANGELOG section blocks release with explicit hint
- **GIVEN** `CHANGELOG.md` does NOT contain a `## v0.1.4` heading
- **WHEN** the user runs `pnpm release patch` (computed new version 0.1.4)
- **THEN** the script SHALL exit non-zero with message: "CHANGELOG.md is missing the v0.1.4 section — generate it first in a Claude session (the bug-workflow ship ritual recommends this step as part of 'cortemos v0.1.4'). Aborting before any file is touched."
- **AND** SHALL NOT modify any file

### Requirement: Stdout echoes the CHANGELOG section before committing

After guards + version bumps + Cargo.lock refresh, the script SHALL read the new CHANGELOG section (from `## v<new>` to the next `## ` heading or EOF) and print it to stdout surrounded by `--- BEGIN CHANGELOG ENTRY ---` / `--- END CHANGELOG ENTRY ---` markers. The section was already written by Claude in the preceding step — this is for visibility only.

#### Scenario: Stdout shows the section between markers
- **GIVEN** `CHANGELOG.md` has a `## v0.1.4 — 2026-05-22` section with 3 bullets
- **WHEN** the script reaches the echo step
- **THEN** stdout SHALL contain the line `--- BEGIN CHANGELOG ENTRY ---`
- **AND** the version header line
- **AND** the 3 bullet lines
- **AND** the line `--- END CHANGELOG ENTRY ---`

### Requirement: Dry-run mode performs no mutations

The script SHALL accept `--dry-run` that performs all read operations (guards, version computation, section extraction) and prints what would be done, without modifying any file, committing, tagging, or pushing.

#### Scenario: --dry-run leaves the working tree state unchanged
- **WHEN** the user runs `pnpm release patch --dry-run`
- **THEN** the script SHALL print would-be operations
- **AND** print `WOULD UPDATE`, `WOULD COMMIT`, `WOULD TAG`, `WOULD PUSH` lines instead of executing
- **AND** the working tree SHALL NOT change as a result of the script
- **AND** the script SHALL exit 0 if all guards pass

### Requirement: --no-push flag for integration testing

The script SHALL accept `--no-push` that performs all local mutations (file edits, commit, tag) but skips the two `git push` calls.

#### Scenario: --no-push creates local commit + tag without pushing
- **GIVEN** the user is on a test branch with a fake CHANGELOG section
- **WHEN** the user runs `pnpm release patch --no-push`
- **THEN** the 5 files SHALL be modified
- **AND** the commit + tag SHALL be created locally
- **AND** `git push` SHALL NOT execute
- **AND** stdout SHALL print `WOULD PUSH origin main` and `WOULD PUSH origin v<new>`

### Requirement: Failures after pre-flight surface recovery hints

If any step after pre-flight guards fails, the script SHALL NOT attempt automatic rollback. It SHALL print the failing command's stderr followed by a recovery hint with exact commands.

#### Scenario: Push failure leaves local commit+tag intact with hint
- **GIVEN** version bump, Cargo.lock refresh, commit, and tag all succeeded
- **WHEN** `git push origin main` fails
- **THEN** the script SHALL print the push error
- **AND** print: "Local commit and tag created but push failed. To retry: `git push origin main && git push origin v<new>`. To undo: `git reset --hard HEAD~1 && git tag -d v<new>`."
- **AND** exit non-zero

#### Scenario: Commit failure leaves files modified
- **GIVEN** files modified, `git add` succeeded
- **WHEN** `git commit` fails (e.g., pre-commit hook rejected)
- **THEN** the script SHALL print the hook's stderr
- **AND** print: "Files modified but step 'commit' failed. Inspect with `git status` and `git diff`. To reset non-changelog files: `git checkout -- package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock`. CHANGELOG.md edits were written earlier by Claude — keep them or revert manually."
- **AND** exit non-zero
- **AND** SHALL NOT pass `--no-verify`

### Requirement: CHANGELOG content generation is explicitly out of scope

The script SHALL NOT generate the CHANGELOG section content. That responsibility belongs to the Claude session orchestrating the release: Claude reads `git log <prev-tag>..HEAD` and relevant context (diffs, BUG-NN entries from the just-archived working file), writes a contextual user-facing section, and prepends it to `CHANGELOG.md` in the session BEFORE invoking the script. The script's only interactions with CHANGELOG.md are (a) verifying the section exists as a pre-flight guard, (b) echoing it to stdout for visibility, and (c) staging it for the commit.

This separation deliberately exchanges determinism for changelog quality. The user has historically written rich user-facing prose that mechanical prefix-rewriting cannot match. Quality matters more than reproducibility for this project's release notes.

#### Scenario: Script run without prior Claude section fails fast
- **GIVEN** no Claude session wrote the new section
- **WHEN** the user attempts `pnpm release patch`
- **THEN** the script SHALL fail at the CHANGELOG-presence guard
- **AND** the user MAY (a) write the section manually then re-run, or (b) fall back to the 7-step legacy manual flow
- **AND** the script SHALL NOT silently degrade to mechanical changelog generation

### Requirement: Out-of-scope operations explicitly deferred to Change B

The script SHALL NOT:
- Invoke `pnpm tauri build` or any artifact-producing command
- Create or upload to a GitHub release
- Sign artifacts
- Edit any previous release's body or banner

These responsibilities SHALL be handled by capability `release-ci-signed` — a GitHub Actions workflow triggered by the tag push that the script performs at the end of its run. Post-script, the user SHALL NOT need to run any additional local command; CI handles build, signing, release creation, and banner update.

#### Scenario: Script terminates after push with CI handoff message
- **WHEN** all script steps complete successfully
- **THEN** the script SHALL exit 0 immediately after `git push origin v<new>`
- **AND** SHALL print exactly: `CI workflow will now build, sign, and publish the release. Monitor at https://github.com/Mufdi/nergal/actions`
- **AND** SHALL NOT print the legacy "manual next steps" reminder (build / gh release create / banner edit)

#### Scenario: --no-push mode still recommends CI awareness
- **WHEN** the script runs in `--no-push` mode and completes the local commit + tag
- **THEN** the script SHALL print: `Released <tag> locally (--no-push). To push and trigger CI: git push origin main && git push origin <tag>`
- **AND** SHALL NOT push automatically

