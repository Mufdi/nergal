---
type: review
feature: osc7501-session-status
reviewer: adversarial
reviewed_at: 2026-11-26T15:25:05-03:00
verdict: FAIL
---

# Review — osc7501-session-status

> Rules: review only the diff and the artifacts named below. Skip anything a gate or CI already enforces (types, lint, formatting, test pass/fail). Tag every finding with its pass and its severity. **Important** = would ship a defect, a vulnerability, or a divergence from spec/plan. **Nit** = everything else; **max 5 nits**, drop the rest.

## Diff scope

- Files changed: 44 · LOC: +3408 / -127 (`git diff main...HEAD`, sin `docs/shortcuts.md`).
- Artefactos leídos: `proposal.md`, `design.md`, `implementation.md`, `tasks.md`, `specs/**`, `docs/design.md` §3.10, `work/osc7501/research.md`.

## Pass 1 — Bugs

- [Important] `src-tauri/src/hooks/server.rs:1114` — El gate nuevo del `Stop` drain cambia el comportamiento sin OSC 7501. El caso es una sesión con una espera de background agent preservada. En `Stop`, `keep_wait` (`:1060-1063`) evita `record_activity`, así que el hook mode sigue en `needs_attention`. Antes, el `Stop` drain pegaba igual en el prompt libre. Ahora `delivery_busy` da `true` y el drain no corre. El mensaje espera a `agent_completed` o al próximo turno del user. Esto rompe el goal "Behavior without OSC 7501 stays exactly as today" (`design.md:16`). También empeora el riesgo residual de D9 (`design.md:100`): dice "as today", y hoy el `Stop` drain sí entregaba. → Excluye del gate el caso `needs_attention` con prefijo `background agent` y sin root `blocked`. Otra opción: corrige el goal y D9 y pide decisión al user. Agrega un test del gate (ver Pass 3).

- [Important] `src-tauri/src/mcp/messaging.rs:63-68` — `drain_idle` devuelve `0` en dos casos distintos: "nada pendiente" y "un wake sigue in flight" (`mcp/delivery.rs:210`). Con el segundo, `ready_drain` llama igual a `drain_worktree_outcomes` y pega el outcome mientras el wake anterior espera su Enter. El cambio agrega dos disparadores nuevos (reader thread y hook server) que pueden coincidir con el deferred wake (`messaging.rs:296`) o con el `Stop` drain. Un ejemplo: un send justo después de `Stop` y el `done` del mismo turno. Los dos drains corren cerca de T+3 s. D9 afirma "The in-flight guard of `drain_idle` prevents a double paste when both drains meet" (`design.md:94`). El código no lo cumple cuando hay un outcome encolado. El spec pide "at most one paste per transition". → Haz que `drain_idle` devuelva un resultado de tres valores (`Pasted(n)`, `Nothing`, `InFlight`). Llama a `drain_worktree_outcomes` solo con `Nothing`, aquí y en `hooks/server.rs:1123-1128`. Agrega un test con `MockDelivery` y un wake in flight.

- [Important] `src-tauri/src/mcp/messaging.rs:271-275` — El send path usa dos reglas de asentamiento distintas para la misma transición. `just_idled` solo mira el hook mode `idle` y su `last_activity`. Un root que pasa a not busy no abre la ventana. Caso 1: el turno termina, Claude Code escribe `done` y el hook `Stop` aún no llega. El hook sigue en `running`, así que `just_idled` es `false` y el wake pega de inmediato. Antes ese mensaje quedaba `queued` hasta el `Stop` drain. Caso 2: el folder-trust prompt cierra (`blocked` → `idle`) y `SessionStart` deja el hook en `running`. Un send en los 2 s siguientes pega en el TUI que aún vuelve al prompt. El `ready_drain` espera `IDLE_SETTLE_SECS + 1` para esa misma transición. Un paste que queda pegado sin enviar se marca consumido, porque el Enter se escribió bien. Eso deja el mensaje varado, contra el invariante del user. → Calcula `just_idled` con el instante en que el target dejó de estar busy: el máximo entre el `Stop` del hook y el `set_at` del root cuando el root decide. Agrega un test con root `done` y hook `running` recién puesto.

