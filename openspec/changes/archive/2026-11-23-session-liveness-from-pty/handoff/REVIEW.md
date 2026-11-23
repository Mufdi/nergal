---
type: review
feature: session-liveness-from-pty
reviewer: adversarial
reviewed_at: 2026-11-22T00:00:00-03:00
verdict: PASS_WITH_NOTES
---

# Review — session-liveness-from-pty

> Rules: review only the diff and the artifacts named below. Skip anything a gate or CI already enforces (types, lint, formatting, test pass/fail). Tag every finding with its pass and its severity. **Important** = would ship a defect, a vulnerability, or a divergence from spec/plan. **Nit** = everything else; **max 5 nits**, drop the rest.

## Diff scope

- Files changed: 6 modificados + 3 archivos de test nuevos · LOC: +242 / -40 en `src-tauri/src` (sin tests nuevos), +367 en tests nuevos.
- Rutas revisadas contra el live set: `reserve_session_pty`, `release_session_pty`, `kill_session_pty_inner`, rama `Gone` de `start_claude_session`, EOF del reader thread, `shutdown_all`, `delete_session`, `delete_workspace` (frontend mata los PTY antes, `src/stores/pendingDeletes.ts:258`), deep-link (pasa por `start_claude_session`), aux y quake shells (`agent_session = false`).
- Resultado: ninguna ruta deja una sesión live sin PTY ni un PTY de agente vivo sin liveness, salvo los casos que el design ya declara.

## Pass 1 — Bugs

- Ningún hallazgo Important. Las escrituras del live set respetan el orden de locks `session_ptys` → DashMap en todas las rutas. El token `pty_id` protege el EOF tardío y el release tardío.

## Pass 2 — Security

- Ningún hallazgo Important o Nit propio del cambio. Ver Pass 3 sobre la identidad del caller, que sigue leyendo el agent cache.

## Pass 3 — Compliance (vs specs/**, design.md, implementation.md, project CLAUDE.md conventions)

- [Important] `src-tauri/src/agents/state.rs:308-313` — `resolve_session_hint` sigue validando la identidad contra `agent_id_cache`. Su rustdoc dice "Resolve … to a live `nergal_session_id`". El comentario de `src-tauri/src/mcp/mod.rs:758` dice "Re-resolved against the live registry". El spec `nergal-mcp-server` (`openspec/specs/nergal-mcp-server/spec.md:59`) exige validar la identidad contra el registro de sesiones live. Ahora existen dos definiciones de "live": el live set para el directorio y el cache para la identidad. Un id de una sesión ya matada sigue identificado y puede actuar como `sender` en `send_to_session`. → Validar el hint nergal contra `live_ptys` (y el hint CC contra el nergal id resuelto), o modificar en el delta el requisito de identidad para que diga "sesión conocida". Declarar la decisión en `proposal.md`.
- [Important] `src-tauri/src/mcp/worktree_sessions.rs:1050-1056` — El spec principal `agent-spawned-worktrees` (`openspec/specs/agent-spawned-worktrees/spec.md:141`) dice que el target "SHALL be marked busy the instant it is approved so delivery to it queues until its first idle". Con este cambio, entre la aprobación y la reserva del frontend la sesión no es live. Un `send_to_session` en esa ventana devuelve `inactive_target` y no encola. `design.md` lo acepta como riesgo, pero `proposal.md` no lo declara y el delta no modifica ese requisito. → Agregar un MODIFIED requirement al delta de `agent-spawned-worktrees` que describa la ventana, y listarla en "What Changes" del proposal.
- [Important] Scenario "Soft-closed tab stays live until the kill" (`specs/session-directory/spec.md`) — no hay test. Ningún test verifica `is_live: true` para una sesión con PTY reservado antes del `kill_session_pty`. → Agregar un test en `src-tauri/src/mcp/liveness_tests.rs` que reserve el PTY y lea `get_session` antes del kill.
- [Nit] `src-tauri/src/pty.rs:787-794` — El `pty_id` es `pty-<session>-<millis>`. El design lo usa como token de generación, pero dos spawns de la misma sesión en el mismo milisegundo producen el mismo id. Entonces el EOF tardío del PTY viejo borra la liveness del nuevo. → Agregar un contador `AtomicU64` al id.
- [Nit] `src-tauri/src/pty.rs:192-198`, `src-tauri/src/agents/state.rs:170-172` — `has_session_pty` y `mark_dead` no tienen llamadores de producción, solo tests. `proposal.md` dice que la aprobación usa `has_session_pty`, pero usa `resume_block_state`. → Quitar las dos funciones o restringirlas a `#[cfg(test)]`, y corregir el proposal.
- [Nit] `src-tauri/src/mcp/mod.rs:220-223` — Las `instructions` del MCP dicen que `mode` está vacío para una sesión no live. El código y el delta devuelven el `mode` persistido. → Sacar `mode` de esa lista y nombrar `last_assistant_message` y `session_crons`.
- [Nit] `src-tauri/src/mcp/messaging.rs:67`, `src-tauri/src/mcp/mod.rs:262` — El hint de `inactive_target` manda a `create_worktree_session`. Para una sesión existente la ruta correcta es `request_session_resume`. El texto es previo al cambio, pero ahora cada tab cerrada llega a este hint. → Nombrar `request_session_resume` en ambos textos.
- [Nit] `proposal.md` ("Every live-set change happens under the same lock") y `design.md` D2 — El EOF (`src-tauri/src/pty.rs:580-582`) y `forget_session` (`src-tauri/src/agents/state.rs:161`) escriben el live set sin el lock de `session_ptys`. El comentario de `src-tauri/src/pty.rs:201-203` promete un par atómico que el EOF puede partir. El resultado es benigno, pero la afirmación es falsa. → Acotar la afirmación a reserve, release y kill.

