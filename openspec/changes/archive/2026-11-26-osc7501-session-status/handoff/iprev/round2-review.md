# Revisión ronda 2: `osc7501-session-status` (FRI-623)

Comprobé el diff contra el código de la rama y contra `vtparse` de wezterm (`5046fc2`). La ronda 1 quedó resuelta casi entera:

| Ítem | Estado |
|---|---|
| #4 `session_activity` | ✅ |
| #5 `sessionScopedMapAtom` | ✅ |
| #6 `SessionEnd` | ✅ |
| #8 `drain_len` y el caller `match` | ✅ |
| #9 saneo de `msg` | ✅ |
| #10 tint | ✅ |
| #1 entrega | ⚠️ |
| #2 ownership | ⚠️ |
| #3 write queue | ⚠️ |
| #7 abort del scanner | ⚠️ |

En los cuatro ⚠️ el arreglo introdujo un defecto o se apoya en una premisa falsa. Corrijo además una premisa mía de la ronda 1: allí dije que una sesión en folder-trust "no tiene actividad de hooks". Eso es falso, y el plan heredó el error (punto 1).

1. **High: el drain al salir de `blocked` nunca entrega en el caso folder-trust.**
   - **Por qué:** `reserve_session_pty` (`pty.rs:946-947`) llama `record_activity(session_id, "running", None)` al reservar el PTY. Además `SessionStart` mapea a `running` (`hooks/events.rs:273-278`).
   - **La secuencia real:**
     1. Reserva → hook `running`.
     2. Prompt de trust → root `blocked`.
     3. El usuario confía → root `idle`, y llega `SessionStart` → hook `running`.
     4. A los 3 s, `delivery_busy` ve el hook `running`, responde `true` y salta el drain.
     5. El mensaje queda varado hasta el primer `Stop`, que en una sesión sin turno no llega nunca.
   - **Qué queda mal en el plan:**
     - El escenario "Closing the dialog delivers the queued message" de `cross-session-messaging` falla en la app real.
     - El walk 6.6 va a fallar.
     - La frase del proposal "When the dialog closes, the queued message is delivered" es falsa.
     - Las premisas "no hook-derived activity" (D9, escenario 1 de `cross-session-messaging`, escenario "Root status without hook activity" de `session-directory`) no ocurren en un PTY vivo.
   - **Arreglo:** cuando hay root status, que sea la señal principal de busy: `busy = root blocked || (root working && hook running)`. Sin root status queda la regla de hoy.
     - `done`, `idle` y `error` no son busy aunque el hook diga `running` (que puede venir de la reserva o de `SessionStart`).
     - `working` con hook `idle` (background después de `Stop`) sigue sin ser busy, así que no se reabre BUG-08.
     - El hook `needs_attention` sobra: AskUser, permisos y plan review ya llegan como root `blocked`. Mantenerlo arrastra el caso de un `Notification` `idle_prompt`, que deja hoy la sesión `needs_attention` (busy) para siempre.
   - **Tests:** reescribir los escenarios con el estado real (hook `running` desde la reserva) y agregar "trust → root idle + SessionStart → drain entrega".

2. **Medium: con "el primer dueño gana", un PTY viejo que tarda en cerrar deja sin status al PTY nuevo.**
   - **Qué dice el plan:** D4 y `apply_program_status` ignoran un reporte "when an entry exists with another `pty_id`".
   - **Por qué falla:** el reader del PTY viejo recibe EOF recién cuando el lado slave se cierra, o sea cuando el proceso hijo termina. Eso no ocurre al hacer drop del master. Mientras tanto el PTY nuevo de la misma sesión ya está reservado.
   - **Consecuencia:** se ignoran sus primeros reportes (`clear`, luego `idle`). Como Claude Code solo reenvía diffs, la sesión nueva no muestra su status hasta que este cambie, y mientras tanto queda el status viejo (por ejemplo `blocked`, que retiene la entrega).
   - **Arreglo:** la autoridad ya existe. `live_ptys` se fija en `mark_live` al reservar (`pty.rs:946`) y se limpia en `mark_dead_if` (`agents/state.rs:176-179`).
     - Aplicar un reporte solo si `live_ptys[sid] == pty_id`. En ese caso reemplaza cualquier entrada de otro dueño.
     - El EOF sigue protegido por dueño.
   - **Tests:** el escenario "Late report from an old PTY is ignored" sigue valiendo. Falta "el PTY vivo pisa la entrada de un PTY viejo que no cerró".

