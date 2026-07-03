## 1. Pre-flight (first-run debt)

- [x] 1.1 Run the Full check locally (`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`); fix cheap violations, `#[allow]`-with-comment or file follow-ups for expensive ones. The gate must land green.

## 2. Shared deps action

- [x] 2.1 Create `.github/actions/linux-deps/action.yml` (composite) with the apt list from release.yml:48-59; switch release.yml's build-linux to use it.

## 3. ci.yml

- [x] 3.1 Extend trigger `paths` to the union (src-tauri/**, src/**, package.json, pnpm-lock.yaml, tsconfig.json, index.html, vite.config.ts, scripts/**, .github/workflows/ci.yml); add `workflow_dispatch`.
- [x] 3.2 Add `lint-and-test` (ubuntu): linux-deps action, `Swatinem/rust-cache` (mirror the cross-check jobs' cache config), clippy -D warnings, cargo test, fmt --check; conditioned to run only when `src-tauri/**` changed (design D1 path-filter step).
- [x] 3.3 Add `frontend-check` (ubuntu): pnpm setup + install, `npx tsc --noEmit`; conditioned on the frontend paths.
- [x] 3.4 Add `release-scripts-test`: `pnpm release:test`; conditioned on `scripts/**`.

## 4. release.yml

- [x] 4.1 Add `verify` job (linux-deps + Full check + `npx tsc --noEmit` + `pnpm release:test`); set `needs: [verify]` on build-linux, build-macos, build-windows.

## 5. Config files

- [x] 5.1 Add `rustfmt.toml` (edition pin only) and `.editorconfig` (utf-8, lf, final newline, 4-space Rust, 2-space TS/JSON/YAML). No reformat commits — both must describe current state.

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 6.2 `npx tsc --noEmit`
- [ ] 6.3 Open a scratch PR touching only `src/` → frontend-check runs, Rust jobs skip; a scratch PR touching `src-tauri/` → all Rust gates run. Cut the next release normally and confirm `verify` precedes builds.
