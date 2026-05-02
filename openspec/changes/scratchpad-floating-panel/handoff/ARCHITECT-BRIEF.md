# ARCHITECT-BRIEF — scratchpad-floating-panel

## Project mission (from CLAUDE.md)

cluihud is a desktop app wrapper for Claude Code CLI on Linux (Tauri v2 + React 19). It is **not** a standalone terminal, **not** a Claude Code reimplementation, **not** an agent framework. Useful work improves the experience of using Claude Code: plan editing UX, task visibility, session navigation, hook-driven panels, keyboard shortcuts, workspace/worktree management.

The scratchpad fits this mission: it removes a friction (user opens Sublime outside cluihud to take quick notes) by adding a Claude-Code-adjacent surface inside the app. It does **not** reimplement Claude functionality, **not** replace Obsidian for serious notes, **not** become a session-coupled feature.

## Context

**Discovery phase**: covered via direct conversation with the user (4 rounds of design iteration). No `/discovery` invocation needed; the design space, options, and trade-offs were explored conversationally and crystallized into the proposal. The conversation produced explicit user confirmations on every contested decision (modal vs floating panel, glass vs opacity, files vs DB, naming convention, soft-delete strategy).

**Why no /discovery**: the design space was small enough to negotiate directly. `/discovery` would have re-asked the same questions the user already answered.

**User-stated emphasis**: "Cuidado con el diseño del proposal, debe matchear correctamente con los updates futuros si fuera necesario." → the proposal explicitly enumerates extensibility hooks, and iterative-plan-review (3 rounds) is queued specifically to stress-test those hooks.

## Sprint Contract embed

### Qué construyo

1. Floating panel React component con drag/resize/opacity, persistente en SQLite.
2. Multi-tab system con `+`, close-per-tab, naming dinámico por posición.
3. CodeMirror 6 editor con autosave debounced atómico.
4. Backend Tauri commands para CRUD de tabs sobre filesystem.
5. Soft-delete a `.trash/` + purga automática 30 días en startup.
6. Watcher sobre scratchpadPath para refresh de cambios externos.
7. Setting `scratchpadPath` en SettingsPanel con default `~/.config/cluihud/scratchpad/`.
8. Shortcut Ctrl+L (toggle) + Esc (close-when-focused).

### Cómo verifico

```bash
cd src-tauri && cargo clippy -- -D warnings
cd src-tauri && cargo test
cd src-tauri && cargo fmt --check
npx tsc --noEmit
pnpm dev   # smoke manual
```

### Criterio de done

Ver `proposal.md § Sprint Contract`. Auditable post-implementation.

### Estimated scope

- files_estimate: 12
- risk_tier: low
- tags: [feature]
- visibility: private
- spec_target: scratchpad (new)

## Dependencies / blockers

**External libs already in stack**:
- CodeMirror 6 (frontend editor) — usado en `src/components/editor/`. Reutilizar la misma versión.
- `notify` + `notify-debouncer-full` (backend watcher) — ya usado en `src-tauri/src/claude/transcript.rs` para watch del transcript.
- `rusqlite` (backend DB) — ya usado en `src-tauri/src/db.rs`. Migrations pattern existente.
- `dirs` crate (resolver de `~/.config/`) — verificar si está agregado; si no, dep nueva.

**No new external deps esperadas** salvo `dirs` si falta. Si surge una dep nueva durante implementación, eleva a user.

**No blockers**: cero coupling con código en flight (ship-flow v3, conflict resolution UI, etc.).

## Risk tier: low

- Sin schema cross-cutting: nueva tabla aislada, sin foreign keys a tablas existentes.
- Sin auth, sin secrets, sin billing.
- Sin breaking changes a APIs públicas (Tauri commands nuevos, no modificados).
- Reversible: si la feature se desactiva, basta con remover el componente y el shortcut. No deja estado huérfano más allá de archivos en `~/.config/cluihud/scratchpad/` (que el user puede borrar manualmente).

**Soft risks** (no escalan a critical):
- Floating panel drag/resize en WebKitGTK Linux: comportamiento de `position: fixed` + transforms está validado por el CommandPalette pattern. No hay incógnita.
- Watcher + autosave: race condition entre escritura del editor y notify event. Mitigado por: (a) escribir vía `tmp + rename`, (b) ignorar notify events que coincidan con un write reciente del propio editor (debounce + own-write tracking).

## Gating decision

| Phase | Decision | Rationale |
|-------|----------|-----------|
| Discovery | Skip | Cubierto en conversación previa con el user |
| Iterative plan review | **3 rounds, Claude evaluator** | User pidió cuidado en extensibility. Iprev valida que los hooks futuros no estén sobre-rotulados como "extensible" sin serlo de verdad |
| Reviewer post-build | Single-sequential, code-quality default | Risk tier low, sin auth/security/migration triggers |
| 6-phase gates | 1-3 only (compile, test, lint) | No schema cross-cutting, no deps modifications |
| Reviewer escalation | Auto si `files_touched > 1.5 × 12 = 18` | Detecta scope creep |

## Per-phase model (R0027 + R0043)

| Phase | Model | Where |
|-------|-------|-------|
| Architect (this brief, proposal, iprev rounds) | claude-opus-4-7 | this conversation |
| Builder (implementation steps in Mode B) | claude-sonnet-4-6 | Task spawn per step |
| Exploration (file discovery during Mode B) | claude-haiku-4-5 | Task spawn for grep/scan |

## Goal ancestry injection

Each Builder Task spawn in Mode B will include:
> "Project mission: cluihud is a desktop app wrapper for Claude Code CLI. The scratchpad does not reimplement Claude functionality nor replace Obsidian. It is a global, cross-project, cross-session quick-notes buffer. Decisions must preserve no-coupling with session/workspace state."

## Lazy skill check (Architect §A6)

Skills relevant to this work, evaluated against `~/.claude/skill-library/INDEX.md`:

- **shadcn** (UI components): not directly applicable; cluihud uses `@base-ui/react` primitives + custom patterns documented in DESIGN.md.
- **redesign-existing-projects**: not applicable; this is greenfield UI.
- **modal-vs-page**: applicable in spirit (the user asked exactly this); the proposal already chose floating panel based on §3.9 of DESIGN.md.
- **vercel-react-best-practices**: applicable for the React component design (memoization, atomic state). To be applied during Builder phase.
- **next-best-practices**: not applicable (cluihud is Tauri + Vite, not Next).
- **api-design-principles**: applicable for Tauri commands naming (consistent verb prefixes, clear input/output types).

**Action**: during Builder phase, the spawn for frontend components reads `vercel-react-best-practices`; the spawn for Tauri commands reads `api-design-principles` for consistency.

## Open items deferred to Mode B

- Decisión exacta de schema SQL para `scratchpad_meta` (column types, indexes) — Builder propone, Reviewer valida.
- Layout exacto del header del FloatingPanel chrome (drag handle, opacity slider futuro, close button) — guided by DESIGN.md §3.7 (window controls only for the app window, not for floating tools — usar `Button variant="ghost" size="icon-sm"`).
- Estrategia de own-write tracking en watcher (write barrier vía timestamp window vs explicit ack channel) — Builder propone con cita a notify docs.
