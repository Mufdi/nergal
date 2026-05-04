## Context

OpenCode (https://github.com/sst/opencode) es un CLI agent open-source con dos superficies relevantes para integración externa:

1. **TUI nativo** (`opencode`): interactivo, paint propio, parecido al de Claude Code. Plugin system in-process en JS — un plugin puede registrar handlers para `tool.execute.before`, `tool.execute.after`, `permission.asked`, `permission.replied`, `session.idle`, etc.
2. **Server mode** (`opencode serve`): HTTP server en `:4096` (puerto configurable) que expone OpenAPI 3.1, SSE streams en `/event` y `/global/event`, y endpoints REST para responder permissions (`POST /session/:id/permissions/:permissionID`).

Cualquier integración externa (no-OpenCode-plugin) tiene dos caminos:

- **Plugin route**: shippeamos un `cluihud-bridge.js` que copiamos a `~/.config/opencode/plugins/`; el plugin reenvía eventos al socket Unix de cluihud. Pro: TUI nativo preservado. Con: shippeamos código JS embebido en el binary de cluihud (bundle weight + maintenance).
- **Serve route**: cluihud spawnea `opencode serve` en background y se suscribe a SSE. Pro: protocolo estándar HTTP, debuggable con cualquier cliente HTTP, sin código JS en cluihud. Con: pierde el TUI nativo — la sesión se renderiza chat-style desde el stream SSE.

Decisión D1 (en chat de origen): **serve route**. El motivo principal fue mantener el adapter HTTP-based, transparente y sin acoplamiento JS. La consecuencia visual (no TUI) se acepta como tradeoff documentado.

## Goals / Non-Goals

**Goals:**
- Adapter funcional para OpenCode usando `opencode serve` + SSE.
- Chat rendering en el área central del workspace cuando la sesión activa es OpenCode (en lugar del terminal canvas).
- Permission interception via REST endpoint, integrado al `AskUserModal` existente del frontend (capability gating cuadra).
- Auto-detección filesystem-based.
- BYO credentials — cluihud no almacena ni gestiona keys; delega a `opencode auth login`.

**Non-Goals:**
- Plan mode (no soportado por OpenCode de forma observable; se oculta el panel via capability gating, decisión D3).
- Plugin bundle (rechazado en D1).
- Compartir un único `opencode serve` entre sesiones (cada sesión = su instance).
- Reverse-engineering del SQLite storage de OpenCode (lo evitamos; consumimos solo el SSE bus).
- Soporte de annotations injection equivalente a CC's UserPromptSubmit (OpenCode no expone un hook análogo via SSE; capability gating lo oculta).

## Decisions

### Decision 1: Cada sesión cluihud = su propio `opencode serve` instance

**Decisión**: El adapter spawnea un `opencode serve` por sesión, en un puerto local autoasignado. La supervisión vive en `agents/opencode/server_supervisor.rs`.

**Por qué**: Compartir un server entre sesiones implicaría multiplexing por session-id en el cliente cluihud y arriesga eventos cross-session. Un server por sesión simplifica el modelo (1:1 cluihud session ↔ opencode session) y evita contaminación. El costo (un proceso `opencode serve` por sesión activa) es manejable — son procesos lightweight.

**Alternativa rechazada**: server compartido + filtrado por `session_id` en el SSE consumer. Más complejo, riesgo más alto, beneficio mínimo.

### Decision 2: Puerto autoasignado, descubierto post-spawn

**Decisión**: El supervisor pasa `--port 0` (o equivalente que pida al SO un puerto libre) a `opencode serve` y lee stdout para extraer el puerto real elegido. Si OpenCode no soporta `--port 0`, fallback a un range `49152-65535` con retry on EADDRINUSE.

**Por qué**: Hardcodear `:4096` impide múltiples sesiones simultáneas. Un puerto efímero por server permite escala arbitraria.

### Decision 3: SSE consumer en background task, eventos al EventSink central

**Decisión**: `agents/opencode/sse_client.rs` mantiene una conexión SSE long-lived a `http://127.0.0.1:<port>/event`, parsea cada evento JSON, y lo traduce a la variant correspondiente de `TranscriptEvent` o `BackendEvent` (genéricos del trait). El `EventSink` recibe eventos normalizados — el frontend no sabe ni le importa que vinieron de SSE.

**Por qué**: Mantener el adapter como detail. La capa superior (event dispatcher en `hooks/server.rs`) no tiene que aprender HTTP — solo recibe eventos vía el sink.

### Decision 4: Permission responder es REST POST, no FIFO

**Decisión**: `submit_ask_answer(session_id, answers)` traduce a `POST /session/:opencode_session_id/permissions/:pending_permission_id` con el body apropiado. El adapter mantiene un mapping `(cluihud_session_id) -> (opencode_session_id, pending_permission_ids)` para resolver el path correcto.

**Por qué**: Simétrico a CC's FIFO write, pero usando el transport propio de OpenCode. El `AskUserModal` del frontend no cambia — el adapter abstrae el transport.

### Decision 5: Chat rendering desde el SSE bus

**Decisión**: Nuevo `components/chat/OpenCodeChat.tsx` consume `agentEventStreamAtom` (un atom alimentado por el sink) y renderiza messages, tool uses (como cards expandibles), tool results, y permission prompts inline. El terminal canvas se hide via render-routing en `Workspace.tsx`.

**Por qué**: SSE entrega messages estructurados (no streams de bytes ANSI), por lo que renderizarlos como terminal sería falsificar. Chat es la abstracción honesta.

### Decision 6: BYO credentials — cluihud no gestiona

**Decisión**: La settings panel para OpenCode contiene **instrucciones**, no inputs de API key. El user corre `opencode auth login <provider>` en su terminal; OpenCode persiste credenciales donde quiera; cluihud solo verifica que `opencode serve` arranca exitosamente.

**Por qué**: cluihud no quiere ser security boundary para credenciales de terceros. Delegar a `opencode auth login` mantiene a cluihud fuera del path de tokens y simplifica auditoría.

### Decision 7: Capabilities declaradas conservadoras

**Decisión**: Declaramos `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE` como **confirmed**. `TASK_LIST` y `SESSION_RESUME` quedan **pending verification** durante el spike — si el SSE bus de OpenCode emite eventos task-list-shaped y session-resume-friendly, los habilitamos; si no, quedan off.

**Por qué**: Mejor declarar menos y sumar en una change posterior que declarar de más y romper UI gating. Capability false silencia el panel correspondiente — fallar abierto en este punto es feo.

### Decision 8: `opencode serve` lifecycle atado a la sesión cluihud

**Decisión**: `start_event_pump(session_id)` arranca `opencode serve` (si no está ya corriendo para esa sesión) + abre la conexión SSE. `stop_event_pump(session_id)` (o session destroy) mata el proceso `opencode serve` con SIGTERM (luego SIGKILL después de 5s).

**Por qué**: Un `opencode serve` huérfano consume puerto + memoria. Lifecycle atado al ciclo de la sesión cluihud previene leaks.

## Trait sketch (specifics)

```rust
// src-tauri/src/agents/opencode/mod.rs

pub struct OpenCodeAdapter {
    binary_path: PathBuf,
    config_path: PathBuf,
    server_supervisor: Arc<ServerSupervisor>,
    sse_clients: Arc<DashMap<SessionId, SseClient>>,
}

#[async_trait::async_trait]
impl AgentAdapter for OpenCodeAdapter {
    fn id(&self) -> AgentId { AgentId::opencode() }
    fn display_name(&self) -> &str { "OpenCode" }

    fn capabilities(&self) -> &AgentCapabilities {
        &AgentCapabilities {
            flags: AgentCapability::ASK_USER_BLOCKING
                 | AgentCapability::TOOL_CALL_EVENTS
                 | AgentCapability::STRUCTURED_TRANSCRIPT
                 | AgentCapability::RAW_COST_PER_MESSAGE,
            // TASK_LIST and SESSION_RESUME pending spike verification.
            supported_models: vec![],  // populated dynamically via opencode list-models if available
        }
    }

    fn transport(&self) -> Transport {
        Transport::HttpSse {
            base_url: Url::parse("http://127.0.0.1:0").unwrap(),  // port resolved at spawn time
            sse_path: "/event",
            permission_endpoint: "/session/:id/permissions/:pid",
        }
    }

    async fn detect(&self) -> DetectionResult { /* check ~/.config/opencode/, ~/.local/share/opencode/, which opencode */ }

    fn spawn(&self, ctx: &SpawnContext) -> anyhow::Result<SpawnSpec> {
        // For OpenCode: spawn doesn't directly start a TUI binary — the supervisor will start `opencode serve`.
        // SpawnSpec returned here is a noop or a wait-loop placeholder; the real work happens in start_event_pump.
        // The PTY layer for OpenCode sessions is dormant (no TUI rendered).
        unimplemented!("OpenCode uses HttpSse transport, not PTY spawn")
    }

    async fn start_event_pump(&self, session_id: &str, sink: EventSink) -> anyhow::Result<()> {
        let port = self.server_supervisor.start(session_id).await?;
        let sse_client = SseClient::connect(format!("http://127.0.0.1:{port}/event")).await?;
        sse_client.run(sink, session_id);  // background task
        self.sse_clients.insert(session_id.into(), sse_client);
        Ok(())
    }

    async fn submit_ask_answer(&self, session_id: &str, answers: serde_json::Value) -> anyhow::Result<()> {
        let pending = self.pending_permission_for(session_id)?;
        let url = format!("http://127.0.0.1:{}/session/{}/permissions/{}", pending.port, pending.session_id, pending.permission_id);
        reqwest::Client::new().post(url).json(&answers).send().await?;
        Ok(())
    }

    async fn submit_plan_decision(&self, _session_id: &str, _decision: PlanDecision) -> anyhow::Result<()> {
        Err(AdapterError::NotSupported(AgentCapability::PLAN_REVIEW))
    }
}
```

## SSE event mapping (best-effort, to verify in spike)

| OpenCode event           | TranscriptEvent variant                                     |
|--------------------------|-------------------------------------------------------------|
| `message.part.updated`   | `Message { role, content, model }`                          |
| `tool.execute.before`    | `ToolUse { name, input }`                                   |
| `tool.execute.after`     | `ToolResult { tool_use_id, output }`                        |
| `permission.asked`       | Backend event: emit `ask:user`                              |
| `permission.replied`     | Backend event: clear pending ask                            |
| `session.idle`           | Backend event: emit `session:idle` (analog of CC Stop)      |
| `session.compacted`      | Skip or surface in activity drawer                          |
| `file.edited`            | Backend event: emit `file:changed`                          |
| `session.error`          | Backend event: emit error toast                             |

Cost field shape (`message.usage` o similar): **unclear from docs**. Spike confirms y si el shape es publicable, se mapea a `RawCost`.

## Frontend chat panel sketch

```typescript
// src/components/chat/OpenCodeChat.tsx
export function OpenCodeChat({ sessionId }: Props) {
  const events = useAtomValue(agentEventStreamAtomFamily(sessionId));
  return (
    <div className="flex flex-col gap-2 p-4 overflow-y-auto">
      {events.map((ev) => match(ev)
        .with({ kind: "message" }, (m) => <MessageBubble role={m.role} content={m.content} />)
        .with({ kind: "tool_use" }, (t) => <ToolUseCard name={t.name} input={t.input} />)
        .with({ kind: "tool_result" }, (r) => <ToolResultCard output={r.output} />)
        .with({ kind: "permission_asked" }, (p) => <PermissionPromptInline prompt={p} />)
        .otherwise(() => null)
      )}
    </div>
  );
}
```

## Risks

- **R1**: SSE event schema de OpenCode no está completamente documentada. Mitigación: spike de implementación incluye un task de "log raw SSE events from a real OpenCode session" para mapear shapes empíricamente antes de codificar el parser.
- **R2**: `opencode serve` puede no soportar `--port 0` (puerto autoasignado). Mitigación: fallback a range scan con retry on EADDRINUSE.
- **R3**: BYO credentials UX puede confundir a usuarios que esperan que cluihud almacene la key. Mitigación: settings panel claro con instrucciones step-by-step y link a docs de OpenCode.
- **R4**: Lifecycle del `opencode serve` huérfano si cluihud crashea. Mitigación: PID file + cleanup at startup; matar procesos `opencode serve` huérfanos cuyo cluihud parent ya no existe.
- **R5**: La ausencia de TUI puede sentirse extraña — el usuario espera un terminal y encuentra un chat. Mitigación: documentar claramente en `docs/agents/opencode.md` y en el agent picker tooltip que OpenCode usa chat rendering.
- **R6**: Cost reporting puede no ser estructurado en el SSE bus. Mitigación: si el spike confirma que el shape no permite extracción, declaramos `RAW_COST_PER_MESSAGE` off y la status bar oculta el segmento.
