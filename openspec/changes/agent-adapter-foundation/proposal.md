## Why

Cluihud está acoplado a Claude Code (CC) en ~13 subsistemas: hook CLI subcommands, setup flow, hook socket server, plan-review FIFO, AskUser FIFO, transcript parser, cost extractor, plan watcher, task parser, PTY spawn, frontend stores, hardcoded paths (`~/.claude/`, `.claude/plans/`), y pricing del modelo. Para soportar múltiples agentes CLI (OpenCode, Pi Code, Codex) sin reescribir esos 13 puntos tres veces, hace falta una capa de abstracción `AgentAdapter` que normalice spawn, suscripción a eventos, parsing de transcript, captura de cost, y declaración de capabilities. Sin esta foundation, cada adapter siguiente arrastra el costo total de la integración.

CC sigue siendo first-class — esta change extrae el comportamiento actual hacia un primer adapter sin alterar UX. Los cambios visibles para el usuario llegan en las changes posteriores (`opencode-adapter`, `pi-adapter`, `codex-adapter`) que dependen de esta foundation.

## What Changes

- **Nuevo módulo `src-tauri/src/agents/`** con trait `AgentAdapter` y enums `AgentCapability` + `Transport`.
- **`AgentRegistry`** que enumera adapters disponibles y autodetecta instalaciones via filesystem scan (`~/.claude/`, `~/.codex/`, `~/.config/opencode/`, `~/.pi/agent/`).
- **CC extraído como primer adapter** en `src-tauri/src/agents/claude_code/`. Movimientos: `claude/transcript.rs`, `claude/cost.rs`, `claude/plan.rs` → `agents/claude_code/`. `claude/openspec.rs` queda fuera del adapter (es convención del proyecto, no del agente).
- **Refactor de `hooks/`, `setup.rs`, `commands.rs`, `pty.rs`** para enrutar a través del trait en vez de llamar directo a la lógica CC.
- **Capability-gated UI**: PlanPanel, TaskPanel, AskUserModal y CostStatusBar renderizan solo cuando el adapter activo declara la capability correspondiente.
- **Hardcoded Sonnet 4 pricing se mueve a privado dentro del CC adapter** (`agents/claude_code/cost.rs::legacy_usd_for_sonnet4`). El trait core no expone USD; la extracción devuelve solo `RawCost` (token counts). Para CC, el status bar sigue mostrando USD via Tauri command `get_session_cost_usd` que internamente llama el helper privado — **sin regresión UX para usuarios CC**. Otros adapters retornan `None` hasta que aterrice el módulo `pricing` general (change futura).
- **Frontend**: nuevo `agentAdapterAtom` + `agentCapabilitiesAtom` consumido por componentes para gating.
- **DB schema**: nueva columna `agent_id TEXT NOT NULL DEFAULT 'claude-code'` en tabla `sessions`. Backward-compatible para datos existentes.
- **Config**: nuevo campo `agent_overrides: HashMap<ProjectPath, AgentId>` para forzar adapter por proyecto.
- **Sin BREAKING para usuarios CC**: zero regression. Sesiones existentes siguen funcionando idénticas.
- **BREAKING para devs**: paths de archivos cambian (`src-tauri/src/claude/` → `src-tauri/src/agents/claude_code/`).

## Capabilities

### New Capabilities

- `agent-adapter`: contract genérico para integrar CLI agents en cluihud (spawn, eventos, transcript, cost, capabilities, autodetección, UI gating).
- `cc-adapter`: implementación CC del contract `agent-adapter`. Preserva todo el comportamiento actual.

### Modified Capabilities

- ninguna spec previa cambia comportamiento user-facing. Internamente todos los componentes que llaman a `claude/*` ahora pasan por el trait, pero la observación user-facing es idéntica.

## Impact

- **Backend**:
    - Nuevo módulo `src-tauri/src/agents/` con `mod.rs` (trait + enums + registry), `claude_code/mod.rs` + submódulos.
    - `src-tauri/src/claude/` → renombrado a `src-tauri/src/agents/claude_code/`. `openspec.rs` se mueve a `src-tauri/src/openspec.rs` (top-level, no parte del adapter).
    - `hooks/cli.rs`: cada subcomando recibe `agent_id` y delega al adapter correspondiente.
    - `hooks/server.rs`: el dispatcher usa el adapter del `agent_id` resuelto desde `cluihud_session_id` para parsear el payload del hook.
    - `setup.rs`: se generaliza a `setup --agent <id>`. La versión actual queda como `setup --agent claude-code`, default cuando no se pasa flag.
    - `pty.rs`: spawn delega a `adapter.spawn_command()` que devuelve `(binary, args, env)`.
- **Frontend**:
    - Nuevo `src/stores/agent.ts` con `agentAdapterAtom`, `agentCapabilitiesAtom`, `availableAgentsAtom`.
    - `stores/hooks.ts`, `stores/tasks.ts`, `stores/plan.ts`, `stores/askUser.ts` consumen capability gating antes de actualizar atoms.
    - `components/plan/PlanPanel.tsx`, `components/tasks/TaskPanel.tsx`, `components/session/AskUserModal.tsx`, `components/layout/StatusBar.tsx` (cost): ocultan render si capability falta.
    - `components/sidebar/`: indicador visual del agente activo por sesión (icono o badge).
- **DB**: migration adding `agent_id` column, default `'claude-code'` para rows existentes.
- **Config**: `~/.config/cluihud/config.json` agrega campo opcional `agent_overrides`.
- **Tests**: integration tests que validen zero-regression CC: PlanPanel se abre, TaskPanel actualiza, cost se calcula, AskUserModal intercepta, plan FIFO funciona end-to-end.
- **Sin cambios** en: terminal canvas, ship-flow, conflictos, scratchpad, file browser, command palette, layout, git ops (worktree/stash genéricos).
