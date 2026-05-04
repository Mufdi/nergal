## Context

Cluihud nació como wrapper específico de Claude Code (CC), con el hook system de CC como mecanismo central de observación. La auditoría de Fase 0 (ver `docs/agent-discovery-phase0.md` o el chat de origen) identificó 13 subsistemas con touchpoints CC-específicos. Tres agentes objetivo (OpenCode, Pi Code, Codex) divergen significativamente del modelo CC:

- **Codex**: file-config hooks casi idénticos a CC (`~/.codex/hooks.json`).
- **OpenCode**: HTTP+SSE server (`opencode serve` en `:4096`) o JS plugins in-process. **No file-config hooks.**
- **Pi**: ni hooks ni plan mode (por diseño). Solo JSONL tail o RPC stdio.

Cualquier abstracción shallow ("renombrar event names") fallaría: el transport es lo que más diverge, no los nombres. El trait debe modelar transport + capabilities como tipos, no como métodos uniformes.

## Goals / Non-Goals

**Goals:**
- Extraer comportamiento CC a un `AgentAdapter` trait sin regression user-facing.
- Definir trait + enums (`AgentCapability`, `Transport`) que admitan los 4 agentes objetivo.
- Auto-detección filesystem-based al startup; el usuario no configura nada explícitamente para que cluihud "vea" un agente instalado.
- Capability-gated UI: paneles ocultos cuando el adapter no soporta la feature.
- Foundation reusable: agregar un quinto agente en el futuro debe requerir solo un nuevo `agents/<id>/` + entry en el registry.
- Cost capture devuelve raw tokens; pricing por modelo es metadata posterior, no responsabilidad del trait core.

**Non-Goals:**
- Implementar OpenCode/Pi/Codex aquí (changes separadas).
- UI explicit-switching de agente dentro de una sesión activa (un adapter por sesión, fijo desde creación).
- Migración de DB de sesiones CC existentes (mantienen `agent_id = 'claude-code'` por default).
- Synthesizing plan-mode para agentes que no lo tienen nativo (decisión D3: ocultamos).
- Pricing dinámico desde API de proveedores (los adapters devuelven raw tokens; cost USD es responsabilidad de un módulo posterior, fuera de scope).
- Cross-adapter session resumption (cada agente tiene su modelo de session resume; cluihud no intenta unificar).

## Decisions

### Decision 1: Trait modela transport como dato, no como herencia

**Decisión**: `AgentAdapter` declara su `Transport` como variante de un enum, no como subtrait. El runtime selecciona el dispatcher de eventos basándose en `Transport`, no en monomorfismo.

```rust
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> AgentId;
    fn display_name(&self) -> &str;
    fn capabilities(&self) -> &AgentCapabilities;
    fn transport(&self) -> Transport;
    fn detect(&self) -> DetectionResult;
    fn spawn(&self, ctx: &SpawnContext) -> Result<SpawnSpec>;
    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent>;
    fn extract_cost(&self, line: &str) -> Option<RawCost>;
}

pub enum Transport {
    FileHooks { settings_path: PathBuf, hook_event_names: Vec<&'static str> },
    HttpSse { base_url: Url, sse_path: &'static str, permission_endpoint: &'static str },
    JsonlTail { sessions_dir: PathBuf },
    RpcStdio { binary: String, args: Vec<String> },
}
```

**Por qué**: Los 4 transports tienen lifecycle, errores y semántica fundamentalmente distintos (un FIFO no tiene equivalente en SSE; un PUT REST no tiene equivalente en file hook). Modelarlos como variantes hace explícito que el dispatcher es polymorphic. Si en el futuro aparece un agente con transport novel, agregar variante > inventar método uniforme que sirva a todos.

**Alternativa rechazada**: trait con `subscribe_events() -> Stream<Event>` uniforme. Forzaría a los adapters HTTP a wrap-and-translate cada call, y a los file-hook adapters a hacer polling artificial. La heterogeneidad real del problema se ocultaría detrás de una abstracción frágil.

### Decision 2: AgentCapability como bitset, no como métodos