3. **Medium-low: la write queue no lleva solo respuestas del emulador; lleva todo el input de teclado.**
   - **Qué dicen D2 y Risks:** "Only emulator answers go through the queue" y "they are a few bytes per query".
   - **Qué hace el código:** por el writer de wezterm pasan también:
     - todas las teclas: `key_down`, `pty.rs:2228` → `session.rs:312`;
     - los reportes de mouse: `session.rs:449`, `:482`;
     - el focus: `session.rs:319`.
   - **Consecuencia:** todo lo que se teclea pasa a dar dos saltos de thread (`ThreadedWriter` → `QueueWriter` → queue thread). Un error de escritura detiene la queue y mata el teclado. Hoy pasa lo mismo con `ThreadedWriter`, pero el plan no lo nombra.
   - **Arreglo:**
     - Corregir el texto de D2 y de Risks.
     - Agregar un test por `build_terminal_session` (helper cerca de `pty.rs:2856`): `key_down` y la respuesta 7501 llegan por la misma queue al writer compartido, y en orden.
   - **Por qué ese test:** el de 2.1, con `set_status_reply(w.clone())` directo al `CapturedWriter`, prueba un camino síncrono que no es el de producción.

4. **Medium-low: el drain de `Stop` y `drain_worktree_outcomes` se saltan `delivery_busy`.**
   - **El hueco:** D9 protege solo el send path y el deferred wake. `hooks/server.rs:1103-1119` pega sin mirar el root status.
   - **El caso:** un diálogo que abre Claude Code justo después del turno (root `blocked` sin `kind`, por ejemplo la encuesta de feedback, que acepta dígitos) recibe la nota y el Enter.
   - **Arreglo:** que el drain de `Stop` no pegue si el root está `blocked`. El drain al salir de `blocked` ya reintenta.
   - **Límite:** si el reporte llega después del hook, la carrera queda. Hay que nombrarla como residual.

5. **Low: el claim "as in termwiz" sobre el abort es falso. Mi propuesta de la ronda 1 también estaba mal planteada.**
   - **Qué hace vtparse:** en `vtparse/src/transitions.rs`, el `EXIT` de `OscString` es `OscEnd`, y `parse_byte` lo ejecuta en cualquier cambio de estado. Así, `ESC`+x, CAN y SUB *despachan* el OSC en termwiz; no lo abortan.
   - **Qué conviene hacer:** el abort del scanner es la opción más segura, porque evita aplicar un `state=working:msg=QWx` truncado. Hay que mantenerlo pero declararlo como divergencia deliberada.
   - **Otro claim sin respaldo:** el research no muestra que el scanner de Claude Code aborte de esa forma; hay que quitar esa parte.

6. **Low: escenarios de spec sin test automatizado.**
   - **Drain:** "Closing the dialog delivers…" y "A busy target at the end of the settle window…" no se pueden probar, porque `schedule_unblocked_drain` recibe un `AppHandle`.
     - **Arreglo:** que reciba `SharedDb`, `AgentRuntimeState` y `Arc<dyn SessionDelivery>` (como el deferred wake de `messaging.rs:211-226`). Así se prueba con `MockDelivery` (`mcp/delivery.rs:377`) y tiempo de tokio pausado.
   - **"SessionEnd keeps the status":** sin test en 3.1 ni en 5.1. Basta con uno del reducer o del handler de `session_end` que no toque `programStatusMapAtom`.

7. **Low: falta definir si `set_at` se actualiza con un reporte idéntico.**
   - Con "Set stores… set_at = now" y "returns change only when status differs", no queda claro si un reporte idéntico mueve `last_activity`.

**Rollback:** sin cambios respecto de la ronda 1, está bien. La write queue es el único cambio de infraestructura en el camino del teclado, y se revierte con volver a pasar `SharedWriterAdapter` a wezterm.

VERDICT: REVISE
