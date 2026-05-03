## Why

Pi (https://github.com/badlogic/pi-mono, package `@mariozechner/pi-coding-agent`) es el segundo adapter post-foundation. Pi rompe los modelos de CC y OpenCode: no tiene file-config hooks, no tiene plan mode (decisión explícita del autor), no tiene sistema de permisos blocking observable externamente. Sus únicas superficies de observación son:

- **JSONL session log** en `~/.pi/agent/sessions/--<cwd-with-slashes>--/<timestamp>_<uuid>.jsonl` — append-only.
- **JSON-RPC stdio** vía `pi --mode rpc` — bidireccional, pero reemplaza el TUI entero (cluihud sería cliente RPC, no envoltorio de terminal).

Decisión D2 (chat de origen): **JSONL tail** para v1. RPC mode queda para v2 si la observación-only resulta insuficiente. Esto significa que Pi en cluihud es **observable, no interceptable**: vemos lo que pasa pero no podemos bloquear, modificar prompts ni interceptar ask-user.

Pi tiene la **mejor estructura de cost reporting** de los 3 (cost USD breakdown estructurado por mensaje), pero por D6 de la foundation (pricing fuera del adapter) solo se exponen los token counts.

## What Changes

- **Nuevo módulo `src-tauri/src/agents/pi/`** implementando el trait con `Transport::JsonlTail`.
- **JSONL tail watcher**: file watcher con `notify` sobre `~/.pi/agent/sessions/<cwd-encoded>/`. Cuando aparece un archivo `.jsonl` nuevo asociado a la sesión activa, se lee desde el inicio y se sigue (tail-f-style).
- **Capabilities declaradas**: `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE`, `SESSION_RESUME` (Pi soporta resume nativo via session_id).
- **Capabilities NO declaradas**: `PLAN_REVIEW` (Pi no tiene plan mode por diseño), `ASK_USER_BLOCKING` (no hay endpoint observable para interceptar prompts), `ANNOTATIONS_INJECT` (no hay equivalente de UserPromptSubmit), `TASK_LIST` (Pi no expone task list discreto).
- **Spawn vía PTY**: a diferencia de OpenCode, Pi sí se spawnea como TUI en el terminal canvas. La integración cluihud es híbrida: TUI nativo en el canvas + JSONL tail en background para alimentar paneles laterales (activity drawer, status bar tokens).
- **Auto-detección**: scan de `~/.pi/agent/` y `which pi`.
- **BYO credentials**: Pi delega a su propio config; cluihud no almacena ni gestiona keys. Settings panel similar al de OpenCode.
- **No hay setup flow**: Pi no consume hooks file-config.

## Capabilities

### New Capabilities

- `pi-adapter`: implementación del trait `AgentAdapter` para Pi usando `Transport::JsonlTail` + PTY spawn nativo.

### Modified Capabilities

- ninguna en specs previas. Esta change es aditiva.

## Impact

- **Backend**:
    - Nuevo `src-tauri/src/agents/pi/{mod.rs, jsonl_tail.rs, transcript.rs, session_resolver.rs}`.
    - Sin nuevas dependencias externas (reusa `notify` que ya está en uso).
    - El adapter spawnea `pi` (o `pi --resume <session_id>`) como CC: en el PTY, con el TUI visible al usuario.
    - Paralelo al PTY: el adapter inicia un file watcher sobre el directorio JSONL específico de la sesión y emite eventos al `EventSink` cuando aparecen nuevas líneas.
- **Frontend**:
    - Sin cambios al render del workspace — Pi usa el terminal canvas existente (PTY-backed).
    - Capability gating ya cubierto por foundation: PlanPanel, AskUserModal y TaskPanel quedan ocultos para sesiones Pi.
    - Activity drawer se alimenta del JSONL tail (events `tool_use`, `tool_result`).
    - Status bar muestra tokens (input/output/cache) cuando llegan eventos cost.
- **Database**: ninguna columna nueva.
- **Tests**: integration test con un fixture JSONL que simule eventos Pi y verifique parser + emit a sink.
- **Doc**: `docs/agents/pi.md` con capabilities limitadas, instructions, expectativas (TUI nativo + paneles read-only).
- **Dependencias del usuario**: `npm install -g @mariozechner/pi-coding-agent` y configurar credenciales según docs de Pi.
