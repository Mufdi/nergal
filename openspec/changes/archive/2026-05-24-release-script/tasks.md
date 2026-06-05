> Checkboxes marked retroactively on 2026-06-05 (post-archive audit): in production since the v0.1.4 cut — `pnpm release` drove v0.1.4 (including `--dry-run`/`--no-push` paths covered by `pnpm release:test`). Checkboxes were never tracked during implementation.

## 1. Script foundation

- [x] 1.1 Create `scripts/release.mjs` with shebang `#!/usr/bin/env node`. Use only `node:` stdlib modules (`node:fs`, `node:child_process`, `node:path`, `node:url`, `node:test`). No external dependencies.
- [x] 1.2 Parse `process.argv`. Accept positional bump arg: `patch | minor | major` or explicit semver (`v0.1.10` or `0.1.10`). Accept flags: `--dry-run`, `--no-push`, `--help`. Reject everything else with usage message + exit 1.
- [x] 1.3 Implement `--help` describing args, flags, and the 2-step ship flow (Claude writes CHANGELOG section → script handles mechanics).
- [x] 1.4 Add `"release": "node scripts/release.mjs"` and `"release:dry": "node scripts/release.mjs --dry-run"` to `package.json` `scripts`.

## 2. Pre-flight guards

- [x] 2.1 Verify working tree: `git status --porcelain` may contain ONLY `CHANGELOG.md` as modified (Claude wrote the new section in the same session). Any other dirty file → fail with "working tree has unrelated uncommitted changes — commit or stash before release". Empty status is also OK (no Claude pre-write happened; the changelog guard at 2.6 will catch the missing section).
- [x] 2.2 Verify branch is `main`: `git rev-parse --abbrev-ref HEAD` must equal `main`. Fail with "release must run from main, currently on <X>".
- [x] 2.3 Verify previous tag exists: `git describe --tags --abbrev=0` must succeed. Fail with "no previous tag found — tag the previous release manually first".
- [x] 2.4 Compute new version (per bump arg). Verify new tag doesn't exist locally: `git tag --list v<new>` must be empty.
- [x] 2.5 Verify new tag doesn't exist on origin: `git ls-remote --tags origin refs/tags/v<new>` must be empty.
- [x] 2.6 **Verify CHANGELOG.md has a section for the new version**: read `CHANGELOG.md`, look for a heading line matching `^## v<new>\b`. If absent, fail with: `CHANGELOG.md is missing the v<new> section — generate it first in a Claude session (the bug-workflow ship ritual recommends this step as part of "cortemos v<new>"). Aborting before any file is touched.`
- [x] 2.7 In `--dry-run` and `--no-push` modes, all guards still run identically.

## 3. Version bumping

