Warning: no stdin data received in 3s, proceeding without it. If piping from a slow command, redirect stdin explicitly: < /dev/null to skip, or wait longer.
## Estado de los issues de R2

- **R2#1 resuelto.** `resume_blocked_reason(is_live, has_pty)` revisa primero si la sesión está live y conserva el texto de `worktree_sessions.rs:1007`. El escenario nuevo del spec y la matriz de 3.1 lo cubren.
- **R2#2 resuelto.** Release hace compare-and-remove contra su `pty_id` y llama `mark_dead_if` en el mismo lock hold. Repetí el contraejemplo de R2 contra el diseño y ya no se reproduce. El test existente de `pty.rs:2322` (reserva `pty-a` y libera) sigue válido pasando `pty-a`.
- **R2#3 resuelto.** Walk 4.8 ahora cubre la ruta feliz después de cerrar el tab.
- **R2#4 resuelto, sin regresión en el estado estable.** Verifiqué los otros lectores de `session_activity`:
  - **`messaging.rs:192-233`.** `target_busy` encola, y el reintento diferido de `just_idled` relee el modo.
  - **`worktree_sessions.rs:511-526`.** `notify_outcome` despierta a la sesión solo con `idle`.
  - **`directory.rs:247`.** El descriptor muestra el modo.
  - **`hooks/server.rs:974-978` y `:1467-1473`.** Son escritores, y `record_activity` reemplaza el valor anterior.

  `SessionStart` ya mapea a `running` (`hooks/events.rs:258-263`) y está registrado (`agents/claude_code/adapter.rs:121`). Por eso hoy una sesión recién lanzada ya queda en `running` hasta su primer `Stop`. Marcar `running` al reservar solo cubre los ~5 s de warmup previos a `SessionStart`. El caso "notas encoladas para siempre si el usuario nunca escribe un prompt" ya existe hoy. No veo riesgo de deadlock: `record_activity` toma un shard de `session_activity` adentro de `session_ptys`, y ningún path hace el orden inverso, porque los `session_activity(..)` clonan y sueltan el guard enseguida.

## Issues

1. **Medium: cuatro escenarios nuevos del spec contradicen el requisito de visibilidad que ya existe.**
   - **Qué dice el spec vigente.** `openspec/specs/session-directory/spec.md:8` y `:65` muestran una sesión no live solo si su marker `last_stop_at` cae dentro de la ventana. `directory.rs:144-151` (`list_sessions`) y `:189-195` (`get_session`) lo implementan así: sin marker, la sesión no aparece y `get_session` devuelve not found.
   - **Qué afirman los escenarios nuevos.** Que la sesión se reporta con `is_live: false`, pero sus GIVEN no garantizan un marker:
     - "Registering the adapter does not make a session live": `list_sessions` SHALL report S with `is_live: false`.
     - "App restart starts with no live session": lo mismo.
     - "Closed tab is not live": `get_session(S)` SHALL report `is_live: false`.
     - "Soft-closed tab stays live": este sí aplica, porque la sesión está live.
   - **Consecuencias.**
     - Test 1.2, tal como está escrito ("registered … reports `is_live: false` in `list_sessions` and `get_session`"), falla si no siembra un transcript marker.
     - Quien implemente el Red puede "arreglarlo" en la dirección equivocada, por ejemplo relajando la visibilidad.
     - Walks 4.7 y 4.8 solo pasan si la sesión cerró al menos un turno antes de matarla.
   - **Fix.**
     - Agregar "and S has a transcript marker within the history window" a los GIVEN de esos escenarios, y que 1.2 siembre el marker.
     - O cambiar el THEN a "S SHALL NOT appear with `is_live: true`" y agregar un escenario explícito: sin marker, la sesión desaparece del listado.

2. **Medium: el proposal no declara un cambio de comportamiento visible.**
   - **El cambio.** Hoy, una sesión creada y cerrada antes de su primer `Stop` sigue listada como live por el cache. Con el plan desaparece de `list_sessions` y `get_session` responde not found, porque queda no live y sin marker.
   - **Qué no cambia.** `request_session_resume` no depende del marker (`worktree_sessions.rs:388-408`), así que sigue funcionando con el id.
   - **Por qué importa.** Es correcto según el requisito vigente, pero un agente que guardó ese id va a ver un cambio de semántica.
   - **Fix.** Mencionarlo en What Changes o en Risks, con el escenario de arriba.

3. **Low: una sesión no live ahora puede mostrar campos de actividad viejos.**
   - **Hoy.** Las sesiones matadas en esta vida del daemon se reportaban live, así que no había contradicción.
   - **Con el plan.** Aparecen como `is_live: false` con `mode: "running"` (si las mataron a mitad de turno, o si el spawn falló después de que la reserva marcó `running`), además de `recently_touched_files` y `last_assistant_message` poblados.
   - **Qué contradice.** Las instrucciones del MCP server dicen "for a non-live session the live activity fields … are empty". Lo que dice el spec (`:55-56`) solo cubre las sesiones que el daemon no vio desde que arrancó, así que no lo viola en sentido estricto.
   - **Fix barato.** Que `descriptor_from` use el modo de la DB cuando `!is_live`, o documentarlo en Risks.

4. **Low: el guard de D5 lee dos fuentes sin atomicidad.**
   - **Ruta 1.** `live_session_ids()` devuelve `false`. Antes de que corra `has_session_pty()`, el usuario abre la sesión a mano y se reserva su PTY. El guard ve `(false, true)` y responde "close the tab" a una sesión que en realidad está live.
   - **Ruta 2.** Un kill entre las dos lecturas puede dar el mismo par con el orden inverso.
   - **Impacto.** En los dos casos la aprobación falla igual. Solo el texto queda mal en una ventana chica.
   - **Fix opcional.** Un `PtyManager::resume_block_state(&agents, id)` que lea las dos fuentes bajo el lock de `session_ptys`, que ya es el orden total de D2.

5. **Info.** Rollback, seguridad y concurrencia del EOF y del kill siguen sin cambios respecto de R2. `record_activity("running")` en `resume_session` (`worktree_sessions.rs:1018`) queda redundante pero no hace daño. El comentario actualizado de `:1014-1017` debería decirlo.

VERDICT: REVISE
