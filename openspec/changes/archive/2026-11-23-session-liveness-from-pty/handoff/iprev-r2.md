El plan revisado resuelve los 10 puntos de R1, pero el guard nuevo de D5 introduce una regresión y queda una carrera que puede dejar una sesión live para siempre. Lo verifiqué leyendo `pty.rs:632-705`, `:740-780`, `:1515-1545`, `mcp/worktree_sessions.rs:705-800`, `:990-1030` y `mcp/messaging.rs:50-110`.

## Estado de los issues de R1

- **R1#1 resuelto.** El spec quedó acotado al fallo de `spawn_pty` o a un kill durante el startup. Los errores después de `Intact` están documentados en Risks como un estado que ya existe hoy.
- **R1#2 resuelto, pero introduce el issue 1.** D5 cubre el caso del PTY que salió solo con el tab abierto.
- **R1#3 y R1#6 resueltos en reserve, kill y EOF.** Las escrituras van dentro del lock de `session_ptys`, el kill usa `mark_dead_if` con el id que removió, y el EOF hace compare-and-remove con su propio id. Dejé el contraejemplo de R1 corriendo contra el diseño y ya no se reproduce. Release es la excepción (issue 2).
- **R1#4 resuelto.** Risks argumenta la ventana y el comentario de `:1014-1017` entra en las tareas.
- **R1#5 resuelto en su mayoría.** Hay tests de reserve, release, kill con un PTY más nuevo, late hook y `has_session_pty`, y walk 4.8 para el self-exit. Lo que falta está en el issue 3.
- **R1#7 a R1#10 resueltos.** Las citas cuadran con el código.

## Issues

1. **Medium: el guard de D5 cambia la respuesta para una sesión que está live.**
   - **El problema.** Implementation §3 pone el check de `has_session_pty` *antes* de `resume_session`. Una sesión live (PTY corriendo, tab abierto) también tiene su reservation en `session_ptys`, así que `has_session_pty` da `true`.
   - **El caso real.** Es justo el que cubre el re-check de `worktree_sessions.rs:1000-1008`: el request quedó pending, el usuario abrió la sesión a mano y después aprobó.
   - **Hoy.** La aprobación falla con "session became live before approval — message it directly", que es lo correcto.
   - **Con el plan.** Falla con "the session's tab is still open; close it, then request the resume again". Le dice al requester que cierre una sesión sana que debería recibir el mensaje directo.
   - **La API del helper.** `resume_blocked_reason(has_pty: bool)` no puede distinguir los dos casos.
   - **Fix.**
     - Que el guard dispare solo con `has_session_pty && !is_live`. Puede ir dentro de `resume_session`, después del re-check de liveness, o como `resume_blocked_reason(is_live, has_pty)` con el caso live primero.
     - Agregar al spec de `agent-spawned-worktrees` el escenario "live session at approval still answers 'message it directly'".
     - Sumar ese caso al test 3.1.

2. **Medium: `release_session_pty` sigue removiendo sin comparar, y eso puede dejar liveness pegada para siempre.**
   - **El problema.** El plan le pasa el `pty_id` (§2) pero solo lo usa en `mark_dead_if`. La remoción de la reservation sigue siendo `session_ptys.remove(session_id)` sin condición (`pty.rs:654-658`).
   - **Contraejemplo.**
     1. El start A reserva `pty-1` y queda live.
     2. Un kill remueve `pty-1` y ejecuta `mark_dead_if(pty-1)`.
     3. El start B reserva `pty-2` y queda live con `pty-2`.
     4. El `spawn_pty` de A falla y `release_session_pty` remueve la reservation de B. Su `mark_dead_if(pty-1)` no hace nada.
   - **Resultado.** `live_ptys[S] = pty-2` con su instancia corriendo, pero sin reservation. Cuando el tab cierra, `kill_session_pty` no encuentra nada que remover y nunca llama `mark_dead_if`. El PTY queda huérfano y no muere, así que tampoco llega su EOF. La sesión reporta `is_live: true` hasta reiniciar, que es exactamente el bug que este change cierra.
   - **Origen.** La remoción sin comparar ya existía. Lo que agrava el plan: hoy ese estado dejaba un PTY huérfano, y ahora además deja una liveness falsa que nadie limpia.
   - **Fix.** Ya tiene el `pty_id` a mano: compare-and-remove (`if session_ptys.get(session_id) == Some(pty_id)`) más `mark_dead_if`. Sumarlo al test 2.1: release con un `pty_id` viejo no toca ni la reservation ni la liveness del más nuevo.

3. **Low: la cobertura del guard de D5 es solo la función pura.**
   - 3.1 testea `resume_blocked_reason` aislada. Ni el wiring en `approve_worktree_request` ni su orden respecto del re-check de liveness (el issue 1) tienen test automático.
   - Walk 4.8 cubre el rechazo, pero ningún walk cubre la ruta feliz después de cerrar el tab: self-exit, cerrar tab, aprobar y ver que revive.
   - Si el guard sale como función con `is_live` y `has_pty`, el test cubre la matriz completa sin Tauri. Agregar el segundo paso a 4.8.

4. **Low: la sesión está live durante su propio warmup y el modo de actividad viene del ciclo anterior.**
   - **El problema.** D2 marca live al reservar, antes de que exista el shell y unos 5 s antes de escribir el comando `claude`. Si alguien reabre la sesión a mano (sin pasar por una aprobación), el modo de `record_activity` sigue en el `idle` del último `Stop`.
   - **La consecuencia.** Un `send_to_session` en esa ventana pasa `inactive_target` (`messaging.rs:64`) y no encola. La nota llega a un shell pelado, que la ejecuta como comando si la delivery manda Enter.
   - **Origen.** Ya existía (antes `agent_id_cache` también la daba como live) y el plan no lo empeora. Risks solo cubre la ventana de aprobación. Conviene mencionarlo. Fix barato opcional: `mark_dead_if` en kill/EOF resetea el modo, o reserve marca `running`.

5. **Info: rollback, seguridad y concurrencia del EOF.**
   - Rollback y seguridad siguen sin cambios respecto de R1.
   - El EOF fuera del lock de `session_ptys` es correcto: `DashMap::remove_if` es atómico por key y solo puede borrar su propio id.
   - No veo riesgo de deadlock por el orden de locks. `session_ptys` → shard de `live_ptys` es el único anidamiento, y ningún path toma un shard de `live_ptys` y después `session_ptys`.

VERDICT: REVISE
