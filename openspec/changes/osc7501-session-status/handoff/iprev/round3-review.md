# Revisión ronda 3: `osc7501-session-status` (FRI-623)

Revisé el diff contra el código de la rama. Estado de los ítems de la ronda 2:

| Ítem r2 | Estado |
|---|---|
| #1 busy rule con root status | ✅ |
| #2 ownership por `live_ptys` | ✅ |
| #3 write queue (texto) | ✅ texto · ⚠️ el test nuevo es racy |
| #4 gate del `Stop` drain | ⚠️ introduce stranding de worktree outcomes |
| #5 divergencia de vtparse | ✅ |
| #6 testabilidad del drain | ⚠️ el harness que se describe no existe |
| #7 `set_at` idéntico | ✅ |

1. **Medium: el gate nuevo del `Stop` drain deja varados los worktree outcomes, y `notify_outcome` queda fuera de la busy rule.**
   - **Qué dice el plan:** que el drain de outcomes en `hooks/server.rs:1103-1119` se salte si `delivery_busy`. La recuperación es `schedule_ready_drain`, que según implementation §4 llama solo a `drain_idle`.
   - **El caso:**
     1. La sesión termina el turno con un outcome encolado.
     2. Aparece un diálogo post-turno, así que el root queda `blocked`.
     3. El `Stop` drain se salta.
     4. El diálogo cierra y el ready drain entrega solo los mensajes cross-session.
     5. El outcome espera hasta el próximo `Stop`, que en una sesión quieta no llega nunca.
   - **El path que falta en la busy rule:** el spec dice "Every PTY delivery path" y enumera cuatro, pero hay un quinto. `notify_outcome` (`mcp/worktree_sessions.rs:509-523`) pega de inmediato con `mode == "idle"` del hook. Con root `blocked` y hook `idle` (un diálogo después de `Stop`) pega dentro del diálogo, lo que rompe el invariante del user.
   - **Arreglo:**
     - Que `schedule_ready_drain` replique la secuencia del `Stop`: `drain_idle`, y si pegó 0, `drain_worktree_outcomes`, respetando la regla de un paste por transición (`server.rs:1107-1111`). Para eso necesita `WorktreeGateState`.
     - Que `notify_outcome` use `!delivery_busy` en lugar de `mode == "idle"`.
     - Agregar los dos escenarios al spec y a 4.1.

2. **Medium: el test de `schedule_ready_drain` "con MockDelivery y paused tokio time" no se puede escribir tal como está.** Hay tres obstáculos:
   - `tokio` no tiene el feature `test-util` (`Cargo.toml:16`), y `[dev-dependencies]` solo trae `tempfile` (`:162-163`). Sin ese feature no existen `start_paused` ni `time::pause`.
   - `schedule_ready_drain` hace `tauri::async_runtime::spawn`, que corre en el runtime global de Tauri y no en el runtime del test. Aunque el test pause el tiempo, el sleep del future spawneado no se entera.
   - `MockDelivery` es un `struct` privado dentro de `mcp::delivery::tests` (`delivery.rs:333`), así que no se ve desde los tests de `messaging.rs`. Esos tests usan `NoopDelivery`, cuyo `wake_idle` devuelve `Err`.
   - **Arreglo:**
     - Separar un `async fn ready_drain(...)` puro (sleep, re-check, drain) que el test haga `await` directo, y dejar a `schedule_ready_drain` como un wrapper que solo spawnea.
     - Agregar `tokio = { features = ["test-util", ...] }` en dev-deps.
     - Exponer `MockDelivery` como `pub(crate)` bajo `#[cfg(test)]`.
     - Sin esto, 4.1 queda sin Red posible, o el builder termina con sleeps reales de 3 s.

3. **Medium-low: el test pty-level de orden key → reply es nondeterminístico.**
   - **Por qué:** wezterm envuelve su writer como `BufWriter::new(ThreadedWriter::new(writer))` (`term/src/terminalstate/mod.rs:509`). Un `key_down` llega a la queue desde el thread de `ThreadedWriter`. La respuesta 7501 entra directo a la queue desde el reader thread.
   - **Consecuencia:** "a `key_down` then a chunk with the 7501 query reach the inner writer … in that order" (tasks 2.1, implementation §2) no está garantizado. El orden depende del scheduling.
   - **El orden que sí está garantizado:** reply antes que DA1, porque la reply se encola síncrona antes de `advance_bytes` y DA1 pasa por el otro thread. Ese caso ya está cubierto.
   - **Arreglo:** que el test espere los bytes de la tecla con `drain_len` antes de alimentar la query, y que afirme que ambos llegan por el writer compartido. No hay que afirmar un orden entre ellos. El reorden entre una tecla y la reply es inocuo, porque cada `write` es un buffer completo.

4. **Medium-low: el invariante del spec promete más de lo que el mecanismo cubre.**
   - **Qué promete el spec:** "A target that starts accepting a prompt is drained" dice que un mensaje nunca queda encolado si su target "has had a busy-to-not-busy transition after the message was queued".
   - **Qué cubre el mecanismo:** solo agenda el drain ante cambios del root status.
   - **El caso que queda fuera:** `Notification agent_completed` (`hooks/server.rs:1585-1591`) pasa el hook de `needs_attention` (background wait) a `idle` sin drain.
     - Con root `working`, eso es una transición busy → not-busy por el lado del hook.
     - El mensaje queda varado hasta el próximo `Stop`.
     - Es un hueco que ya existe hoy, pero ahora el spec afirma lo contrario.
   - **Arreglo:** en el hook server, calcular `delivery_busy` antes y después de `record_activity` (incluido `:1590`) y agendar el mismo ready drain ante busy → not-busy. La otra opción es acotar el texto del requirement a "a root status change".

5. **Low: falta nombrar una race residual nueva al inicio del turno.**
   - **Qué cambia:** con root presente, `idle` + hook `running` ahora es not-busy. Entre `UserPromptSubmit` (hook `running`) y el reporte `working` de Claude Code, el target cuenta como libre.
   - **El caso concreto:** dos sends seguidos al mismo target. El segundo `drain_idle` corre cuando el in-flight guard del primero ya se liberó (tras el submit) pero antes del reporte `working`. Así, el segundo paste cae en un turno que está arrancando.
   - **Antes:** el hook `running` lo cubría.
   - **Arreglo:** declararla junto a la residual del `Stop` en D9. Opcionalmente, se puede tratar como busy un hook `running` más nuevo que el `set_at` del root, pero eso puede reabrir el stale `SessionStart`, así que solo conviene si se ordena por timestamp.

6. **Low: el texto de D9 sobre `working` + `needs_attention` justifica la regla solo con plan review.**
   - **Lo que omite:** esa misma combinación aparece con el background wait (`waiting_for` "background agent…"), que `preserves_background_wait` mantiene tras los `Stop` del foreground.
   - **Consecuencia:** el foreground acepta prompt y aun así el mensaje se retiene. Es el comportamiento de hoy, pero contradice la segunda mitad del invariante.
   - **Arreglo:** nombrarlo como excepción conocida o resolverlo con el arreglo del punto 4. Con ese arreglo, el mensaje sale al menos cuando el agente completa.

**Rollback:** sin cambios, está bien.

**Seguridad:** sin hallazgos nuevos. El saneo de `msg` y el gate por `live_ptys` cierran la superficie.

VERDICT: REVISE
