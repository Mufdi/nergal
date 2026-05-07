# ARCHITECT-BRIEF — live-preview-browser

## Project mission (from CLAUDE.md)

cluihud is a desktop app wrapper para Claude Code CLI (Plan Pro) en Linux. Tauri v2 + React 19 hybrid architecture. Useful recommendations are those that improve the experience of using Claude Code: plan editing UX, task visibility, session navigation, hook-driven panels, keyboard shortcuts, workspace/worktree management.

cluihud is NOT a standalone terminal, NOT a Claude Code reimplementation, NOT an agent framework.

## Why this change matters

The frontend dev loop currently requires context-switch to an external browser to verify localhost dev server output. This is the most visible gap when claude is editing UI and the user wants to validate. Embedding a browser inside cluihud — dual-mode (dock persistent + floating popup) — closes the loop without forcing the user to leave the workspace, which directly improves the experience of using Claude Code on frontend work.

## Sprint Contract embed (from proposal.md)

### Qué construyo
1. spike YAML for Phase 0 validation gate
2. stores/browser.ts with session-keyed atoms
3. components/browser/ (BrowserPanel, BrowserToolbar, BrowserFloating)
4. rightPanel.ts modifications (TabType, SINGLETON_TYPES, PANEL_CATEGORY_MAP)
5. layout/RightPanel.tsx, layout/StatusBar.tsx integrations
6. src-tauri/src/browser.rs (webview lifecycle + port scanner)
7. commands.rs + lib.rs registration
8. capabilities/default.json updates
9. DESIGN.md pattern doc

### Cómo verifico
- Backend: `cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- Frontend: `npx tsc --noEmit`
- Manual: 7-step golden path from proposal.md

### Criterio de done
Tab abre, toolbar funciona, floating mode reutiliza FloatingPanel, port scanner detecta dev servers, click en chip abre browser, Cmd+L focus URL, all gates pass, DESIGN.md updated, follow-up changes documented as stubs.

### Estimated scope
- files_estimate: 11
- risk_tier: medium
- tags: [feature, security]
- visibility: private
- spec_target: live-preview-browser

## Dependencies + blockers

- **Blocker**: Phase 0 spike outcome. If FAILED → reconcile fallback before continuing.
- **No conflict** con active changes: context-bridge, diff-annotation-mode, spec-annotation-mode son módulos diferentes.
- **Touch points** con specs existentes:
  - `tab-system` (TabType union extension)
  - `panel-categories` (browser → "tool")
  - `adaptive-layout` (RightPanel mounting)

## Risk gating decision

Triple-prompt gating: **OFF** by default (risk_tier=medium, files_estimate=11 < 5 trigger but tags include `security`).

Trigger to escalate to triple-prompt mid-build:
- Diff to `capabilities/default.json` → security reviewer auto-spawn.
- `files_touched > 1.5 × 11 = 17` → scope creep escalation.

## Per-phase model assignments

- Architect (this brief, /discovery, decisions): claude-opus-4-7 (already done, this turn).
- Builder (Phase 1-5 implementation): claude-sonnet-4-6.
- Reviewer (post-build): claude-opus-4-7 if security-reviewer spawned (capabilities diff); claude-sonnet-4-6 for code-quality default.
- Exploration (file discovery during build, if needed): claude-haiku-4-5.

## Lazy-skill check

Searched ~/.claude/skill-library/INDEX.md against this work scope:
- **Frontend** category may apply: `shadcn` (none of the new components map directly to shadcn primitives — toolbar uses custom controls), `redesign-existing-projects` (NO — this is greenfield additive). No auto-load.
- **API** / Pagos / Auth / Database: no aplica.
- Fallback: standard `tauri-app-dev` agent skill is registered globally; it surfaces automatically for Builder.

## Lineage

- Source ideas: `Inspiración y Referencias - Claude Code GUIs (2026).md` § cmux, Vibeyard, Limux, Glass, Intent, opensessions, Orca (deferred).
- Backlog item: § "Implementaciones Futuras y Correcciones" → "In-app browser dual-mode".
- Follow-up changes (out of scope): `browser-design-mode`, `browser-reload-on-save`.
