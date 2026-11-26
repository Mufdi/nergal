# Revisión del plan `osc7501-session-status` (FRI-623)

Comprobé las afirmaciones del plan contra el código de la rama y contra el checkout de wezterm (`5046fc2`). El enfoque central funciona: el pre-scan con la respuesta escrita antes de `terminal.advance_bytes` sí deja la respuesta 7501 antes del DA1. Pero hay huecos en concurrencia, en los consumidores del estado y en las convenciones del repo.

1. **High: `mcp/messaging.rs` ignora el root status, y el plan da por hecho lo contrario.**
   - **Dónde falla el plan:** "Verified codebase facts" dice que `mcp/directory.rs:245` "is the only place that needs the combined mode". D7 justifica el mapeo con "Other agents use `mode` to decide whether a session can take a message". Pero la entrega real decide con `ctx.agents.session_activity(to)` directo (`mcp/messaging.rs:192-200` y otra vez en el deferred wake, `:221-224`). No usa el descriptor ni `combined_mode`.
   - **Folder-trust:** una sesión con el prompt de folder-trust abierto (root `blocked`, sin actividad de hooks) cuenta como no busy. `drain_idle` pega el mensaje y un Enter dentro del diálogo de trust. Eso puede aceptar la opción por defecto ("trust this folder") sin que el usuario la elija. El bug ya existe hoy, pero este change trae justo la señal que lo cierra y no la usa.
   - **Cablear `combined_mode` ahí no basta:** una sesión `working` después de `Stop` (background) quedaría "queued" esperando un Stop-drain que ya pasó. Es el mismo strand de BUG-08 que documenta el comentario de `messaging.rs:186-191`. El plan necesita un disparo de drain en la transición del root status (`working`/`blocked` → `done`/`idle`/`error`), con su test.

2. **High: el clear en EOF no está protegido por `pty_id`.**
   - **El precedente en el repo:** `on_agent_pty_eof` (`pty.rs:970-976`) usa `mark_dead_if(session_id, pty_id)` justamente para que "the EOF of an old PTY cannot mark a newer PTY of the same session dead".
   - **Qué propone el plan:** D4 y el paso 4 limpian `program_status` por `session_id` nada más. Así, el EOF (o el drain final) del PTY viejo borra o pisa el status del PTY nuevo de la misma sesión y emite `state: null`.
   - **Arreglo:** guardar el `pty_id` dueño junto al `ProgramStatus` (o en la key), y aplicar `apply`/`clear` solo si coincide.
   - **Tests:** agregar "EOF del PTY viejo no borra el status del nuevo" y "reporte tardío del PTY viejo se ignora".

3. **Medium: la escritura síncrona de la respuesta en el reader thread es un camino de lock y bloqueo nuevo. El design dice lo contrario.**
   - **Qué afirma el plan:** D2 y Risks dicen que el reader thread tomando el writer "is the same as today for wezterm's own answers".
   - **Qué hace el código hoy:** wezterm nunca escribe desde el reader thread. Envuelve el writer en `ThreadedWriter`, que usa un channel y un thread propio (`term/src/terminalstate/mod.rs:448-478`; también lo dice el comentario en `pty.rs:23`). Con el plan, el reader thread toma el mutex de `SharedWriter` y hace un `write_all` bloqueante mientras tiene el mutex de `TerminalSession`.
   - **El deadlock:** `write_to_session_pty` o un bracketed paste tienen el writer en un `write_all` bloqueado porque la cola de input del PTY está llena. A la vez, Claude Code (Node escribe síncrono a un TTY) está bloqueado escribiendo stdout y no lee stdin. El reader thread se bloquea en el writer, no drena el output, y el emitter/render se bloquea en el mutex de la sesión.
   - **Probabilidad:** baja, porque el query solo sale en el probe de arranque. Aun así coincide con los pastes de arranque (bind/launch).
   - **Arreglo:** darle a wezterm un writer que encole en un channel propio de Nergal, con un solo thread consumidor, y encolar la respuesta en ese mismo channel antes de `advance_bytes`. El orden se mantiene y no hay I/O bloqueante bajo el lock.
   - **Corregir el texto de D2:** dice "written later, under the same writer mutex". El orden se cumple porque wezterm encola después, no por el mutex.

4. **Medium: "Stamp `last_activity`… without touching its mode" (paso 3) está indefinido y choca con otros consumidores.**
   - **La contradicción:** si no hay entrada en `session_activity`, el plan no dice si crea una ni con qué `mode`. Si la crea, el caso "live session with a root status but no hook activity" del mismo paso no puede ocurrir. Y cualquier `mode` que invente pasa a ser el "hook mode" que leen `messaging.rs` y `combined_mode`.
   - **Interacción con `just_idled`:** el re-stamp con cada reporte (`working` se reenvía cada vez que cambia el `msg` "N still running") reinicia la ventana de `just_idled` (`messaging.rs:197-200`). Eso difiere la entrega una y otra vez.
   - **Arreglo:** guardar el timestamp del root status en el propio `ProgramStatus`, sin tocar `session_activity`, y que el descriptor use el máximo de los dos.

