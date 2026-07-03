## Context

CI exists only as two cross-compile `cargo check` gates born from the multiplatform port (CLAUDE.md "Cross-platform invariant"); the quality chain lives in CLAUDE.md and human discipline. The release pipeline signs and publishes three platforms with no verification prerequisite. A pending sibling change (`critical-path-test-coverage`) adds the tests this change would run — they pair but don't depend: gates are valuable with today's 136+ Rust tests already.

## Goals / Non-Goals

**Goals:**
- Every PR runs the checks relevant to what it touches; releases build only from a verified tree.
- Keep CI wall-clock proportionate (Rust jobs cached; frontend job is ~1-2 min).

**Non-Goals:**
- eslint/prettier adoption (follow-up); PR-time bundle builds; matrix-testing OSes for the test suite (ubuntu only — cross-OS compile gates already exist).

## Decisions

### D1: one workflow, per-job path scoping (not separate workflows)

Extend ci.yml's top-level `paths` to the union (src-tauri/**, src/**, package.json, pnpm-lock.yaml, tsconfig.json, index.html, vite.config.ts, scripts/**, .github/workflows/ci.yml), then scope jobs internally with `dorny/paths-filter` (or a tiny `git diff --name-only` step) so Rust jobs skip frontend-only PRs and vice versa. **Alternative — separate workflow files with their own `paths`**: simpler per-file, but branch-protection "required checks" get awkward (a skipped required workflow blocks merging; GitHub treats path-filtered-out workflows as pending). The single-workflow + conditional-jobs shape keeps required checks green-or-skipped correctly. (If the repo has no required-checks branch protection yet, this still keeps the door open.)

### D2: system deps for clippy/test on ubuntu

Tauri's `-sys` crates (webkit2gtk, gtk, soup) fail even `cargo clippy` without headers. Reuse the exact apt list from release.yml:48-59 (webkit2gtk-4.1, gtk-3, soup-3.0, appindicator3, librsvg2, patchelf, gstreamer base/good). Factor it into a composite action (`.github/actions/linux-deps/action.yml`) so ci.yml and release.yml share one list — the audit's docs finding (#19) showed this list already drifted from README once.

### D3: release gating via a `verify` job

`verify` (ubuntu): Full check + `npx tsc --noEmit` + `pnpm release:test`. `build-linux`/`build-macos`/`build-windows` gain `needs: [verify]`. **Alternative — trust the PR gates** (release tags only cut from main, which was PR-gated): pushes to main aren't guaranteed to have gone through a PR (the release script pushes main directly), so the tag-time gate is the only sound one. Cost: one extra ubuntu job per release.

### D4: first-run debt handling

Before merging the workflow change, run the Full check locally; fix cheap violations, `#[allow]` with a comment or file follow-ups for expensive ones. The gate must land green — a red-on-arrival gate gets disabled socially within a week.

### D5: rustfmt.toml pins defaults; .editorconfig stays minimal

`rustfmt.toml` contains only `edition` (matching Cargo.toml) — the codebase is already default-formatted; pinning prevents toolchain-default drift. `.editorconfig`: utf-8, lf, final-newline, 4-space Rust, 2-space TS/JSON/YAML — descriptive of current state, not a reformat.

## Risks / Trade-offs

- [Cache-less first runs are slow (~10 min Rust cold build)] → `Swatinem/rust-cache` (already used by ci.yml's cross-checks — verify and reuse the same config).
- [`cargo test` needs a display/dbus for some Tauri code paths] → tests are currently headless-safe (they run locally without a window); if a test needs GTK init, mark it `#[ignore]` in CI with a comment (task calls this out).
- [Path-filter logic errors silently skip gates] → keep the union-trigger + explicit job conditions reviewable in one file; add `workflow_dispatch` for manual full runs.

## Open Questions

- None blocking. Whether to make the new jobs *required* branch-protection checks is a repo-settings decision for the user post-merge.