**Decisión**: Las features se expresan como flags. UI gating consulta el bitset. El trait NO declara métodos opcionales como `Option<fn>`.

```rust
bitflags! {
    pub struct AgentCapability: u32 {
        const PLAN_REVIEW           = 1 << 0;
        const ASK_USER_BLOCKING     = 1 << 1;
        const TOOL_CALL_EVENTS      = 1 << 2;
        const STRUCTURED_TRANSCRIPT = 1 << 3;
        const RAW_COST_PER_MESSAGE  = 1 << 4;
        const TASK_LIST             = 1 << 5;
        const SESSION_RESUME        = 1 << 6;
        const ANNOTATIONS_INJECT    = 1 << 7;
    }
}
```

**Por qué**: bitflags = comparación O(1), serialización trivial, debug-friendly. Métodos `Option<fn>` proliferarían (`fn plan_review(&self) -> Option<...>`) y obligarían al frontend a recibir señales de capability + métodos separadas.

**Alternativa rechazada**: trait jerárquico (`AgentAdapter: AgentAdapterPlanReview + AgentAdapterCost`). Combinatoria de subtraits explota; no escala a 8 capabilities.

### Decision 3: AgentId como string newtype validado, no enum

**Decisión**: `pub struct AgentId(String)` con constructor `AgentId::new(s: &str) -> Result<Self, AdapterError>` que valida contra el regex `^[a-z][a-z0-9-]{0,31}$` (1-32 chars, debe empezar por letra lowercase ASCII; chars subsiguientes pueden ser letra lowercase, dígito o guión). Constructores conocidos: `AgentId::claude_code()`, `AgentId::opencode()`, `AgentId::pi()`, `AgentId::codex()`. El registry rechaza registrations con IDs duplicados (`Err(AdapterError::DuplicateAgentId)`).

**Por qué**: Permite agregar adapters externos en el futuro (plugin system) sin recompilar el core. La validación cierra el surface de inyección — `AgentId` se usa en filenames de FIFOs, env vars (`CLUIHUD_AGENT_ID`), DB rows, paths del state directory. Sin validación, un plugin malicioso podría registrar `"../../etc/passwd"` o vacío. El charset es deliberadamente conservador (más restrictivo que filename-safe POSIX); si en el futuro hace falta agregar `_` o uppercase, es bump menor.

### Decision 4: Auto-detección al startup, refresh manual via comando

**Decisión**: `AgentRegistry::scan()` se ejecuta una vez al `app_setup()` y popula `availableAgentsAtom`. Re-scan se dispara via comando `cluihud rescan-agents` o desde Settings. No hay file watcher continuo sobre `~/.{claude,codex,...}/`.

**Por qué**: Los agentes se instalan/desinstalan raramente; un file watcher gasta inotify slots por nada. Manual refresh es la solución pragmática y honesta.

### Decision 5: agent_id por sesión, fijo desde creación

**Decisión**: La tabla `sessions` agrega `agent_id TEXT NOT NULL`. Una sesión está bound a un adapter desde su creación; no se cambia mid-flight. Un proyecto puede tener un override default vía `config.agent_overrides`.

**Por qué**: Los transports son incompatibles (una sesión no puede pasar de file-hooks a SSE sin reiniciar el agente). Forzar la decisión upfront es honesto. La UX de "elegir agente" se hace al crear sesión (modal o picker), default = CC para preservar el flujo actual.

### Decision 6: Pricing fuera del trait, USD privado al CC adapter como bridge

**Decisión**: `extract_cost()` del trait devuelve `RawCost { input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, model_id }`. La conversión a USD NO vive en el trait. **Sin embargo**, para evitar regression UX al usuario CC, el `ClaudeCodeAdapter` mantiene una función privada `legacy_usd_for_sonnet4(raw: &RawCost) -> f64` que reemplaza el actual hardcoded pricing — el cálculo se mueve, no desaparece. El status bar consume USD vía un Tauri command `get_session_cost_usd(session_id)` que internamente delega al adapter; para CC retorna USD via `legacy_usd_for_sonnet4`, para otros adapters retorna `None` hasta que aterrice el módulo `pricing` genérico.

