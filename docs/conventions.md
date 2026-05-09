# Coding conventions

Project-specific rules. Standard language conventions are not duplicated here.

## Rust style

- `for` loops over iterator chains when iteration is non-trivial.
- `let ... else` for early returns.
- Shadow variables to refine through pipelines (`let raw = ...; let parsed = ...`); do not invent prefixed renames.
- Newtypes over bare `String` / `bool` for domain identifiers.
- Match all enum variants explicitly. No catch-all wildcards on owned enums.
- No `unwrap()` / `expect()` outside tests — propagate with `anyhow` and `?`.
- `///` doc comments on every public item (RFC 1574 style: summary sentence, then optional sections / examples).
- No `//` inline comments explaining WHAT — only WHY.

## React / TypeScript

- Jotai atoms for all shared state. Keep atoms primitive and composable.
- Subscribe with `useAtomValue()`; mutate with `useSetAtom()`.
- Frontend → backend via `invoke<T>(command, args)`. Never construct shell commands client-side.
- Backend → frontend via Tauri `listen()`. Translate to atom updates in `src/stores/hooks.ts`.
- TailwindCSS utility classes; shadcn/ui primitives.
- Terminal lives outside React. `terminalService.ts` owns the canvas + glyph atlas and renders rows on `terminal:grid-update`. Do not introduce xterm.js.
- Keyboard shortcuts use `event.code` (not `event.key`) — WebKitGTK Linux bug.
- Verify `src/stores/shortcuts.ts` before adding a binding. Collisions silently break existing flows.

## Comments

Only WHY, never WHAT. A useful comment explains:

- A hidden constraint or invariant.
- A workaround for a specific upstream bug (link the issue or describe the symptom).
- A non-obvious motivation for a design choice.
- A pointer to authoritative context (`see openspec/specs/X/spec.md §Y`).

Restating the next line of code is not a comment.

## Project hygiene

- No TODO / FIXME — open an issue or an OpenSpec change instead.
- Absolute paths in tool calls.
- Run independent operations in parallel when reasonable.
- Read before Write/Edit.
