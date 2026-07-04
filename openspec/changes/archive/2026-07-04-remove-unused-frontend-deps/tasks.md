## 1. Dependency cleanup

- [x] 1.1 Re-verify zero imports right before editing: grep `xyflow`, `reactflow`, `ReactFlow` over `src/` — must return nothing. Then `pnpm remove @xyflow/react`.
- [x] 1.2 Move `shadcn` to `devDependencies` (`pnpm remove shadcn && pnpm add -D shadcn` or manual edit + `pnpm install`). Do NOT remove it — `src/styles/globals.css:3` imports `shadcn/tailwind.css` at build time and `components.json` drives its CLI.

## 2. Verification

- [x] 2.1 `npx tsc --noEmit`
- [ ] 2.2 `pnpm tauri build` completes and the app launches with intact styling (the shadcn semantic colors block in `globals.css` renders as before).
  - 2.2 (full `pnpm tauri build` walk) pending — covered at the dependency level by a green `pnpm vite build` (CSS pipeline incl. the shadcn import resolves). NOTE: the proposal's "shadcn provides a ./tailwind.css file export" is imprecise — no physical file, but the import resolves via the package exports map; build confirmed green. See REVIEW.md.