**Aggregation ownership**: como `extract_cost()` emite por-línea pero los totales son por-sesión, introducimos `SessionCostAggregator` (genérico, no agent-bound) en `src-tauri/src/agents/cost_aggregator.rs`. Cada sesión tiene un `SessionCostAggregator` que recibe `RawCost` events y mantiene running totals (`input_tokens_total`, `output_tokens_total`, etc.). Esto reemplaza el comportamiento actual de `parse_cost_from_transcript` (whole-file accumulation) — ahora la accumulación vive en un struct con state, alimentado por el stream de `TranscriptEvent::Cost(...)`.

**Por qué**: Pricing cambia frecuentemente (model releases, discounts) y depende del proveedor — el trait core no debe ser su fuente de verdad. Pero el usuario CC actual ve USD en la status bar; quitarlo sin reemplazo es regression. Solución: el adapter-private USD calc preserva la UX para CC (que es 100% del uso hoy) mientras los otros adapters esperan el módulo `pricing` general (que llegará en una change futura, fuera de scope de esta foundation).

**Consecuencia**: el módulo `pricing` futuro reemplaza `legacy_usd_for_sonnet4` con una tabla data-driven; los call sites del status bar no cambian.

### Decision 7: Capability gating sincrónico en stores, no async fetch

**Decisión**: Los Jotai listeners (`stores/hooks.ts` etc.) verifican capability **antes** de actualizar atoms. La capability set se popula **sincrónicamente** desde la session row al activar la sesión, no via `invoke()` async. La struct `Session` que devuelve `list_sessions()`, `get_session(id)`, y los eventos `session:created` / `session:activated` agrega los campos `agent_id: AgentId` y `agent_capabilities: Vec<String>` (serialized form del bitset). El frontend desempaqueta directo a `agentCapabilitiesAtom` sin invoke separado.

**Por qué evitar invoke async**: Un async fetch deja una ventana TOCTOU donde los listeners de hooks ya están armados pero el atom de capabilities está vacío — eventos legítimos se dropean silenciosamente. Sincronizar desde la session row elimina esa ventana.

**Defensa en profundidad**: aún con la capability set siempre poblada, si un evento llega para un session_id desconocido, el listener lo dropea con un `console.warn`. Pero esto es safety net, no el flow primario.

### Decision 8: OpenSpec reader sale del adapter

**Decisión**: `claude/openspec.rs` se mueve a `src-tauri/src/openspec.rs`. No es agent-bound — es una convención de proyecto (carpeta `openspec/`). Cualquier sesión, cualquier agente, en un proyecto con openspec, debería ver el panel.

**Por qué**: La carpeta `openspec/` no la genera Claude — es un workflow del usuario que cualquier agente puede leer. Mantenerlo en el adapter sería acoplamiento innecesario.

### Decision 9: Hook event → agent_id resolution con cache in-memory + drop policy

**Decisión**: Cuando un hook event llega al socket Unix con un `cluihud_session_id`, el dispatcher resuelve el `agent_id` consultando un `DashMap<SessionId, AgentId>` poblado en `create_session()` y limpiado en `destroy_session()`. La DB es fallback si el cache miss (puede ocurrir tras restart de cluihud con sesiones huérfanas), no la primera consulta. Si la resolución falla (None en cache + None en DB), el evento se dropea con un log `warn` (no panic, no buffer).

**Race con SessionStart hook**: el `SessionStart` hook puede dispararse antes de que la session row sea committed si la inserción al cache se hace post-spawn. Mitigación: insertar al cache **antes** de spawn-PTY (la session row está creada en este punto, solo falta arrancar el child); de esta forma, cuando el child fire-y-olvida el `SessionStart` hook, el cache ya tiene la entrada.

**Por qué cache + DB fallback**: Cache hit en hot path es O(1), evita SQLite I/O por cada hook event (decenas a cientos por minuto en sesiones activas). DB fallback recovers from process crash + restart con sessions vivas. Drop on miss es honesto — un evento sin resolución no tiene a quién pertenecer; logar y descartar es preferible a buffering indefinido.

