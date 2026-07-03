# ci-verification-gates Specification

## Purpose
TBD - created by archiving change ci-quality-gates. Update Purpose after archive.
## Requirements
### Requirement: PR verification gates

CI SHALL run `cargo clippy -- -D warnings`, `cargo test`, and `cargo fmt --check` on every PR touching `src-tauri/**`, and `npx tsc --noEmit` on every PR touching the frontend surface (`src/**`, `package.json`, `pnpm-lock.yaml`, `tsconfig.json`, `index.html`, `vite.config.ts`). PRs touching `scripts/**` SHALL run `pnpm release:test`.

#### Scenario: frontend-only PR is no longer unchecked

- **WHEN** a PR changes only files under `src/`
- **THEN** the frontend check job runs `npx tsc --noEmit` (previously zero CI ran)

#### Scenario: Rust PR runs the full chain

- **WHEN** a PR changes `src-tauri/**`
- **THEN** clippy (deny warnings), tests, and fmt --check all run and must pass, alongside the existing macOS/Windows cross-compile gates

### Requirement: Release builds are gated on verification

The release workflow SHALL run a `verify` job (Rust Full check + `npx tsc --noEmit` + `pnpm release:test`) and the platform build jobs SHALL depend on it, so no bundle is built or published from a tree that fails verification.

#### Scenario: failing verify blocks the release

- **GIVEN** a `v*` tag push where a test fails
- **WHEN** the release workflow runs
- **THEN** `verify` fails and no `build-*` or `publish` job executes

### Requirement: Formatting configuration is pinned

The repo SHALL contain a `rustfmt.toml` pinning the formatting baseline and an `.editorconfig` describing indentation/EOL conventions, so `cargo fmt --check` results are stable across contributor toolchains.

#### Scenario: consistent fmt across machines

- **WHEN** two contributors on different rustfmt minor versions run `cargo fmt --check`
- **THEN** both get the same verdict on unmodified code

