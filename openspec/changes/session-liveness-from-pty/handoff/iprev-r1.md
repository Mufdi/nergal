Esta es mi revisión de forma independiente del plan FRI-601 (`session-liveness-from-pty`). Verifiqué contra el código cada referencia `path:line` del plan. Todas coinciden salvo una nota menor en el ítem 9. El enfoque central es correcto: un set de liveness aparte, con el `pty_id` como generation token, saca la liveness de `agent_id_cache` y anula la re-registración que hace el hook en `hooks/server.rs:958-967`. Quedan contradicciones entre el spec y los paths reales, carreras de orden y huecos de cobertura que conviene cerrar antes de implementar.

## Issues

1. **High: el escenario "Failed spawn is not live" del spec es falso para la mitad de los paths de fallo de `start_claude_session`.** El spec pide "WHEN `start_claude_session` for S fails … THEN S SHALL not be in the live set". D2 / implementation §2 solo hacen rollback en el error de `spawn_pty` y en `Gone`. Después de `Intact` la función puede devolver `Err` por varios caminos:
   - `registry.get(&agent_id).ok_or_else(...)?` (`pty.rs:815-818`)
   - `adapter.spawn(&spawn_ctx)...?` (`pty.rs:889`)
   - `state.instances.lock()?`, `writer.lock()?`, `write_all(...)?`, `flush()?` (`pty.rs:921-925`)

   En todos ellos la reservation y la instancia del PTY siguen vivas: un shell pelado sin agente. La sesión queda en `live_ptys`. Hay que hacer una de dos cosas:
   - acotar el spec a "the `spawn_pty` step fails or the session is killed during startup";
   - o hacer rollback completo (kill de la instancia + `release_session_pty` + `mark_dead_if`) en los errores posteriores a `Intact`.

   Marcar not-live sin liberar la reservation sería peor: el siguiente `start_claude_session` devuelve el id existente por el early return y la sesión nunca vuelve a quedar live.

2. **High: con un PTY que salió solo, el plan crea un resume que no lleva a nada.** Con el plan, el EOF limpia la liveness, pero ese path no toca `session_ptys`. Solo los kill paths lo hacen (`pty.rs:1523-1526`, `:219`). Hoy esa sesión responde `already_live`. Después del cambio responde `pending`, se puede aprobar, y `resume_session` (`worktree_sessions.rs:1012-1018`) la deja en `running`. `approveWorktreeRequest` solo la activa (`src/stores/worktreeGate.ts:90-93`), y cualquier `start_claude_session` choca con el early return (`pty.rs:743-747`) con el `pty_id` muerto. El agente nunca re-spawnea, no hay `mark_live`, y el mensaje relayed nunca se entrega. El escenario "Self-exited PTY is not live" vuelve accionable vía MCP un estado que el plan no sabe recuperar. Opciones:
   - que el EOF del agent PTY libere `session_ptys[session]` con compare-and-remove contra su `pty_id`, y que el frontend re-spawnee al activar;
   - o documentar esto como gap conocido en Risks y en el spec, y que el approval de un resume falle si `session_ptys` todavía tiene una reservation para la sesión.

   Hoy no hay ninguna de las dos.

3. **Medium: hay carreras de orden entre kill y start, y el goal "cannot change liveness the wrong way" no se cumple.** `mark_live` va después de que `reserve_session_pty` suelta el lock de `session_ptys`. `mark_dead` en kill va después de que `kill_session_pty` suelta el suyo. Contraejemplo sobre la propuesta de D3:
   1. kill quita `pty-1` de `session_ptys` y suelta el lock;
   2. un `start_claude_session` concurrente reserva `pty-2` y hace `mark_live(pty-2)`;
   3. kill ejecuta `mark_dead(session)` sin condición.

   Resultado: `pty-2` está corriendo y la sesión reporta not live hasta el próximo spawn. Corren en hilos distintos: `kill_session_pty` es sync en el main thread y `start_claude_session` es async en un worker. Hay una carrera simétrica entre reserve y `mark_live` de dos starts entrelazados con un kill, en la que `mark_live(pty-1)` pisa a `pty-2` y después el `Gone` de `pty-1` borra la entrada. La ventana es chica, pero el plan vende el generation token justamente como garantía de orden. Fix barato:
   - kill usa `mark_dead_if(session, &removed_pty_id)` con el id que sacó de `session_ptys`, no `mark_dead` incondicional;
   - `mark_live` y `mark_dead_if` se ejecutan con el lock de `session_ptys` tomado, por ejemplo dentro de `reserve_session_pty` / `kill_session_pty_inner`, para que ambos maps compartan un orden total.