### Decision 10: Default agent priority + adapter setup gating

**Decisión**: Cuando se crea una sesión y `available_agents` tiene >1 detectado, el picker default usa esta priority list (estable, codificada en `registry.rs`):

1. CC (`claude-code`) — si instalado.
2. Codex (`codex`) — si instalado.
3. OpenCode (`opencode`) — si instalado.
4. Pi (`pi`) — si instalado.

Empate dentro de la lista no es posible (cada uno tiene id único). Si ninguno está instalado, el picker bloquea creación con un mensaje "No agents detected — install at least one".

El usuario puede sobreescribir vía `config.default_agent: Option<AgentId>` o `config.agent_overrides[project_path]`. Lookup priority: `agent_overrides[project] > config.default_agent > registry priority list`.

**Adapter setup gating**: cada adapter declara `requires_cluihud_setup() -> bool` en el trait. CC retorna `true` (escribe `~/.claude/settings.json`); OpenCode/Pi retornan `false` (no consumen hooks file-config); Codex retorna `true` (escribe `~/.codex/hooks.json`). El Settings UI gates el botón "Run setup" por este flag.

**Por qué**: Priority codificada > registration order (que es opaca al usuario). Adapter setup gating > "todos tienen botón" + error UX feo.

### Decision 11: HttpSse con auth slot upfront

**Decisión**: La variant `Transport::HttpSse` incluye `auth: Option<AuthScheme>` desde v1, aunque OpenCode no lo use. `AuthScheme` es un enum **`#[non_exhaustive]`** con variants iniciales `None`, `Bearer(String)`, `Header { name, value }`. Future variants (`OAuth2 {...}`, etc.) se agregan sin breaking change a consumers, gracias al atributo non_exhaustive.

**Por qué**: Si OpenCode (o un agente futuro con HttpSse) introduce auth tokens, el shape de `Transport::HttpSse` no cambia — solo se agrega una variant a `AuthScheme`. `#[non_exhaustive]` fuerza a los consumers a usar `_ =>` en sus matches, garantizando que agregar variants futuras no rompa el build.

### Decision 12: Watchers CC dentro del adapter, lifecycle atado a session

**Decisión**: El transcript watcher (`agents/claude_code/transcript.rs`) y el plan watcher (`agents/claude_code/plan.rs`) **viven dentro del adapter** y se inicializan en `ClaudeCodeAdapter::start_event_pump(session_id, sink)`. Su lifecycle queda atado a la sesión: arrancan en start, se detienen en `stop_event_pump`. Esto contradice ligeramente la frase "start_event_pump es noop para CC" en una versión anterior — la versión correcta es: **start_event_pump arranca los watchers de CC; el Unix socket SHARED para hook events sigue corriendo afuera del adapter (es del runtime), pero los watchers son adapter-owned**.

