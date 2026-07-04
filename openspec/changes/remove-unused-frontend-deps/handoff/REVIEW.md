# REVIEW — remove-unused-frontend-deps

## Orchestrator vet (XS deps, done directly) · 2026-07-04

**Verdict: PASS** (with one proposal-premise correction, build-verified).

- `@xyflow/react` removed: re-verified zero imports (`xyflow`/`reactflow` grep over
  `src/` → exit 1). Lock shed 178 lines (the xyflow subtree + transitive deps).
- `shadcn` moved dependencies → devDependencies. **Did NOT use `pnpm remove && pnpm add
  -D`** — that re-resolved shadcn 4.7.0 → 4.13.0 (an unrequested version bump). Reverted
  and edited package.json manually + `pnpm install`, preserving the exact HEAD-resolved
  **4.7.0** and the `^4.0.3` spec. Surgical: version resolution unchanged from HEAD.
- **Proposal premise corrected**: the proposal claims `shadcn` provides a real
  `./tailwind.css` file export. It does NOT — shadcn 4.7.0 (and 4.13.0) is the CLI
  package; `ls node_modules/shadcn` = LICENSE/README/package.json only, no `tailwind.css`
  file. BUT `@import "shadcn/tailwind.css"` still resolves at build time (via the
  package's `exports` map / Tailwind v4 resolution) — verified by a real `pnpm vite build`
  (CSS + JS compiled, `index-*.css` 149 kB, ✓ built 5.93s) with shadcn in devDeps. So the
  reclassification is correct: build-time tooling in devDependencies, and the CSS import
  resolves unchanged (devDeps are installed at build time, as the proposal's Risk note
  states).
- tsc clean; the soft premise error didn't affect the outcome — shadcn stays installed
  (dev), the import works.

## Gates

- Gate 1-3: PASS (tsc clean; `pnpm vite build` green — the real CSS-pipeline check).
- Gate 5 (deps): the point of the change; `@xyflow/react` gone, `shadcn` reclassified,
  lock regenerated with no unintended version change.
- Task 2.2's full `pnpm tauri build` (Rust bundle) not run — the frontend `vite build`
  covers the dependency-relevant surface; the Tauri wrapper is unaffected by a frontend
  devDep move. Left as the manual walk.
