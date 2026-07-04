## Why

`package.json` carries a production dependency with zero imports and a build-time tool misfiled as a runtime dependency. Both inflate the production dependency surface (install time, audit surface, lockfile churn) for no runtime benefit.

## What Changes

- **Remove `@xyflow/react`** (`package.json:49`, `^12.10.1`): zero imports anywhere in `src/` (verified — `xyflow`/`reactflow`/`ReactFlow` grep over `src/` returns nothing; the only matches in the repo are the `package.json` line itself and the lockfile).
- **Move `shadcn` (`package.json:61`, `^4.0.3`) from `dependencies` to `devDependencies`.** Correction to the original audit claim: `shadcn` is NOT import-free — `src/styles/globals.css:3` does `@import "shadcn/tailwind.css"` (a real `./tailwind.css` style export of the package). But that import is consumed at **build time** by the Tailwind/Vite pipeline and compiled into the bundle; nothing references the package at runtime. That is exactly the class of `tailwindcss` itself, which already lives in `devDependencies`. The package MUST NOT be removed — only reclassified.
- Regenerate `pnpm-lock.yaml` accordingly.

## Capabilities

### New Capabilities

- `frontend-dependency-hygiene`: declared production dependencies are imported by application source; build-time-only tools live in `devDependencies`.

### Modified Capabilities

_None._

## Impact

- **`package.json`**: one dependency removed, one moved to `devDependencies`; `pnpm-lock.yaml` regenerated.
- **Risk**: LOW — `@xyflow/react` has no references; `shadcn` stays installed (dev), so `globals.css` and the `components.json`-driven CLI keep working. `pnpm tauri build` runs with devDependencies installed, so the CSS import resolves unchanged.
- **Out of scope**: auditing the remaining dependencies for the same misfiling (e.g. `tw-animate-css` is also build-time CSS but is left as-is — sweep separately if desired); any Rust-side dependency work.
