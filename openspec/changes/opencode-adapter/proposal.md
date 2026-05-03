## Why

OpenCode (sst/opencode) es uno de los CLI agents target post-foundation. Su modelo de integración rompe el patrón PTY+hooks del que parten CC y Codex: los hooks son **plugins JS in-process** (no comandos externos disparados por config), y la observación externa más limpia es **HTTP+SSE** vía `opencode serve` en `:4096`. El adapter de OpenCode es el primero post-foundation porque expone el espacio de problema más rico (transport HTTP, REST permission endpoint, capability subset distinto de CC) y valida si el `AgentAdapter` trait está bien diseñado.

**Tradeoff aceptado por decisión D1**: para OpenCode usamos `opencode serve` (HTTP+SSE) en vez de bundlear un plugin JS. Consecuencia: las sesiones OpenCode **no muestran el TUI nativo de OpenCode dentro del terminal canvas** — se renderizan como chat-style desde el SSE bus. Esta es una desviación visual respecto a CC (que sí muestra el TUI de Claude en el terminal). El motivo: HTTP+SSE es transparente, debuggable, y evita shippear código JS en el binary de cluihud. Si el chat-rendering resulta insuficiente, se puede revisitar a un modo plugin en una change futura.

## What Changes

- **Nuevo módulo `src-tauri/src/agents/opencode/`** implementando el trait `AgentAdapter` definido en `agent-adapter-foundation`.
- **Transport `HttpSse`**: cluihud spawnea `opencode serve` en background (puerto autoasignado para evitar colisiones), se suscribe al SSE en `/event`, y responde permissions vía `POST /session/:id/permissions/:permissionID`.
- **Capabilities declaradas**: `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `TASK_LIST` (si OpenCode lo expone — research pendiente confirmar), `RAW_COST_PER_MESSAGE` (best-effort, fields unclear from docs).
- **Capabilities NO declaradas**: `PLAN_REVIEW` (OpenCode tiene `plan` agent toggle, pero **no expone un evento discreto de "plan ready for review"** — el modo se cambia con Tab pero la salida del agente plan no se diferencia estructuralmente del modo build hasta donde la doc explica). `ANNOTATIONS_INJECT` (OpenCode no tiene equivalente de UserPromptSubmit hook que inyecte preámbulo). Estas dos quedan pendientes de research más profundo en el spike de implementación.
- **Chat rendering panel**: nuevo `components/chat/OpenCodeChat.tsx` que renderiza el stream de mensajes desde el SSE bus en lugar del terminal canvas. El terminal canvas queda **vacío o oculto** para sesiones OpenCode. Sin TUI visible.
- **Auto-detección**: scan de `~/.config/opencode/` y `~/.local/share/opencode/storage/`. Si existe alguno, OpenCode aparece en `availableAgentsAtom`.
- **BYO credentials UI**: settings panel para OpenCode incluye un campo de configuración de provider + API key (no se gestionan en el filesystem de cluihud — se delega a `opencode auth login` o similar; cluihud solo abre el flow). Detalle en `design.md`.
- **No hay setup flow tipo `setup --agent opencode`**: OpenCode no consume hooks file-config. La "configuración" se reduce a tener `opencode` instalado y haber corrido su login una vez.

## Capabilities

### New Capabilities

- `opencode-adapter`: implementación del trait `AgentAdapter` para OpenCode usando `opencode serve` + HTTP+SSE + REST permissions.

### Modified Capabilities

- ninguna en specs previas. Esta change es aditiva sobre `agent-adapter-foundation`.

## Impact

- **Backend**:
    - Nuevo `src-tauri/src/agents/opencode/{mod.rs, sse_client.rs, permission_client.rs, transcript.rs, server_supervisor.rs}`.
    - Dependencia nueva: `reqwest` (HTTP) + `eventsource-stream` (SSE parsing) + `tokio-process` (ya transitivo, supervisor del proceso `opencode serve`).
    - El `opencode serve` corre como child process supervised por el adapter; ciclo de vida atado a la sesión cluihud (start at session create, kill at session destroy).
    - Cada sesión OpenCode tiene su propio `opencode serve` instance (puerto autoasignado) — no compartimos un server entre sesiones para evitar cross-session contamination.
- **Frontend**:
    - Nuevo `components/chat/OpenCodeChat.tsx` con renderer message-by-message, ToolUse cards, permission prompts inline.
    - `components/layout/Workspace.tsx` enruta el render del área central por `agent_id`: terminal canvas para CC/Pi/Codex (PTY-backed), `OpenCodeChat` para OpenCode (chat-backed).
    - `components/settings/AgentsSettings.tsx`: panel para configurar provider/API key de OpenCode (instructions point a `opencode auth login` from a terminal; cluihud no almacena credenciales).
- **Database**: ninguna columna nueva sobre lo añadido en `agent-adapter-foundation`.
- **Tests**: integration test que levante `opencode serve` mock (servidor HTTP local con SSE simulado) y verifique que el adapter consume eventos correctamente.
- **Doc**: `docs/agents/opencode.md` con instrucciones de instalación y caveats del rendering chat-style.
- **Dependencias del usuario**: tener `opencode` instalado (Linux: `.deb`/`.rpm`/AppImage/AUR) y haber corrido `opencode auth login` una vez con su provider preferido.