4. **Medium: regresión en el warmup de las sesiones aprobadas, y Risks no la menciona.** `resume_session` marca `running` en `worktree_sessions.rs:1014-1018`, y el comentario de esas líneas dice que es para que un `send_to_session` durante el warmup quede en *cola* en vez de pegarse en un prompt que todavía no está listo. Lo mismo pasa con la registración al crear en `worktree_sessions.rs:969`. Con el plan, entre el approval y la reserva del frontend, `send_to_session` responde `inactive_target` y no encola. El outcome note ya le dijo al requester "approved — session X created". El riesgo de D2 solo cubre el doble resume. Hay que argumentar en Risks por qué la ventana es despreciable (el requester necesita un turno de LLM antes de mandar) o encolar los envíos a sesiones aprobadas que aún no tienen PTY. Además, el comentario de `:1014-1017` deja de ser cierto tal como está escrito y hay que actualizarlo.

5. **Medium: los tests no cubren el wiring que el plan cambia.**
   - 2.1 prueba `kill_session_pty_inner` y `on_agent_pty_eof`, pero ningún test cubre `mark_live` en reserve, ni el rollback de `spawn_pty`-error/`Gone`. Ese es el escenario "Failed spawn is not live" y es justo el código más propenso a romperse. `start_claude_session` necesita `AppHandle`, así que hay que extraer la parte reserve+mark (y su rollback) a un helper que reciba `&PtyManager` y `&AgentRuntimeState`. Así se testea igual que los tests de reservation existentes.
   - Ningún test cubre la llamada del reader thread vía `try_state`. Si el wiring falta o usa mal el tipo, todos los unit tests igual pasan. 3.6/3.7 cubren el kill en un walk manual, pero no hay walk para el self-exit (`exit` dentro del tab del agente y luego `list_sessions`).
   - Falta un test de regresión del escenario "late hook": `mark_dead` → `register_session` (lo que hace el hook server) → sigue not live. Tarea 1.1 lo cubre solo parcialmente ("register_session alone").

6. **Low: dónde va `mark_dead` dentro de `kill_session_pty`.** El body actual empieza con `kill_session_aux_shells(&state, &session_id)?;` (`pty.rs:1521`). Si `mark_dead` queda "plus" al final (implementation §2), un lock envenenado ahí hace el early return y la sesión queda live con el PTY a medio desmontar. Hay que especificar que la liveness se limpia junto a la remoción de `session_ptys`, no al final.

7. **Low: shutdown y restart.** `shutdown_all` (`pty.rs:205-222`) limpia `session_ptys` pero no `live_ptys`. En la práctica los EOF que siguen lo limpian y el proceso termina igual, así que solo pido mencionarlo en D3 como un kill path más que se revisó. Al reiniciar, el set arranca vacío y solo `start_claude_session` lo llena. Es correcto y conviene decirlo en el spec, porque antes un hook podía revivir la sesión tras un restart.

8. **Low: seguridad.** El cambio achica la superficie de confianza: menos destinos aceptados para pegar en un PTY. El riesgo que Risks describe como "paste into the shell" en realidad es *ejecución* en el shell del usuario si la delivery manda Enter. Está fuera de alcance por decisión del usuario y no lo re-discuto, pero el texto de Risks debería decirlo con precisión. No veo superficies de inyección nuevas.

9. **Low: precisión de citas.** Los tests de reservation están en `pty.rs:~2300-2340`, no en `~2400-2445`. Además, el rustdoc de `live_session_ids` (`state.rs:155-156`) y el module doc de `state.rs:8` describen la semántica vieja ("sessions with an entry in the agent cache"); hay que listarlos en las tareas para que no queden desactualizados.

10. **Info: rollback.** El estado es solo en memoria y no hay cambios de schema ni del shape del IPC (Tauri inyecta los params `State`), así que revertir el commit basta. No hay riesgo de fallo parcial persistente.

VERDICT: REVISE
