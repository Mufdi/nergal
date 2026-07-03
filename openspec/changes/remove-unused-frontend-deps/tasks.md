## 1. Dependency cleanup

- [ ] 1.1 Re-verify zero imports right before editing: grep `xyflow`, `reactflow`, `ReactFlow` over `src/` — must return nothing. Then `pnpm remove @xyflow/react`.
- [ ] 1.2 Move `shadcn` to `devDependencies` (`pnpm remove shadcn && pnpm add -D shadcn` or manual edit + `pnpm install`). Do NOT remove it — `src/styles/globals.css:3` imports `shadcn/tailwind.css` at build time and `components.json` drives its CLI.

## 2. Verification

- [ ] 2.1 `npx tsc --noEmit`
- [ ] 2.2 `pnpm tauri build` completes and the app launches with intact styling (the shadcn semantic colors block in `globals.css` renders as before).
