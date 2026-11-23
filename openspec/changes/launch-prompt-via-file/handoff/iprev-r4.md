## Review: ronda 4 de `launch-prompt-via-file`

La inversión del registro de Obsidian a "marcar el caso bueno" es correcta. Pero el plan la conecta mal en el camino `Gone`, y la re-verificación de la reserva queda descrita con el orden de locks invertido. Las dos cosas se arreglan editando pocas líneas del plan.

1. **Medium: `Gone` vuelve a escribir en el vault (D3 "The startup-kill path … disarms the guard", implementation "`ReservationOutcome::Gone`: `rollback.disarm()`" y "`disarm()` inserts `pty_id` into `launched_ptys`").**
   - El plan hace que el camino `Gone` llame a `disarm()`, y `disarm()` mete el `pty_id` en `launched_ptys`.
   - En `pty.rs:822-831`, después del `disarm`, viene `drop(orphan)`. El reader thread de ese shell llega a EOF, `take_launched` devuelve `true` y se ejecuta `finalize_session_obsidian` (`pty.rs:578`).
   - Con eso se gasta el `claim_finalization` de una sesión que nunca llegó a correr. Es justo el bug de la ronda 3 #1, y contradice el propio D3 ("a `Gone` exit … never write to the vault") y el scenario "A launch that never wrote its command…".
   - El test de task 1.1 ("false for an id that never disarmed (rollback, `Gone`, failed spawn)") no lo detecta, porque prueba el guard aislado y simula `Gone` como "nunca desarmado", que no es lo que hace el código del plan.
   - Fix recomendado: en `Gone`, **no** desarmar y dejar que el guard haga rollback. En ese punto el rollback es idempotente y no hace daño:
     - `session_ptys[sid]` ya no apunta a este `pty_id`, así que no se toca la reserva.
     - `mark_dead_if` está guardado por id.
     - `instances.remove` devuelve `None`, porque el orphan ya se extrajo.
     - No hay archivo ni prompt consumido.
   - Alternativa: separar `disarm()` en `commit()` (marca launched) y `defuse()` (no marca nada). En los dos casos hay que corregir el texto de D3, la línea de implementación y el bullet del test.

2. **Medium: la re-verificación de la reserva, tal como está escrita, invierte el orden de locks y puede dejar la app en deadlock (D3 "`start_claude_session` resolves the writer under the `instances` lock, and passes `None` when … `session_ptys[sid]` no longer equals this `pty_id`", implementation "check every place that nests these two locks and keep the existing order").**
   - El orden que ya existe en el código es `session_ptys` → `instances`, anidados: `kill_session_aux_shells` en `pty.rs:1342-1343`. `kill_session_pty_inner` llama a esa función como primer paso (`pty.rs:1587`).
   - Si `deliver_launch` toma `instances` y dentro lee `session_ptys`, que es lo que sugiere "under the instances lock … re-check session_ptys", el orden queda al revés. Un kill concurrente durante el launch puede entonces bloquear para siempre los dos mutex, y con ellos todos los comandos de terminal.
   - El plan deja la decisión al implementador ("check … keep the existing order") en vez de fijarla, en un punto que la ronda 3 ya marcó para confirmar.
   - Fix: escribirlo explícito. Tomar `session_ptys`, comprobar que `session_ptys[sid] == pty_id`, tomar `instances` **dentro** de ese lock, sacar el writer, y mantener los dos locks durante `write_all` + `flush`.
   - Consecuencia buena: como el kill tiene que tomar `session_ptys` para quitar la reserva, sostener ese lock durante la escritura cierra la ventana que Risks da por aceptada ("A kill can land between the reservation re-check and the PTY write"). El kill queda estrictamente antes de la escritura (writer `None`, rollback) o estrictamente después (cierre normal de un shell ya lanzado). Ese riesgo se puede borrar o reescribir.
   - El costo es que `session_ptys` queda bloqueado durante la escritura de una línea corta, lo mismo que hoy pasa con `instances` en `pty.rs:975-980`.

3. **Low: `launch_prompt_file_path` no valida el stem, pero el kill path lo usa para borrar.** `write_launch_prompt_file` rechaza `/`, `..` y similares. En cambio, `discard_launch_prompt` en `kill_session_pty_inner` arma la ruta con `launch_prompt_file_path(dir, pty_id)` sin validar, y hace `remove_file` sobre ella.
   - Hoy los session ids los genera Nergal, así que no hay un camino explotable conocido.
   - Aun así, la propiedad "cannot escape the folder by construction" solo vale para la escritura, no para el borrado.
   - Fix: poner la validación del stem dentro de `launch_prompt_file_path`, que pasaría a devolver `Option<PathBuf>` o `io::Result`, para que la usen tanto el writer como los dos borrados.

4. **Low: entrada huérfana en `launched_ptys` si el shell muere antes de la escritura.** Si el shell sale solo durante la espera de 5 s y la escritura al master igual devuelve `Ok`, `commit()` mete el id después de que su EOF ya pasó, y nadie lo saca nunca. No es un bug funcional: no se escribe al vault y el id no se repite. Basta una línea en Risks, o hacer que el EOF deje un tombstone; no hace falta más.

5. **Low: el walk 2.7 puede dar un conteo inestable.** `drop_off_main_thread` mata el shell en otro thread, así que un `pgrep -P … -c` justo después del error puede contar el hijo que todavía se está cerrando. Hay que indicar que se espere alrededor de 1 s, o mejor comparar la lista (`pgrep -P <pid> -a`) en vez del número.

**Ítems de la ronda 3:**
- #2 (helper de discard con dir explícito): resuelto.
- #3 (contrato de `Drop`): resuelto.
- #4 (texto del requisito recortado y nota sobre el modo `"running"`): resuelto.
- #6 (crear el dir solo si falta, walk que reemplaza el dir por un archivo): resuelto.
- #7 (test renombrado): resuelto.
- #1 (Obsidian): el diseño de marcar el caso bueno es correcto, pero el camino `Gone` lo rompe (ver #1 arriba).
- #5 (kill durante la entrega): parcialmente resuelto. La re-verificación está, pero mal ordenada respecto de los locks (ver #2).

El núcleo de seguridad no cambió y sigue sólido. El prompt nunca pasa por el parser ni por el line editor, `--` y `--prompt=` impiden que se lea como opción, y el archivo es `0600` dentro de un dir `0700` verificado por owner, creado con `create_new`. Los dos bloqueantes son ediciones de una o dos líneas en D3 y en la implementación del plan.

VERDICT: REVISE