- [x] 3.1 Read `package.json`, parse JSON, extract current `version`. Compute new version per bump rules. Validate result is a valid semver triple.
- [x] 3.2 Update `package.json`: read raw, regex-replace the `"version": "<old>"` line, write back. Preserve indentation and trailing newline.
- [x] 3.3 Update `src-tauri/Cargo.toml`: read raw, find the `[package]` section, regex-replace `version = "<old>"` within that section only. Write back.
- [x] 3.4 Update `src-tauri/tauri.conf.json`: JSON parse, set top-level `version` field, JSON stringify with 2-space indent and trailing newline (matches Tauri's expected formatting).
- [x] 3.5 Refresh `src-tauri/Cargo.lock`: spawn `cargo check --offline` from `src-tauri/`; on failure (new dependency), retry without `--offline`. Verify `Cargo.lock` mtime updated; if not, fail with hint.
- [x] 3.6 In `--dry-run`, compute changes but do NOT write. Print `WOULD UPDATE package.json: <old> -> <new>` etc.

## 4. CHANGELOG echo for visibility

- [x] 4.1 Read `CHANGELOG.md`. Extract the section for the new version: from the `## v<new>` line to the next `## ` heading (or EOF).
- [x] 4.2 Print the extracted section to stdout surrounded by `--- BEGIN CHANGELOG ENTRY ---` / `--- END CHANGELOG ENTRY ---` markers.
- [x] 4.3 In `--dry-run`, this step still runs — useful to verify Claude's section looks right before committing to the run.

## 5. Commit + tag + push

- [x] 5.1 Stage 5 files explicitly: `git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock CHANGELOG.md`. Never `git add -A`.
- [x] 5.2 Commit: `git commit -m "chore(release): v<new>"`. Do NOT pass `--no-verify`.
- [x] 5.3 Tag: `git tag v<new>` (lightweight). Verify with `git tag --list v<new>`.
- [x] 5.4 Push branch: `git push origin main`. Capture stderr.
- [x] 5.5 Push tag: `git push origin v<new>`.
- [x] 5.6 Print summary: `Released v<new>` + commit sha + tag + reminder about Change B for build/upload tail.
- [x] 5.7 `--dry-run`: skip 5.1-5.5, print `WOULD COMMIT/TAG/PUSH`.
- [x] 5.8 `--no-push`: run 5.1-5.3, skip 5.4-5.5, print `WOULD PUSH origin main` and `WOULD PUSH origin v<new>`.

## 6. Error handling

- [x] 6.1 Wrap subprocess calls (`git`, `cargo`) in a helper that captures stderr. On non-zero exit: print `[<command>] <stderr>` + a hint.
- [x] 6.2 If a step after version bumping fails, do NOT auto-rollback. Print: "Files modified but step '<step-name>' failed. Inspect with `git status` and `git diff`. To reset: `git checkout -- package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock`. CHANGELOG.md edits were written earlier by Claude — keep them or revert manually."
- [x] 6.3 If push fails after commit+tag succeeded locally: "Local commit and tag created but push failed. To retry: `git push origin main && git push origin v<new>`. To undo: `git reset --hard HEAD~1 && git tag -d v<new>`."

## 7. Tests

- [x] 7.1 Create `scripts/release.test.mjs` using `node --test`.
- [x] 7.2 Test version computation: `bumpVersion('0.1.3', 'patch')` → `'0.1.4'`, `'minor'` → `'0.2.0'`, `'major'` → `'1.0.0'`, explicit `'0.1.10'` → `'0.1.10'`, invalid → throws.
- [x] 7.3 Test section extraction: given a fixture CHANGELOG string with `## v0.1.4 — 2026-05-22\n\n* Added X\n* Fixed Y\n\n## v0.1.3 — ...`, the extracted section equals exactly `## v0.1.4 — 2026-05-22\n\n* Added X\n* Fixed Y`.
- [x] 7.4 Test changelog-presence guard: given a CHANGELOG without `## v0.1.4`, guard returns missing-section error.
- [x] 7.5 Test working-tree guard: given `git status --porcelain` output with only `M CHANGELOG.md`, guard passes; with `M CHANGELOG.md\nM src/foo.tsx`, guard fails.
- [x] 7.6 Smoke test on real repo: prepend a fake `## v0.1.4 — <today>` section to CHANGELOG.md, run `pnpm release patch --dry-run`, verify output sane, then `git checkout -- CHANGELOG.md`.
- [x] 7.7 Integration test on throwaway branch: create test branch, write fake CHANGELOG section, run `pnpm release patch --no-push`, verify 5 files modified + commit + tag locally. Reset with `git reset --hard <orig> && git tag -d v<new>`.

## 8. Documentation

- [x] 8.1 Add "Release commands" section to cluihud `CLAUDE.md`:
  - 2-step ship flow: "Cortemos v0.1.X" in Claude session → Claude reads commits + writes section to CHANGELOG.md → user runs `pnpm release <patch|minor|major>` → script handles mechanics + push.
  - Fallback (no Claude available): 7-step manual flow per the archived `release-automation` follow-up.
- [x] 8.2 Update vault `Bug Workflow.md` ship ritual step 6 to describe the 2-step ship-of-the-repo flow.
- [x] 8.3 Update `bug-workflow/SKILL.md` rule about CHANGELOG: Claude writes at ship, NOT during resolve.
- [x] 8.4 Update memory `reference_bug_workflow.md` correspondingly.
- [x] 8.5 On archive of this change: `mv ~/Documents/Obsidian23/Projects/nergal/follow-ups/release-automation.md ~/Documents/Obsidian23/Records/Projects/nergal/release-automation\ follow-up.md` + update frontmatter to `type: record` + `subtype: plan-completed` + `date: <archive date>` + add closing header note linking to this OpenSpec change.

## 9. Full verification

- [x] 9.1 `node --test scripts/release.test.mjs` — all tests pass.
- [x] 9.2 `pnpm release patch --dry-run` against a temp branch with a fake CHANGELOG section — verify output, no mutations, exit 0.
- [x] 9.3 Real release of v0.1.4 (when ready): Claude session writes CHANGELOG section, then `pnpm release patch` does the rest. Verify v<new> commit + tag pushed, 5 files updated, exit 0.
- [x] 9.4 Follow with manual build + GH release per the legacy 3-step tail (until Change B lands).
