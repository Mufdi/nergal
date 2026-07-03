## Why

Nothing enforces the project's documented quality bar. `.github/workflows/ci.yml` has exactly two jobs — `macos-cross-check` and `windows-check`, both `cargo check --target <triple>` only (ci.yml:31-55, 69-93) — no `cargo test`, no `clippy`, no `fmt`, no `tsc`. The path filter (ci.yml:4-16) is `src-tauri/**` only, so a frontend-only PR runs **zero** CI. `release.yml` publishes signed bundles with no quality gate ahead of the build jobs (`publish` needs only the three builds, release.yml:250). `pnpm release:test` (package.json:16) never runs in CI. There is no `rustfmt.toml`, `.editorconfig`, or any eslint config in the repo. CLAUDE.md documents a "Full check" chain (`cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit`) that only convention runs.

## What Changes

- **New `lint-and-test` job (ubuntu)** in ci.yml: `cargo clippy -- -D warnings && cargo test && cargo fmt --check`, with the Tauri system deps the release workflow already installs (release.yml:48-59 apt list — the `-sys` crates need them even for clippy) and Rust caching.
- **New `frontend-check` job (ubuntu)**: `pnpm install && npx tsc --noEmit`; **extend the path filter** with `src/**`, `package.json`, `pnpm-lock.yaml`, `tsconfig.json`, `index.html`, `vite.config.ts` so frontend PRs get CI. Rust jobs stay filtered to `src-tauri/**` via per-job `paths` logic (design.md D2).
- **`release-scripts-test` job**: `pnpm release:test` on PRs touching `scripts/**` and on the release path.
- **Gate releases**: add a `verify` job to release.yml (same lint-and-test + tsc content) and make the three build jobs `needs: [verify]`.
- **Add `rustfmt.toml`** (pin the already-in-use default style so `fmt --check` is stable across toolchains) **and `.editorconfig`** (indent/eol basics matching current conventions).
- **Explicitly deferred**: frontend eslint adoption — larger (config choice + fixing existing violations); noted as a follow-up candidate, not part of this change.

## Capabilities

### New Capabilities

- `ci-verification-gates`: the documented Full check runs on every PR (scoped by area) and before any release build.

### Modified Capabilities

_None._

## Impact

- **`.github/workflows/ci.yml`**: +3 jobs, extended trigger paths; existing cross-compile jobs untouched.
- **`.github/workflows/release.yml`**: +1 `verify` job; `build-*` gain `needs: [verify]` (adds ~5-10 min to release wall-clock — acceptable, releases are rare and currently un-gated).
- **New files**: `rustfmt.toml`, `.editorconfig`.
- **Risk**: LOW-MEDIUM — the first honest run may surface existing clippy/test/fmt failures that were never enforced; implementation runs the Full check locally first and fixes or explicitly allows findings before enabling the gate (design.md D4).
- **Out of scope**: eslint/prettier adoption; building bundles in PR CI (check/test only — no `pnpm tauri build`, so `patchelf`/gstreamer AppImage concerns don't apply); coverage tooling.