5. **Medium: `programStatusMapAtom` rompe una convención MUST del repo.**
   - **Qué propone el plan:** el paso 5 define `atom<Record<string, …>>({})`.
   - **La regla:** `src/stores/sessionScope.ts:3-7` exige que todo `Record<sessionId, T>` se cree con `sessionScopedMapAtom`, porque si no `pruneSessionStateAction` no limpia la key al cerrar la sesión para siempre.
   - **Arreglo:** usar `sessionScopedMapAtom<ProgramStatusEntry>()`.

6. **Medium: el frontend borra el status en `session_end` y el backend no, así que UI y MCP divergen.**
   - **El caso `/clear`:** dispara `SessionEnd` y luego `SessionStart` en el mismo proceso de Claude Code. Si el último root enviado ya era `idle`, Claude Code no lo reenvía (solo manda diffs).
   - **Resultado:** la UI cae a hooks (`session_start` → `"active"` → thinking) y el MCP sigue en `idle`.
   - **Arreglo:** o se aplica la misma regla en `AgentRuntimeState` desde el hook server, o no se borra en ninguno de los dos lados y se deja al `state=clear` de Claude Code. El plan tiene que elegir una y testearla.

7. **Medium-low: el scanner no define cómo se aborta un OSC, y puede discrepar de termwiz.**
   - **El hueco:** D3 solo contempla BEL y `ESC \`. No dice qué pasa con un `ESC` seguido de otro byte (en VT eso aborta el OSC y empieza otra secuencia), ni con CAN/SUB (`0x18`/`0x1a`).
   - **El riesgo:** si el scanner sigue juntando bytes donde termwiz ya cortó, se traga output real hasta encontrar un terminador o llegar al cap de 4096. En ese tramo puede perderse un query o un reporte legítimo.
   - **Arreglo:** abortar el body con `ESC` no seguido de `\` (y reprocesar ese `ESC` como posible inicio de prefijo) y con CAN/SUB. La misma regla aplica al estado de "skip" del body sobredimensionado.
   - **Tests:** agregar estos casos a 1.1.

8. **Low: los tests 2.1 no van a funcionar tal como están escritos.**
   - **`drain()`:** `CapturedWriter::drain()` (`session.rs:697-709`) vuelve apenas hay un byte. La respuesta síncrona llega antes que el DA1 asíncrono de wezterm, así que el test "reply then DA1" lee solo la respuesta y falla. Hace falta un helper que espere hasta tener el largo esperado.
   - **Constructor:** `session_with_writer` usa `TerminalSession::new` sin reply writer. El plan nombra un "constructor or builder" sin elegir; hay que fijar la firma en la interfaz para el test-writer.
   - **Compilación en `pty.rs:823-828`:** el `match` tiene un arm que devuelve `()` y el otro `Vec<_>`, así que no compila. La nota "callers that ignore the return compile by not binding" no aplica ahí.

9. **Low: `msg` llega sin sanear al MCP `waiting_for` y al estado.**
   - **El problema:** `String::from_utf8_lossy` deja pasar controles C0/C1 y secuencias ANSI. `waiting_for` lo leen otros agentes (es superficie de inyección).
   - **Por qué existe:** cualquier output crudo que llegue al PTY del agente puede falsificar reportes.
   - **Paridad:** el hook path ya expone `message` tal cual (`hooks/events.rs:323-326`), así que es lo mismo que hoy.
   - **Arreglo:** quitar los caracteres de control antes de cortar a 200 caracteres, con un test.

10. **Low: el gap de folder-trust queda cerrado a medias.**
    - **Qué cambia y qué no:** root `blocked` mueve solo el indicador. El tint de la tab/fila y las notificaciones siguen saliendo de `pendingAsksAtom`/`pendingAttentionAtom`, así que el folder-trust no avisa como un AskUserQuestion.
    - **Pedido:** si es intencional, el proposal debe decirlo y no presentar el gap como cerrado.

11. **Low: tests y walk incompletos.**
    - **Faltan tests de:** la regresión de entrega en messaging, la carrera de PTY viejo contra PTY nuevo, y la idempotencia del `session_end`.
    - **Walk 5.5:** verifica `list_sessions` pero no que un `send_to_session` a esa sesión quede en cola y se entregue cuando termina el background.

**Rollback:** está bien. No hay persistencia y, sin la respuesta al query, Claude Code no activa el protocolo.

VERDICT: REVISE
