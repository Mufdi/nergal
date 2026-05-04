## Context

Codex CLI (https://github.com/openai/codex) presenta una superficie de integración casi gemela a la de Claude Code. Las similitudes confirmadas en research:

- **File-config hooks** en `~/.codex/hooks.json` o inline en `~/.codex/config.toml` bajo `[hooks]`. Mismo patrón de matchers + comandos shell que CC.
- **Event names** prácticamente idénticos: `SessionStart`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `UserPromptSubmit`, `Stop`.
- **JSON-on-stdin convention** con campos `session_id`, `cwd`, `hook_event_name`, `tool_name`, `tool_input`, `tool_response`.
- **Allow/deny semantics** vía exit codes y `permissionDecision` en respuesta.
- **Session rollouts** en `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl` (JSONL append-only).

Las diferencias relevantes:

- **No documented Plan mode equivalent**. Codex tiene `approval_policy` (`untrusted | on-request | never`) que controla si comandos se ejecutan automáticamente, pero no hay un evento "plan generated, review before execution" análogo a `ExitPlanMode` de CC. **Decisión D3: ocultamos PlanPanel para Codex** vía capability gating.
- **Trust-gate por proyecto**. Codex no carga `.codex/config.toml` de un proyecto hasta que el usuario le da `codex trust` (o lo equivalente). Esto añade fricción al primer uso en un proyecto nuevo. Cluihud puede surfacear esto en la UX.
- **Per-message cost field shape** no está completamente documentado. Spike confirma.
- **`PermissionRequest`** existe como hook (cubre nuestro `ASK_USER_BLOCKING`). Lo que NO está claro es si Codex tiene un equivalente discreto a `AskUserQuestion` (pregunta libre del agente al user, no un permission gate).

## Goals / Non-Goals

**Goals:**
- Adapter funcional para Codex reusando `Transport::FileHooks` de la foundation.
- `setup_agent('codex')` escribe `~/.codex/hooks.json` con matchers análogos a CC.
- Parser para rollout JSONL.
- Capability gating oculta PlanPanel automáticamente.
- Auto-detección filesystem.
- Trust-gate visible en UX cuando aplica.

**Non-Goals:**
- Plan mode synthesis para Codex (D3 lo descarta — ocultamos).
- Cross-agent session resumption con CC (cada uno tiene su propio rollout schema).
- Reverse-engineering del trust-gate (delegamos al usuario).
- Soporte de `codex mcp-server` mode (Codex-as-MCP-server) — fuera de scope de este adapter, sería una integración separada en el futuro.
- Hardcoded pricing per modelo OpenAI.

## Decisions

### Decision 1: Reuse `Transport::FileHooks` con configuración Codex-específica

**Decisión**: El `Transport::FileHooks` introducido por foundation lleva un payload `{ settings_path, hook_event_names }`. Para Codex, `settings_path = ~/.codex/hooks.json` y `hook_event_names` es el subset que Codex soporta. La infraestructura de Unix socket + dispatch que funciona para CC sirve idéntica.

**Por qué**: Codex es el caso de prueba más fuerte para validar que `Transport::FileHooks` no era CC-disfrazado-de-genérico. Si el trait está bien diseñado, Codex usa la misma maquinaria sin parches.

### Decision 2: setup escribe hooks.json (no config.toml)

**Decisión**: Codex permite hooks tanto en `~/.codex/hooks.json` como inline en `[hooks]` de `config.toml`. cluihud usa el archivo dedicado `hooks.json` para evitar editar la TOML del usuario (más seguro contra corruption).

**Por qué**: TOML editing es propenso a estilo (comentarios, formato). JSON es más predecible. Si el usuario tiene su propia config Codex en TOML, cluihud no la toca.

### Decision 3: Trust-gate como UX explícito en Settings

**Decisión**: Cuando el usuario crea una sesión Codex en un proyecto nuevo, el adapter detecta si Codex ya confía en el proyecto (heurística: presencia de `<project>/.codex/.trusted` o equivalente que documente Codex). Si no, surface un banner "Codex requires trust for this project — run `codex trust` from a terminal" con instrucciones.

**Por qué**: cluihud no debe correr `codex trust` automáticamente — es decisión de seguridad del usuario. Pero podemos hacer la fricción visible en lugar de invisible.

### Decision 4: Capabilities post-spike

**Decisión**: Capabilities iniciales declaradas: `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `SESSION_RESUME`. Pendientes de spike: `RAW_COST_PER_MESSAGE`, `TASK_LIST`, `ANNOTATIONS_INJECT`. Se añaden si el spike empírico confirma que el rollout JSONL los expone.

**Por qué**: Conservador — declarar de menos no causa visual fantasma; declarar de más sí.

### Decision 5: Plan panel oculto sin synthesis

**Decisión**: NO sintetizamos plan-mode para Codex. PlanPanel se oculta vía capability gating y el usuario ve solo paneles relevantes para Codex.

**Por qué**: Synthesis vía pattern matching de PreToolUse sería frágil y confuso. Codex es honesto sobre no tener plan-review; cluihud refleja esa honestidad.

### Decision 6: Session resume vía rollout filename + Codex's own resume

**Decisión**: Codex genera `rollout-<uuid>.jsonl` en `$CODEX_HOME/sessions/YYYY/MM/DD/`. cluihud persiste el `<uuid>` en `agent_internal_session_id` (columna añadida por `pi-adapter`). En resume, `spawn(ctx)` invoca `codex resume <uuid>` (o el flag equivalente — verificar en spike).

**Por qué**: Mismo modelo que Pi. Reutilizamos la columna y la pattern.

### Decision 7: Reuso de `cluihud hook send/inject-edits/ask-user/plan-review` CLI subcommands

**Decisión**: Los subcomandos del binary `cluihud` se generalizan en foundation para aceptar `--agent <id>`. Para Codex, `setup_agent('codex')` produce hooks que invocan `cluihud hook send pre-tool --agent codex` (etc.). El subcommand handler enruta el payload al `CodexAdapter` para parsing.

**Por qué**: Una sola CLI binary con flag de agente > múltiples binarios `cluihud-cc`, `cluihud-codex`. La maquinaria de Unix socket no cambia.

### Decision 8: PermissionRequest cubre AskUser

**Decisión**: Codex's `PermissionRequest` hook se mapea a `ask:user` event en cluihud. El payload incluye un prompt + opciones cuando corresponda. La respuesta del usuario en AskUserModal se traduce a la respuesta de hook que Codex espera (`permissionDecision: 'allow'|'deny'`, `additionalContext: <reason>`).

**Por qué**: PermissionRequest semánticamente cubre el caso de uso de "agente pide al usuario decisión bloqueante". El AskUserModal del frontend funciona idéntico.

## Trait sketch (specifics)

```rust
// src-tauri/src/agents/codex/mod.rs

pub struct CodexAdapter {
    binary_path: PathBuf,
    config_dir: PathBuf,  // ~/.codex
    sessions_dir: PathBuf, // $CODEX_HOME/sessions/
}

#[async_trait::async_trait]
impl AgentAdapter for CodexAdapter {
    fn id(&self) -> AgentId { AgentId::codex() }
    fn display_name(&self) -> &str { "Codex" }

    fn capabilities(&self) -> &AgentCapabilities {
        &AgentCapabilities {
            flags: AgentCapability::ASK_USER_BLOCKING
                 | AgentCapability::TOOL_CALL_EVENTS
                 | AgentCapability::STRUCTURED_TRANSCRIPT
                 | AgentCapability::SESSION_RESUME,
            // RAW_COST_PER_MESSAGE / TASK_LIST / ANNOTATIONS_INJECT pending spike
            supported_models: vec![],  // populated dynamically if Codex exposes a list
        }
    }

    fn transport(&self) -> Transport {
        Transport::FileHooks {
            settings_path: self.config_dir.join("hooks.json"),
            hook_event_names: vec![
                "SessionStart", "SessionEnd", "PreToolUse", "PostToolUse",
                "PermissionRequest", "UserPromptSubmit", "Stop",
            ],
        }
    }

    async fn detect(&self) -> DetectionResult {
        // check ~/.codex/, which codex
    }

    fn spawn(&self, ctx: &SpawnContext) -> anyhow::Result<SpawnSpec> {
        let mut args = vec![];
        if let Some(uuid) = ctx.resume_from {
            args.extend(["resume".into(), uuid.into()]);
        }
        Ok(SpawnSpec {
            binary: self.binary_path.clone(),
            args,
            env: hashmap! { "CLUIHUD_SESSION_ID" => ctx.session_id.into() },
        })
    }

    async fn start_event_pump(&self, _session_id: &str, _sink: EventSink) -> anyhow::Result<()> {
        // No-op — events arrive via the shared Unix socket from cluihud hook subcommands
        // configured in ~/.codex/hooks.json. Foundation's hooks/server.rs dispatches based on
        // the agent_id resolved from CLUIHUD_SESSION_ID.
        Ok(())
    }

    async fn submit_plan_decision(&self, _: &str, _: PlanDecision) -> anyhow::Result<()> {
        Err(AdapterError::NotSupported(AgentCapability::PLAN_REVIEW))
    }

    async fn submit_ask_answer(&self, session_id: &str, answers: serde_json::Value) -> anyhow::Result<()> {
        // Same FIFO mechanism as CC. The cluihud hook ask-user subcommand for Codex
        // creates a FIFO; the frontend writes the decision; Codex hook reads it.
        write_to_fifo(self.fifo_path_for(session_id), &answers).await
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        let entry: CodexRolloutEntry = serde_json::from_str(line).ok()?;
        // Map per spike-confirmed schema
        match entry { /* ... */ }
    }
}
```

## Hook config produced by setup_agent('codex')

```json
{
  "hooks": {
    "SessionStart": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send session-start --agent codex", "async": true }] }
    ],
    "PreToolUse": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send pre-tool --agent codex", "async": true }] }
    ],
    "PostToolUse": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send tool-done --agent codex", "async": true }] }
    ],
    "PermissionRequest": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook ask-user --agent codex", "sync": true, "timeout": 86400 }] }
    ],
    "UserPromptSubmit": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook inject-edits --agent codex", "sync": true }] }
    ],
    "Stop": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send stop --agent codex", "async": true }] }
    ],
    "SessionEnd": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "cluihud hook send session-end --agent codex", "async": true }] }
    ]
  }
}
```

## Risks

- **R1**: Rollout JSONL schema no completamente documentado. Mitigación: spike captura una sesión real y documenta cada entry observado.
- **R2**: Codex pasa de v1 a v2 con breaking changes en hooks. Mitigación: spike captura version; setup escribe el shape de la versión instalada (con detection upfront).
- **R3**: Trust-gate no detectable programáticamente. Mitigación: fallback a banner siempre visible "if Codex behaves unexpectedly, run `codex trust` from a terminal in this project".
- **R4**: PermissionRequest puede no cubrir todos los casos de "ask user". Mitigación: si el spike encuentra un evento adicional (ej. `AskUserQuestion`-like), agregamos un matcher más al hooks.json.
- **R5**: cost field en rollout puede no exponer cache_read/cache_write tokens (OpenAI APIs los reportan distinto que Anthropic). Mitigación: `RawCost` es flexible; cache fields se setean a 0 si Codex no los reporta.
