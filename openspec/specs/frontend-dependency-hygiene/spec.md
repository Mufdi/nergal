# frontend-dependency-hygiene Specification

## Purpose
TBD - created by archiving change remove-unused-frontend-deps. Update Purpose after archive.
## Requirements
### Requirement: Production dependencies are runtime-imported

Every package under `dependencies` in `package.json` SHALL be imported by application source (`src/**` or `index.html`); packages consumed only at build time (CLI tools, CSS compiled into the bundle) SHALL live under `devDependencies`.

#### Scenario: zero-import dependency is removed

- **GIVEN** `@xyflow/react` has no import in `src/`
- **WHEN** the cleanup lands
- **THEN** `@xyflow/react` is absent from `package.json` and `pnpm-lock.yaml`, and `pnpm tauri build` succeeds

#### Scenario: build-time tool is reclassified, not removed

- **GIVEN** `src/styles/globals.css` imports `shadcn/tailwind.css` at build time
- **WHEN** `shadcn` moves to `devDependencies`
- **THEN** `npx tsc --noEmit` and `pnpm tauri build` still succeed and the compiled CSS is unchanged