**Por qué**: Los watchers son CC-specific (transcript JSONL en CC's format, plan files en `.claude/plans/`). Mantenerlos como state del adapter alinea con el modelo de OpenCode (SSE client) y Pi (JSONL tail) — todos usan `start_event_pump` para boot adapter-specific I/O. El Unix socket es infra del runtime de cluihud, no del adapter.

### Decision 13: OpenSpec watcher es workspace-scoped, no session-scoped

**Decisión**: El watcher de `openspec/` (post-move a `src-tauri/src/openspec.rs`) se registra **per-workspace, no per-session**. Cuando el usuario abre un workspace en cluihud, se inicia un único watcher sobre `<workspace_root>/openspec/`. Múltiples sesiones en el mismo workspace comparten el watcher.

**Por qué**: La carpeta `openspec/` es propiedad del workspace, no de la sesión. Una sesión termina, otra empieza, el contenido de openspec persiste. Un watcher per-session sería redundante y costoso.

### Decision 14: Unix socket carries discriminated message kinds

**Decisión**: El wire schema del Unix socket existente cambia para incluir un campo `kind` discriminante:

- `{"kind": "hook_event", ...payload}` — eventos de hook (existente, default cuando `kind` no está presente para backward compat con instalaciones existentes mid-upgrade)
- `{"kind": "control", "op": "rescan_agents"}` — comandos de control (rescan_agents, future: shutdown, status)

El dispatcher routea por `kind`. Hook events siguen al hook handler; control commands van a un control handler separado.

**Por qué**: Sin discriminador, `cluihud rescan-agents` (que escribe al socket) chocaría con el hook event parser y caería en el orphan-drop path. Discriminator explícito mantiene la extensibilidad sin romper hook events existentes.

## Trait sketch

```rust
// src-tauri/src/agents/mod.rs

pub mod claude_code;
pub mod registry;
pub mod cost_aggregator;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("capability not supported by this adapter: {0:?}")]
    NotSupported(AgentCapability),
    #[error("session is not in a state that allows this operation")]
    SessionLocked,
    #[error("invalid agent id: {0}")]
    InvalidAgentId(String),
    #[error("agent id already registered: {0:?}")]
    DuplicateAgentId(AgentId),
    #[error("transport error: {0}")]
    Transport(#[from] anyhow::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(s: &str) -> Result<Self, AdapterError> {
        // ^[a-z][a-z0-9-]{1,31}$  — 1-32 chars, lowercase + digits + hyphen, must start with letter
        let re = regex::Regex::new(r"^[a-z][a-z0-9-]{0,31}$").unwrap();
        if !re.is_match(s) {
            return Err(AdapterError::InvalidAgentId(s.into()));
        }
        Ok(Self(s.into()))
    }
    pub fn claude_code() -> Self { Self("claude-code".into()) }
    pub fn opencode()    -> Self { Self("opencode".into()) }
    pub fn pi()          -> Self { Self("pi".into()) }
    pub fn codex()       -> Self { Self("codex".into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

pub struct AgentCapabilities {
    pub flags: AgentCapability,
    pub supported_models: Vec<String>,
}

pub struct DetectionResult {
    pub installed: bool,
    pub binary_path: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub version: Option<String>,    // populated lazy/async — see Decision 12
    pub trusted_for_project: Option<bool>,  // Codex-specific; None for adapters that don't use a trust gate
}

pub struct SpawnContext<'a> {
    pub session_id: &'a str,         // cluihud session_id
    pub cwd: &'a Path,
    pub resume_from: Option<&'a str>, // agent_internal_session_id (Pi/Codex UUID, CC --continue token, etc.)
    pub initial_prompt: Option<&'a str>,
}

pub struct SpawnSpec {
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,  // MUST include CLUIHUD_SESSION_ID
}

pub enum AuthScheme { None, Bearer(String), Header { name: String, value: String } }

pub enum Transport {
    FileHooks { settings_path: PathBuf, hook_event_names: Vec<&'static str> },
    HttpSse {
        base_url_template: String,   // template with :port placeholder
        sse_path: &'static str,
        permission_endpoint: &'static str,
        auth: Option<AuthScheme>,    // future-proof slot
    },
    JsonlTail { sessions_dir: PathBuf },
    RpcStdio { binary: String, args: Vec<String> },
}

pub enum TranscriptEvent {
    Message { role: String, content: String, model: Option<String> },
    ToolUse { name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, output: serde_json::Value },
    Cost(RawCost),
    Other(serde_json::Value),
}

pub struct RawCost {
    pub model_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

pub struct PlanDecision {
    pub approved: bool,
    pub message: Option<String>,    // for deny: surfaced back to the agent
}

/// Sink that adapters call to forward translated events into the cluihud event bus.
pub type EventSink = tokio::sync::mpsc::UnboundedSender<crate::hooks::events::FrontendHookEvent>;

#[async_trait::async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> AgentId;
    fn display_name(&self) -> &str;
    fn capabilities(&self) -> &AgentCapabilities;
    fn transport(&self) -> Transport;

    /// Whether this adapter requires `setup_agent(id)` to write filesystem config (e.g., ~/.claude/settings.json).
    /// CC + Codex: true. OpenCode + Pi: false.
    fn requires_cluihud_setup(&self) -> bool;

    /// Lightweight detection: must NOT spawn child processes.
    /// `version` is populated lazily by a separate background call (`refresh_version()`) to keep startup fast.
    async fn detect(&self) -> DetectionResult;

    /// Optional async version refresh — runs in background after initial detect.
    /// Default impl returns None (no spawn). CC/Pi/Codex/OpenCode override to call `<binary> --version`.
    async fn refresh_version(&self) -> Option<String> { None }

    fn spawn(&self, ctx: &SpawnContext) -> Result<SpawnSpec, AdapterError>;

    /// Per-line transcript parsing. Stays sync — hot path.
    /// Adapters with file-based transcript (CC, Codex, Pi) parse JSONL lines; OpenCode returns None.
    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent>;

    /// CC, Codex: noop — events arrive via shared Unix socket from `cluihud hook send`.
    /// OpenCode: starts SSE subscription.
    /// Pi: starts JSONL file watcher.
    async fn start_event_pump(&self, session_id: &str, sink: EventSink) -> Result<(), AdapterError>;

    /// Stops any background tasks started by start_event_pump. Idempotent.
    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> { Ok(()) }

    /// For adapters with PLAN_REVIEW. Default impl returns NotSupported.
    async fn submit_plan_decision(&self, _session_id: &str, _decision: PlanDecision) -> Result<(), AdapterError> {
        Err(AdapterError::NotSupported(AgentCapability::PLAN_REVIEW))
    }

    /// For adapters with ASK_USER_BLOCKING. Default impl returns NotSupported.
    async fn submit_ask_answer(&self, _session_id: &str, _answers: serde_json::Value) -> Result<(), AdapterError> {
        Err(AdapterError::NotSupported(AgentCapability::ASK_USER_BLOCKING))
    }
}
```

### Cost aggregator (generic, not in trait)

```rust
// src-tauri/src/agents/cost_aggregator.rs

#[derive(Default, Clone, serde::Serialize)]
pub struct SessionCostTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub messages_counted: u64,
}

pub struct SessionCostAggregator {
    totals: parking_lot::Mutex<SessionCostTotals>,
}

impl SessionCostAggregator {
    pub fn add(&self, raw: &RawCost) {
        let mut t = self.totals.lock();
        t.input_tokens += raw.input_tokens;
        t.output_tokens += raw.output_tokens;
        t.cache_read_tokens += raw.cache_read_tokens;
        t.cache_write_tokens += raw.cache_write_tokens;
        t.messages_counted += 1;
    }
    pub fn snapshot(&self) -> SessionCostTotals { self.totals.lock().clone() }
}
```

The session manager owns one aggregator per session. The hook dispatcher calls `aggregator.add()` whenever `parse_transcript_line` emits `TranscriptEvent::Cost(...)`. This replaces the existing whole-file `parse_cost_from_transcript` accumulation.

## Frontend gating

```typescript
// src/stores/agent.ts
export const agentAdapterAtom = atom<AgentMetadata | null>(null);
export const agentCapabilitiesAtom = atom<Set<AgentCapability>>(new Set());

export const hasCapabilityAtom = atomFamily((cap: AgentCapability) =>
  atom((get) => get(agentCapabilitiesAtom).has(cap))
);

// In PlanPanel.tsx:
const showPlan = useAtomValue(hasCapabilityAtom('PLAN_REVIEW'));
if (!showPlan) return null;
```

## Migration path (single-PR refactor, no transitional re-exports)

We **do not** ship transitional re-exports. Re-exports complicate review and risk leaving them behind. Instead, the move + import-update happens atomically in this change:

1. Trait + enums + registry + cost aggregator skeleton (no consumers yet).
2. Move `claude/transcript.rs`, `claude/cost.rs`, `claude/plan.rs` → `agents/claude_code/`. Move `claude/openspec.rs` → `src-tauri/src/openspec.rs`. Move `tasks/transcript_parser.rs` → `agents/claude_code/tasks.rs`.
3. **In the same commit / PR**, update every importer:
    - `db.rs:8` (currently `use crate::claude::...`) → `use crate::agents::claude_code::...`
    - `commands.rs:5` (same)
    - `plan_state.rs:5` (same)
    - any other `crate::claude::` reference (grep before merging)
    - `crate::tasks::transcript_parser` → `crate::agents::claude_code::tasks`
4. Implement `ClaudeCodeAdapter` wrapping the moved logic.
5. `hooks/server.rs` resolves `agent_id` from session via the in-memory cache (Decision 9), delegates parsing to the adapter.
6. `setup_agent('claude-code')` produces the same output as today's `cluihud setup`.
7. Frontend gates by capability; for CC sessions all flags are on, so UI looks identical.
8. DB migration adds `agent_id` column with default `'claude-code'` + `agent_internal_session_id` (used by Pi/Codex later).
9. Status bar wires CC USD via `legacy_usd_for_sonnet4` (private to `agents/claude_code/cost.rs`); other adapters return `None`.
10. Delete the now-empty `claude/` directory after import grep confirms zero leftovers.

The whole sequence ships as one merge to avoid build-broken intermediate states. Steps 2-3 happen in one commit; steps 4-10 can be split into reviewable sub-commits, all on the same branch.

## Risks

- **R1**: refactor sin regression para CC es no-trivial (13 subsistemas). Mitigación: integration tests que ejerciten plan flow, ask-user flow, task updates, cost extraction, openspec emission, plan FIFO round-trip, annotation injection via UserPromptSubmit, session resume `--continue` / `--resume`, file-changed event flow. Manual walk como complemento, no como substituto.
- **R2**: bitflags en TS no es trivial. Mitigación: serializar como `string[]` en el wire (custom `Serialize` impl en Rust), parsear a `Set<string>` en frontend. Pin `bitflags = "2"` en `Cargo.toml` (sintaxis difiere de v1).
- **R3**: DB migration agrega `agent_id NOT NULL DEFAULT 'claude-code'` + `agent_internal_session_id TEXT NULL`. Validamos con test que ejerce migración sobre DB con rows pre-existing (task 11.7). Downgrade explícito: **no soportado** — SQLite < 3.35 no permite DROP COLUMN; el usuario que quiera revertir debe restaurar backup pre-migration. Documentado en `CHANGELOG`.
- **R4**: Decision 6 mitigada — el USD de CC se preserva via `legacy_usd_for_sonnet4` privado al CC adapter. La regresión UX desaparece para CC. Otros adapters esperan al módulo `pricing` futuro pero no afectan al usuario hoy (no usan otros agentes todavía).
- **R5**: Hook dispatcher hot path bajo `Arc<dyn AgentAdapter>` introduce dynamic dispatch. Mitigación: el lookup de adapter desde el cache es O(1) amortizado (`DashMap::get`), `parse_transcript_line` es sync (no async overhead), y la frecuencia es del orden de decenas de eventos/segundo (no kHz). Sin benchmarks pero acceptable para personal-use scale; si emerge regression observable, agregar bench.
- **R6**: `agent_overrides[project_path]` tiene riesgo de canonicalization mismatch entre escritura/lectura. Mitigación: canonicalizar paths con `std::fs::canonicalize` (o `dunce::canonicalize` cross-platform) en write y en read; serializar como `String`, no `PathBuf`. Test con paths con symlinks + `.` + trailing slash.
- **R7**: `claude --version` durante `detect()` bloquea startup. Mitigación: `detect()` es lightweight (filesystem checks); `refresh_version()` corre en background tokio task post-startup y actualiza el atom cuando termine. UI muestra "version: detecting..." hasta que llega el refresh.
- **R8**: Session creation race con SessionStart hook. Mitigación: cache de `agent_id` por session se popula **antes** de PTY spawn (la session row ya existe en este punto); el hook no puede dispararse antes que el child arranque, así que el lookup desde el hook subprocess siempre encuentra hit.
- **R9**: Agent picker default depende de registration order si no hay priority. Mitigación: priority list codificada (CC > Codex > OpenCode > Pi); user override via `config.default_agent` o `config.agent_overrides[project]`.
