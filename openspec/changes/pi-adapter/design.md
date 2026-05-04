## Context

Pi (Mario Zechner's pi-coding-agent, https://github.com/badlogic/pi-mono) es un agente CLI minimalista por diseño. Citando al autor: "ships with powerful defaults but skips features like sub agents and plan mode". Las únicas superficies de observación externa son:

1. **JSONL session log** — append-only, persistido en `~/.pi/agent/sessions/--<cwd-with-slashes-as-dashes>--/<timestamp>_<uuid>.jsonl`. El path encoding (slashes → dashes) significa que cluihud puede derivar el dir de sesiones para el `cwd` actual sin scrapear paths.
2. **JSON-RPC stdio** vía `pi --mode rpc` — bidireccional, LF-delimited JSONL en stdin/stdout. Reemplaza el TUI nativo: si cluihud usa RPC, deja de ser wrapper de terminal y pasa a ser cliente RPC con renderer propio.

Por D2 (chat de origen) tomamos **JSONL tail** para v1. La consecuencia es que cluihud para Pi es **observable, no interceptable**: ve eventos pero no puede bloquear, inyectar, ni responder. Esto es coherente con la naturaleza de Pi (sin plan mode, sin sistema de permisos analog a CC's PreToolUse).

JSONL tail se complementa con TUI spawn vía PTY (como CC) — el usuario ve el TUI de Pi nativo en el terminal canvas + cluihud paralelamente lee el JSONL para alimentar paneles laterales. Esta es la diferencia más clara con OpenCode (donde no hay TUI render).

## Goals / Non-Goals

**Goals:**
- Adapter funcional para Pi usando file watcher sobre el dir de sesiones de Pi.
- TUI nativo de Pi en el terminal canvas (PTY-backed, igual que CC).
- Activity drawer + status bar (tokens) alimentados desde el JSONL tail.
- Auto-detección filesystem-based.
- Session resume vía `pi --resume <session_id>`.

**Non-Goals:**
- Plan mode (Pi no lo tiene; capability gating oculta el panel).
- Ask-user blocking (Pi no expone interceptación; capability gating oculta el modal).
- Annotation injection (no hay hook análogo).
- RPC mode (queda para una v2 si la observación-only resulta insuficiente).
- Re-implementar la rica estructura de cost USD de Pi — devolvemos token counts y delegamos pricing a un módulo posterior.
- Task list panel (Pi no expone tasks discretos).

## Decisions

### Decision 1: PTY spawn como CC, no como OpenCode

**Decisión**: Pi se spawnea con el TUI nativo en el terminal canvas. `spawn(ctx)` retorna `SpawnSpec { binary: "pi", args: [...], env: { CLUIHUD_SESSION_ID: ... } }`. El usuario ve el TUI de Pi como vería el TUI de Claude.

**Por qué**: Pi tiene un TUI capaz; reemplazarlo con chat-rendering sería empobrecer la UX sin razón técnica (a diferencia de OpenCode donde el server mode no permite TUI). El paralelo JSONL tail se hace por separado, no replazando.

### Decision 2: JSONL tail watcher por sesión

**Decisión**: `start_event_pump(session_id, sink)` deriva el dir de sesiones de Pi a partir del `cwd` (con encoding slashes→dashes), busca el archivo `.jsonl` más reciente cuyo nombre matchee el patrón `<timestamp>_<uuid>.jsonl`, y arranca un tail watcher con `notify` sobre ese archivo.

**Por qué**: Pi crea un nuevo `.jsonl` por sesión. El adapter necesita resolver "qué archivo corresponde a esta sesión cluihud". Patrón: spawn Pi con un session_id conocido, esperar a que aparezca el `.jsonl`, abrirlo. Race conditions mitigadas con un poll inicial corto (100ms × 20).

### Decision 3: Tail-f semantics, no full re-read

**Decisión**: El watcher abre el archivo, lo lee completo una vez (catch-up de eventos pre-watcher), y luego sigue desde el offset alcanzado. En cada `notify` event de modify, se lee desde el último offset hacia EOF.

**Por qué**: Estándar tail-f. Robusto a reordering, permite reanudar tras restarts.

### Decision 4: Esquema JSONL parsing por entry type

**Decisión**: Cada línea es un objeto JSON con campo `type` discriminante. Variantes documentadas en docs de Pi:

- `session` (header con `version`, `id`, `timestamp`, `cwd`)
- `agent` (mensaje de role `user | assistant | toolResult | bashExecution | custom`)
- `tool_call` (con `id`, `name`, `arguments`)
- `tool_result` (con `toolCallId`, `toolName`, output)
- `model_change`, `thinking_level_change`, `compaction`, `branch_summary`, `label`, `session_info`

Mapeo a `TranscriptEvent`:

| Pi entry                 | TranscriptEvent variant                      |
|--------------------------|----------------------------------------------|
| `session`                | (header — store metadata, no event emitted)  |
| `agent` (assistant)      | `Message { role: "assistant", content, model }` |
| `agent` (user)           | `Message { role: "user", content, ... }`     |
| `tool_call`              | `ToolUse { name, input: arguments }`         |
| `tool_result`            | `ToolResult { tool_use_id, output }`         |
| `agent.usage` (assistant)| `Cost(RawCost { input, output, cache_read, cache_write, model_id })` |
| `model_change`           | Surface in activity drawer                   |
| `compaction`             | Surface in activity drawer                   |
| Otros                    | `Other(value)` o skip                        |

**Por qué**: Schema rico de Pi permite mapeo directo. Solo emitimos eventos que el frontend va a renderizar.

### Decision 5: Cost capture rico, USD descartado

**Decisión**: Pi emite `usage: { input, output, cacheRead, cacheWrite, totalTokens, cost: { input, output, cacheRead, cacheWrite, total } }`. El adapter extrae los **token counts** y los empaqueta en `RawCost`. **El campo `cost` (USD) se descarta** — por D6 de la foundation, pricing es responsabilidad de un módulo posterior.

**Por qué**: Aunque Pi entrega USD ya calculado, mantener consistencia con los otros adapters (todos emiten raw counts) simplifica el módulo `pricing` futuro. Si `pricing` decide confiar en cost reporters per-adapter, podemos revisitar.

### Decision 6: Session resume vía `pi --resume <session_id>`

**Decisión**: Cuando el usuario reanuda una sesión Pi en cluihud, el adapter hace `spawn(ctx)` con `args: ["--resume", ctx.resume_from.unwrap()]`. El `session_id` que cluihud guarda corresponde 1:1 al UUID que Pi usa en el filename del `.jsonl`.

**Por qué**: Pi soporta resume nativo y persiste sessions con UUIDs. La pareja cluihud_session_id ↔ pi_session_uuid se establece al primer spawn (parseando el header del JSONL) y se persiste en la DB.

### Decision 7: Sin setup flow

**Decisión**: No hay `setup_agent('pi')`. Pi no consume hooks file-config. La "configuración" se reduce a:

1. Tener `pi` instalado (`npm install -g @mariozechner/pi-coding-agent`).
2. Configurar credenciales según docs de Pi (`pi config` o variables de entorno).

**Por qué**: Cluihud no tiene dónde escribir nada para Pi. El settings panel es read-only (status, version).

### Decision 8: Capability gating cierra los gaps

**Decisión**: PlanPanel, AskUserModal, TaskPanel se ocultan automáticamente para sesiones Pi gracias al gating instalado en foundation. No agregamos lógica Pi-específica en el frontend.

**Por qué**: La foundation ya hace este trabajo. Si introducimos lógica especial para Pi, estamos rompiendo el principio de "capability gating es la única forma de gates UI".

## Trait sketch (specifics)

```rust
// src-tauri/src/agents/pi/mod.rs

pub struct PiAdapter {
    binary_path: PathBuf,
    state_dir: PathBuf,  // ~/.pi/agent/
}

#[async_trait::async_trait]
impl AgentAdapter for PiAdapter {
    fn id(&self) -> AgentId { AgentId::pi() }
    fn display_name(&self) -> &str { "Pi" }

    fn capabilities(&self) -> &AgentCapabilities {
        &AgentCapabilities {
            flags: AgentCapability::TOOL_CALL_EVENTS
                 | AgentCapability::STRUCTURED_TRANSCRIPT
                 | AgentCapability::RAW_COST_PER_MESSAGE
                 | AgentCapability::SESSION_RESUME,
            supported_models: vec![],  // Pi supports many providers; left empty
        }
    }

    fn transport(&self) -> Transport {
        Transport::JsonlTail {
            sessions_dir: self.state_dir.join("sessions"),
        }
    }

    async fn detect(&self) -> DetectionResult {
        // check ~/.pi/agent/, which pi
    }

    fn spawn(&self, ctx: &SpawnContext) -> anyhow::Result<SpawnSpec> {
        let mut args = vec![];
        if let Some(resume) = ctx.resume_from {
            args.extend(["--resume".into(), resume.into()]);
        }
        Ok(SpawnSpec {
            binary: self.binary_path.clone(),
            args,
            env: hashmap! { "CLUIHUD_SESSION_ID" => ctx.session_id.into() },
        })
    }

    async fn start_event_pump(&self, session_id: &str, sink: EventSink) -> anyhow::Result<()> {
        let cwd = self.session_resolver.cwd_for(session_id)?;
        let jsonl_path = self.session_resolver.wait_for_jsonl(&cwd, session_id, Duration::from_millis(2000)).await?;
        let tail = JsonlTail::open(&jsonl_path)?;
        tail.run(sink, |line| self.parse_transcript_line(line));
        Ok(())
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        let entry: PiEntry = serde_json::from_str(line).ok()?;
        match entry {
            PiEntry::Agent { role: "assistant", content, usage, model, .. } => {
                if let Some(u) = usage {
                    Some(TranscriptEvent::Cost(RawCost {
                        input_tokens: u.input,
                        output_tokens: u.output,
                        cache_read_tokens: u.cache_read,
                        cache_write_tokens: u.cache_write,
                        model_id: Some(model),
                    }))
                } else {
                    Some(TranscriptEvent::Message { role: "assistant".into(), content, model: Some(model) })
                }
            }
            PiEntry::ToolCall { name, arguments, .. } => Some(TranscriptEvent::ToolUse { name, input: arguments }),
            PiEntry::ToolResult { tool_call_id, output, .. } => Some(TranscriptEvent::ToolResult { tool_use_id: tool_call_id, output }),
            _ => None,
        }
    }

    async fn submit_plan_decision(&self, _: &str, _: PlanDecision) -> anyhow::Result<()> {
        Err(AdapterError::NotSupported(AgentCapability::PLAN_REVIEW))
    }

    async fn submit_ask_answer(&self, _: &str, _: serde_json::Value) -> anyhow::Result<()> {
        Err(AdapterError::NotSupported(AgentCapability::ASK_USER_BLOCKING))
    }
}
```

## Session JSONL resolution

Pi encodes paths slashes→dashes. For `cwd = /home/user/projects/foo`, Pi looks at `~/.pi/agent/sessions/--home-user-projects-foo--/`. Filenames are `<unix_timestamp>_<uuid>.jsonl`. The adapter:

1. Computes the encoded path from `ctx.cwd`.
2. Spawns `pi` with `CLUIHUD_SESSION_ID` set.
3. Polls the encoded sessions dir for new files (100ms intervals × 20 attempts).
4. The newest file is the active session; opens it and starts tailing.
5. Reads the first line (`session` header) to extract Pi's UUID; persists `pi_session_uuid` in cluihud's session row for resume.

## Risks

- **R1**: La race entre `pi spawn` y `.jsonl` creation puede tardar más que el window de 2s. Mitigación: configurable timeout, surface error si excede.
- **R2**: Pi puede cambiar el path encoding (slashes→dashes) en futuras versiones. Mitigación: feature-flag el encoder en `session_resolver.rs`; tests con fixtures.
- **R3**: JSONL tail puede perder eventos si Pi escribe muy rápido y `notify` debounce comprime cambios. Mitigación: read full file on every modify event (idempotente vía offset tracking).
- **R4**: Sin ASK_USER_BLOCKING, los prompts interactivos de Pi quedan en el terminal nativo (el usuario los ve y responde con teclado en el TUI). Mitigación: documentar en `docs/agents/pi.md` que Pi conserva su flow nativo de input/output, cluihud solo observa.
- **R5**: Sesiones largas con `.jsonl` grande (10MB+) pueden causar slow startup en re-attach. Mitigación: lazy load — los eventos antiguos no se renderizan en el activity drawer hasta scroll.