## Pass 2 — Security

- [Important] `src-tauri/src/pty.rs:873` — El reader thread aplica cualquier report OSC 7501 de la PTY del agente. Esa PTY es un shell interactivo (`pty.rs:816`). Claude Code arranca dentro del shell y el shell sigue vivo antes y después de Claude Code. En esas ventanas, bytes de datos no confiables llegan crudos al scanner: `cat` de un archivo, la salida de `curl` o un commit preparado. Esos bytes pueden fijar `blocked` con un `msg` arbitrario. Ese texto llega a otros agentes como `waiting_for` en `list_sessions` y `get_session`. También pueden fijar `blocked` para frenar la entrega, o `idle` para disparar un ready drain. Para eso no hace falta ejecutar código: basta con imprimir datos. El hook socket, en cambio, exige un proceso del mismo uid. Por eso la nota de riesgo es incorrecta: "same trust level as the hook `message` field" (`design.md:143`). → Acepta reports solo mientras Claude Code tiene el protocolo activo. Por ejemplo, arma un flag por PTY cuando el scanner ve la query, y desármalo en `SessionEnd` y en EOF. Ignora los reports sin flag armado. Corrige la nota de riesgo en D-Risks.

## Pass 3 — Compliance (vs specs/**, design.md, implementation.md, project CLAUDE.md conventions)

- [Important] `src-tauri/src/hooks/server.rs:1064-1072`, `:1114`, `:1600-1604` y `src-tauri/src/pty.rs:189-204` — El task 4.1 está en `[x]`, pero dos de sus tests no existen. Faltan "the `Stop` drain does not paste for a root `blocked` target" y "the `agent_completed` clear with root `working` schedules a ready drain". `a_background_agent_completing_frees_a_working_root` solo prueba `delivery_busy`. Tampoco hay test para el disparador del reader thread ni para el gate `agent_session` (escenario "Quake shell reports are ignored"). Un helper puro habría cubierto los tres sitios, por ejemplo `fn ready_transition(busy_before, busy_after, event) -> bool` más `fn stop_drain_allowed(...)`. Ese helper habría mostrado el hallazgo 1 de Pass 1 y la rama muerta del Nit 1. → Extrae el helper, pruébalo con los escenarios de `cross-session-messaging`, y deja 4.1 en `[ ]` hasta entonces (`config.yaml`: tasks `[x]` verificados contra el código).

- [Nit] `src-tauri/src/hooks/server.rs:1064-1072` — Esta rama no puede dispararse. El único hook mode not busy es `idle`, y solo `Stop` y `SessionEnd` lo producen (`hooks/events.rs:286`). Las dos están excluidas. Con root `done`, `idle` o `error`, `busy_before` ya es `false`. Con root `blocked`, `busy_after` sigue en `true`. La rama solo corre si un report cambia el root entre las dos lecturas. Ahí agenda un segundo drain, que es el caso del hallazgo 2 de Pass 1. El spec presenta este disparador como cobertura ("or a hook event other than `Stop` and `SessionEnd`"). → Bórrala y ajusta D9 y el requirement, o documenta qué evento futuro la usa.

- [Nit] `src-tauri/src/agents/state.rs:262-284` — `apply_program_status` hace `get` y después `insert` o `get_mut`, sin atomicidad. Con un report idéntico de la PTY B, el EOF de la PTY A puede borrar la entrada entre `get` y `get_mut`. El status de B se pierde y Claude Code no lo reenvía. → Usa `self.program_status.entry(session_id)` para comparar y escribir bajo un solo lock de shard.

- [Nit] `src/components/layout/StatusBar.tsx:68-76` y `:139` — El punto de modo de la status bar sigue leyendo solo `activeModeAtom` (hooks). Después de `Stop` con trabajo en background, el tab muestra working y la status bar muestra idle gris. La UI resuelve el mismo concepto de dos formas. → Deriva el color del punto de `resolveIndicatorState`, o deja documentado en D6 que la status bar muestra solo el hook mode.

