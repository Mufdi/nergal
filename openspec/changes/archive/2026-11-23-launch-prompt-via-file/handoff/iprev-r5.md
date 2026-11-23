## Review: ronda 5 de `launch-prompt-via-file`

Los dos bloqueantes de la ronda 4 están resueltos y lo verifiqué contra el código. Lo que queda son detalles de implementación de severidad baja, que se pueden fijar al escribir el código sin otra ronda de plan.

**Verificación de los ítems de la ronda 4:**

- **#1 (`Gone`): resuelto.** Con el guard armado, el rollback en `Gone` no hace nada, como dice D3:
  - `session_ptys[sid]` ya no apunta a este `pty_id`, porque `confirm_reservation_or_extract` solo devuelve `Gone` cuando la reserva no está intacta (`pty.rs:740-751`).
  - `mark_dead_if` está guardado por id.
  - `instances.remove` devuelve `None`, porque el orphan ya se extrajo en `:750` o lo quitó el kill en `:1601-1602`.
  - Como nunca se llama a `disarm()`, el EOF del orphan no finaliza Obsidian.
- **#2 (orden de locks): resuelto.** Revisé todos los sitios del archivo. El único que anida los dos locks es `kill_session_aux_shells` (`pty.rs:1342-1344`), con el orden `session_ptys` → `instances` → `aux_line_trackers`. Los demás (`:219-220`, `:1453/1460`, `:1590/1601`, `:1668/1675`, `:1744/1750`…) sueltan `session_ptys` antes de tomar `instances`. Nadie toma `session_ptys` mientras tiene `instances`, así que el orden que fija el plan no puede trabarse con el código actual. Como el kill tiene que pasar por `session_ptys` (`:1590`), la ventana de kill durante la entrega queda cerrada de verdad.
- **#3 (validación del stem en `launch_prompt_file_path`): resuelto.**
- **#4 (id huérfano en `launched_ptys`): resuelto.** Quedó anotado en Risks.
- **#5 (walk 2.7 con lista en vez de conteo y espera de ~1 s): resuelto.**

**Hallazgos nuevos o residuales:**

1. **Low: `disarm()` tiene que correr con `session_ptys` todavía tomado (D3 "`disarm()` runs once the launch command is written"; implementation "on success `rollback.disarm()`").**
   - El plan no dice si `disarm()` corre antes o después de soltar los locks.
   - Si corre después, puede pasar lo siguiente: un kill toma `session_ptys` justo al soltarse, el reader thread llega a EOF y `take_launched` devuelve `false` antes de que `disarm()` inserte el id.
   - Resultado: no se hace la finalización de Obsidian como fallback en una sesión que sí recibió su comando, y el id queda huérfano en `launched_ptys`. El hook `SessionEnd` normalmente cubre la finalización, por eso es low.
   - Fix: insertar en `launched_ptys` dentro del mismo scope de locks, después de `flush`. El orden queda `session_ptys` → `instances` → `launched_ptys`; el reader thread toma `launched_ptys` solo, así que no hay riesgo de deadlock.

2. **Low: `deliver_launch` escribe el archivo del prompt mientras tiene tomado `instances` (D3 "`start_claude_session` calls it while holding both locks").**
   - `instances` es el lock de todo el I/O de terminal de todas las sesiones. El propio código avisa que no hay que hacer trabajo lento bajo ese lock (`pty.rs:1355-1359`).
   - Con este diseño, bajo ese lock quedan:
     - el `mkdir` de `launch_prompt_dir()`;
     - un `create_new` + `write` de hasta 128 KiB, que en el fallback de `ipc_dir` va a disco;
     - el `remove_file` cuando el writer es `None`.
   - Fix: resolver el prompt (escribir el archivo) antes de tomar los locks. Bajo los locks dejar solo la re-verificación, `write_all`, `flush` y el insert en `launched_ptys`. Si la re-verificación falla, borrar el archivo después de soltarlos. Para eso `deliver_launch` se divide en "preparar" y "enviar"; los tests de task 1.1 siguen valiendo.

3. **Low: un prompt reencolado después de un kill deliberado se entrega cuando el usuario reabre la sesión.**
   - En el caso "killed during startup", el rollback reencola el prompt consumido.
   - Si el usuario cerró la pestaña a propósito, la próxima vez que abra esa sesión, quizás horas después, se envía un mensaje relayado viejo.
   - Si la sesión se borra, la entrada queda en `pending_prompts` para siempre, porque nada la limpia: los únicos accesos son `pty.rs:916` y `:1565`. Esa fuga ya existía para prompts encolados que nunca se lanzaron.
   - Es coherente con la decisión del usuario ("the prompt goes back to the queue"), así que solo pido una línea en Risks.

4. **Low (nota para el implementador, ya correcto por diseño):** el guard tiene que declararse antes que los lock guards. Así, en un `?` o en un panic, Rust suelta primero `session_ptys` e `instances` y después corre el `Drop` del rollback, que vuelve a tomar `session_ptys`. Con el orden de declaración que describe el plan (guard justo después de `reserve_session_pty`) esto se cumple. Si el `Drop` corriera con `session_ptys` todavía tomado, la app se quedaría trabada en un deadlock. Convendría dejarlo dicho en D3 para que un refactor futuro no lo invierta.

**Seguridad (sin cambios, sigue sólida):**
- El prompt nunca pasa por el parser ni por el line editor.
- `--` y `--prompt=` impiden que se lea como opción.
- El archivo es `0600`, se crea con `create_new` dentro de un dir `0700` verificado por owner, y el stem se valida en todas las rutas.
- El rollback es estructural, y Obsidian solo se finaliza en el caso bueno.

**Rollback del cambio:** un revert simple; solo queda un directorio vacío.

VERDICT: APPROVED