- Acceptance criteria (`specs/session-directory/spec.md`):
  - [x] Requirement "Session liveness tracks the agent PTY" — `agents/state.rs:164-185`; consumidores en `mcp/directory.rs:122,174`, `mcp/messaging.rs:64`, `mcp/worktree_sessions.rs:398-408,757`. Set vacío al arrancar: `agents/state.rs:133`.
  - [x] Closed tab is not live — `pty.rs:1589-1596`; tests `killing_a_live_session_clears_its_reservation_and_liveness`, `a_not_live_descriptor_uses_the_persisted_mode_and_empty_live_only_fields`.
  - [x] Self-exited PTY is not live — `pty.rs:580-582,705-711`; test `eof_of_the_current_pty_clears_liveness` (el cableado del reader thread no tiene test).
  - [x] Late hook event does not revive — `hooks/server.rs:966` solo registra; tests `a_late_hook_registration_after_the_kill_leaves_the_session_not_live`, `send_to_a_registered_but_not_live_session_returns_inactive_target`.
  - [x] Late exit of a replaced PTY — test `eof_of_an_older_pty_keeps_the_newer_pty_live`.
  - [x] Registering the adapter does not make live — test `a_registered_session_that_was_never_marked_live_reports_not_live`.
  - [x] Failed spawn is not live — `pty.rs:817,829`; test `releasing_the_current_pty_clears_the_reservation_and_liveness`.
  - [x] Just-spawned session queues — `pty.rs:681-682`; test `a_new_reservation_marks_the_session_live_with_mode_running`.
  - [x] Releasing an older reservation keeps the newer — `pty.rs:695-700`; test `releasing_an_older_pty_keeps_the_newer_reservation_and_liveness`.
  - [.] Soft-closed tab stays live until the kill — sin test (ver Important arriba).
  - [x] App restart starts with no live session — `bootstrap()` vacío; test `a_registered_session_that_was_never_marked_live_reports_not_live`.
  - [x] Killed session without a marker leaves the listing — test `a_killed_session_without_a_marker_leaves_the_listing_but_still_accepts_a_resume_request`.
  - [x] Not-live descriptor carries no live activity — `mcp/directory.rs:244-263`; test `a_not_live_descriptor_uses_the_persisted_mode_and_empty_live_only_fields`.
- Acceptance criteria (`specs/agent-spawned-worktrees/spec.md`):
  - [x] Requirement "Resume approval refuses a session whose terminal is still open" — `mcp/worktree_sessions.rs:756-777,1001-1009`; `pty.rs:204-216`.
  - [.] Self-exited PTY in an open tab — test `resume_block_state_tracks_reservation_eof_and_release` y `resume_blocked_reason_matrix` cubren la decisión. Ningún test verifica el estado `failed` del gate ni la ausencia de prompt encolado.
  - [x] Closed tab resumes normally — `resume_blocked_reason(false, false) == None`; `resume_block_state` tras release devuelve `(false, false)`.
  - [x] Live session at approval still answers message it directly — `resume_blocked_reason_matrix` verifica que `is_live` gana sobre `has_pty`.

## Harness entropy check

- Speculative abstractions? Sí, menor: `has_session_pty` y `mark_dead` sin uso de producción (Nit arriba). · WHAT-comments? No; los comentarios agregados explican por qué. · New deps without user confirm? No.

## Verdict

PASS_WITH_NOTES — No hay bugs ni problemas de seguridad Important. Todas las rutas que crean, matan o desmontan un PTY de agente mantienen el live set coherente, y el token `pty_id` resuelve las carreras de EOF y release tardíos. Quedan tres divergencias de compliance: la identidad del caller sigue en el agent cache, la ventana aprobación→reserva contradice un requisito no modificado, y falta el test del soft-close.

PASS


## Seguimiento (orquestador)

El user confirmó los 3 hallazgos Important y los nits de texto. Quedaron resueltos en el grupo 4 de `tasks.md`: la identidad MCP lee el live set, el spec documenta la ventana entre la aprobación y la reserva, el soft-close tiene test, y los textos apuntan a `request_session_resume`. El nit del `pty_id` con milisegundos queda fuera de alcance.