- [Nit] `openspec/changes/osc7501-session-status/design.md:142`, `:52` y `specs/program-status-protocol/spec.md:83` — Hay dos derivas de documento. (1) D-Risks dice que el test de pty "checks both reach the shared writer in order". El test no comprueba orden (`pty_program_status_tests.rs:70-72`), y `implementation.md` §2 explica por qué. (2) D4 y el spec dicen que un report idéntico "changes nothing". El código cambia el dueño a la PTY que reporta (`agents/state.rs:269-271`). El cambio es correcto, pero el texto no lo dice. → Alinea los dos textos con el código.

- [Nit] `docs/architecture.md:59` y `:67` — La lista de `terminal/` no nombra `program_status` ni `write_queue`. El párrafo del terminal no dice que wezterm escribe por `PtyWriteQueue` ni que un pre-scan lee OSC 7501 antes de wezterm. Ese doc es la referencia viva del flujo de bytes. → Agrega una línea por módulo y una frase sobre el pre-scan y la write queue.

Acceptance criteria (escenarios de los delta specs):

- [x] `program-status-protocol` scanner: split, oversized, abort por `ESC`, CAN, otros OSC — `terminal/program_status_tests.rs:32-112`.
- [x] `program-status-protocol` parse: working, blocked con kind y msg, control chars, child, clear, clear con id, unknown, msg inválido — `terminal/program_status_tests.rs:118-204`.
- [x] `program-status-protocol` dueño por PTY viva: late report, reemplazo, EOF viejo, EOF dueño, `set_at` idéntico — `agents/program_status_state_tests.rs:160-221`.
- [.] `program-status-protocol` "Quake shell reports are ignored" — gate en `pty.rs:873`, sin test (walk 6.8; ver el Important de Pass 3).
- [x] `program-status-protocol` eventos: emisión única, `state: null` en EOF, `SessionEnd` conserva el status — `agents/program_status_state_tests.rs:81`, `pty_program_status_tests.rs:117`, `src/stores/programStatus.test.ts:67`.
- [x] `terminal-wezterm`: respuesta exacta, antes de DA1, query partida, report sin respuesta, nada dibujado — `terminal/session_status_tests.rs:44-98`.
- [x] `session-directory`: los siete escenarios — `agents/program_status_state_tests.rs:243-380`.
- [x] `session-state-indicators`: escenarios nuevos y de fallback — `src/lib/sessionIndicatorState.test.ts:18-105`.
- [x] `cross-session-messaging` send path: folder-trust, background tras `Stop`, root `idle` con hook `running` — `mcp/messaging_status_tests.rs:184-237`.
- [x] `cross-session-messaging` worktree outcome con root `blocked` — `mcp/notify_outcome_tests.rs:31`.
- [.] `cross-session-messaging` "The Stop drain skips a blocked target" — `hooks/server.rs:1114`, sin test.
- [.] `cross-session-messaging` "Answering the folder-trust prompt delivers the queued message" — `ready_drain` probado (`messaging_status_tests.rs:299`); el disparador `pty.rs:202` no.
- [.] `cross-session-messaging` "A background agent completing drains the queue" — `hooks/server.rs:1600-1604`, sin test del agendado.
- [x] `cross-session-messaging` busy al final de la ventana y las 24 combinaciones — `messaging_status_tests.rs:31-93`, `:314`.

## Harness entropy check

- Speculative abstractions? Sí, una: el disparador de `hooks/server.rs:1064-1072` no tiene evento que lo active (Nit 1).
- WHAT-comments? No. Los comentarios agregados explican el porqué.
- New deps without user confirm? No. Solo `tokio` con `test-util` en `[dev-dependencies]`, ya previsto en D9.
- `unwrap()`/`expect()` fuera de tests: ninguno. Seams `cfg` nuevos: ninguno.

## Verdict

FAIL — El núcleo es sólido. El scanner, el parser, la write queue con la garantía de respuesta antes de DA1, la ownership por `live_ptys` y el resolver del frontend están bien hechos y bien probados. Los fallos están en la capa de entrega. El gate del `Stop` drain cambia el comportamiento sin OSC 7501 y contradice D9. `drain_idle` no distingue "in flight" de "vacío" y permite un doble paste. El send path ignora el asentamiento cuando el root libera al target. Además, cualquier dato impreso en el shell del agente puede falsear el root status. Los tests prometidos para `hooks/server.rs` no existen y el task 4.1 figura cerrado.

