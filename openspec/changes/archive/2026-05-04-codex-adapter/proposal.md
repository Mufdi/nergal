## Why

Codex CLI (OpenAI, https://github.com/openai/codex) es el tercer adapter post-foundation. **Es el más fácil de los tres** contraintuitivamente: el sistema de hooks de Codex (`~/.codex/hooks.json`) está modelado **casi 1:1 sobre el de CC** — mismos event names (`SessionStart`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `UserPromptSubmit`, `Stop`), misma convención JSON-on-stdin, misma semántica allow/deny. La integración es mayoritariamente:

1. Reutilizar la infraestructura `Transport::FileHooks` (introducida por la foundation con CC como primer cliente).
2. Escribir un nuevo `setup_agent('codex')` que produzca `~/.codex/hooks.json` en lugar de `~/.claude/settings.json`.
3. Implementar parser para el rollout JSONL en `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl`.

Lo que NO mapea de CC:

- **Plan mode**: Codex no tiene un primitivo análogo a `ExitPlanMode`. La "approval policy" (`untrusted` / `on-request` / `never`) controla execution gating pero no expone "plan generated, review it". Decisión D3: ocultamos PlanPanel para Codex.
- **Pricing fields per-message**: el rollout schema no está completamente documentado. Igual que con CC, devolvemos token counts si están presentes y delegamos USD al módulo `pricing` futuro.

Codex se incluye en la foundation roadmap pero se implementa **al final** del orden (post-OpenCode y post-Pi) porque su parecido con CC implica el menor riesgo de descubrir gaps en el `AgentAdapter` trait. Si hay refinamientos al trait, salen de OpenCode/Pi primero.

## What Changes

- **Nuevo módulo `src-tauri/src/agents/codex/`** implementando el trait con `Transport::FileHooks` (reuse de la infraestructura de hooks de la foundation).
- **Hooks via `~/.codex/hooks.json`**: setup flow escribe el archivo con matchers análogos a los de CC. Eventos enrutados al mismo Unix socket (`/tmp/cluihud.sock`) con `cluihud_session_id` para resolución del adapter.
- **Capabilities declaradas**: `ASK_USER_BLOCKING` (vía PermissionRequest hook), `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE` (best-effort, fields TBD), `TASK_LIST` (si Codex emite eventos task-shaped — verificar en spike), `SESSION_RESUME`.
- **Capabilities NO declaradas**: `PLAN_REVIEW` (sin equivalente). `ANNOTATIONS_INJECT` (Codex tiene `additionalContext` en hook responses, semánticamente cercano a UserPromptSubmit — declarable post-spike).
- **Spawn vía PTY**: como CC y Pi. Codex se ve nativo en el terminal canvas.
- **Auto-detección**: scan de `~/.codex/` y `which codex`.
- **BYO credentials**: Codex requiere config de provider (OpenAI o Azure OAI). Delegado al usuario via `codex login` o config.toml.
- **Trust-gate awareness**: Codex requiere "trust" explícito por proyecto antes de cargar `.codex/config.toml` local. cluihud surface esto en el agent picker / settings con un botón "Trust this project for Codex" si aplica.

## Capabilities

### New Capabilities

- `codex-adapter`: implementación del trait `AgentAdapter` para Codex CLI, reutilizando la infraestructura `Transport::FileHooks` de la foundation.

### Modified Capabilities

- `agent-adapter`: el spec del trait NO cambia. Pero la implementación de `Transport::FileHooks` en la foundation se valida con un segundo cliente real (Codex), confirmando que el diseño no era CC-only by accident.

## Impact

- **Backend**:
    - Nuevo `src-tauri/src/agents/codex/{mod.rs, setup.rs, transcript.rs}`.
    - Sin nuevas dependencias.
    - El módulo `setup.rs` define los hooks que se escriben a `~/.codex/hooks.json`.
    - El parser de transcript lee `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl`.
- **Frontend**:
    - Sin cambios al render del workspace — Codex usa el terminal canvas existente.
    - Capability gating ya cubierto por foundation: PlanPanel se oculta.
    - AgentsSettings panel agrega una sección Codex con botón "Run setup" + "Trust this project" (si aplica).
- **Database**: usa `agent_internal_session_id` ya añadido por `pi-adapter` (también lo usa Codex para resume).
- **Tests**: integration tests con fixture rollout JSONL + mock de hooks executando `cluihud hook send`.
- **Doc**: `docs/agents/codex.md` con install, capabilities, trust-gate handling.
- **Dependencias del usuario**: `npm install -g @openai/codex` o `brew install --cask codex` o binarios estáticos. Configurar provider via `codex login` o `~/.codex/config.toml`.