---

## Review previa (frontend), preservada

> Este archivo ya tenía la review del reviewer `frontend`. Se conserva sin cambios debajo, para no perderla.

# Review — frontend

reviewer: frontend
verdict: APPROVE con 2 Important (sin blockers)

Alcance: diff frontend de `main...HEAD` (src/ y docs/design.md §3.10). Los audits renderizados (teclado, focus, contraste medido) no se pudieron ejecutar: la app es Tauri sobre WebKitGTK y playwright no la maneja. Se revisó desde el código y los tokens. No se toca ninguna superficie de teclado o focus.

## Blockers
Ninguno.

## Important
1. `src/components/session/SessionIndicator.tsx:22` y `src/styles/globals.css:882-889`: bajo `prefers-reduced-motion`, `error` (`bg-red-500`) y `attention` (`bg-primary`, naranja `#ea580c`) quedan como dots sólidos estáticos del mismo tamaño y de tono cercano. A 6px (`xs`) cuesta distinguirlos, sobre todo con deuteranopía o protanopía. El design.md solo argumenta `done` vs `working`. Fix: dar a `error` una forma distinta (por ejemplo un glifo `X` o `AlertCircle` de lucide como hace `completed` con `Check`) o documentar el riesgo aceptado en §3.10.
2. `src/components/session/SessionIndicator.tsx:22-23` y `:28-31`: `error` (rojo) y `working` (verde) difieren solo por tono en reduced-motion y con CVD rojo-verde. Ambos son círculos sólidos, y el único diferenciador de `working` es la animación que se anula. La luminancia sí difiere (verde `#22c55e` más claro que rojo `#ef4444`), pero es una señal débil a 6-8px. Misma mitigación que el punto 1 (forma distinta para `error`).

## Nits
1. `src/components/session/SessionIndicator.tsx:47`: línea en blanco suelta entre los atoms; mueve `programStatusMapAtom` junto a los otros `useAtomValue` para mantener el bloque contiguo.
2. `src/components/session/SessionIndicator.tsx:22`: `done` usa `ring-1 ring-inset` sobre un dot de 6px. El centro transparente queda de 4px y el anillo de 1px; en pantallas de baja densidad puede leerse como un dot más tenue. Verifícalo a mano en `xs`; con `size-2` o más no hay problema.
3. `docs/design.md:480-481`: el párrafo sigue diciendo "All three respect `prefers-reduced-motion`" justo después de añadir los dots estáticos. Aclara que `done` y `error` no animan, para no dejar la frase ambigua.
4. `src/lib/sessionIndicatorState.ts:41`: el `default` del switch cubre `working` y `blocked` (este último ya retornó antes). Un `case "working"` explícito más un chequeo exhaustivo haría que un `RootState` nuevo falle en `tsc` en vez de caer en `working` en silencio.
5. `src/stores/programStatus.ts:27`: `_removed` sin uso depende de la config de lint; `tsc` y los tests pasan, pero confirma que `eslint`/`noUnusedLocals` lo toleran (prefijo `_`).

## Verificado sin hallazgos
- Contraste: `green-500` (`#22c55e`) sobre `#0a0a0b` ronda 9:1 y `red-500` (`#ef4444`) ronda 5:1. Ambos superan 3:1 para componentes UI no textuales (WCAG 1.4.11). Estimado por tokens, no medido en la app.
- Accesibilidad: el dot sigue `aria-hidden` y decorativo junto al nombre de la sesión. `error` y `done` no tienen texto alternativo, igual que el resto de estados; la información equivalente vive fuera del dot (sin regresión).
- Reduced motion: `done` y `error` no animan; no requieren regla nueva en `globals.css`.
- Anti-slop: sin gradientes, glows ni tokens nuevos; reutiliza el vocabulario de §3.10.
- Convenciones: `resolveIndicatorState` e `isAwaitingSession` son puros y están testeados; no hay comentarios WHAT añadidos; sin voseo.
- Spec: la precedencia (attention, completed, root) coincide con el spec; `blocked` sin hook attention da el tint `nergal-ask-pending` en `TopBar.tsx:479` y `SessionRow.tsx:67`.
