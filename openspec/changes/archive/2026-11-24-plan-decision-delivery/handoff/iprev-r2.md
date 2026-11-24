Revisé el plan contra el código: `server.rs:1099-1135`, `:1237-1285`, `:761-808`, `adapter.rs:44-75`, `:280-299`, `commands/plans.rs:113-147`, `hooks/cli.rs:348-450`, `stores/hooks.ts:276-300`, `stores/plan.ts:43-110`, `PlanPanel.tsx`, `RightPanel.tsx:219-221`.

**Cómo quedaron los issues de la ronda 1:**
- #1, #2, #4, #7, #8, #9 y #10 quedaron bien resueltos.
- #6 queda descartado: `RightPanel.tsx:220` renderiza un solo `DocumentContent`.
- #5 queda cubierto con la regla de contenido. El caso residual quedó bien documentado en Risks.
- #3 se resolvió con `async` + `spawn_blocking` + `poll(POLLOUT)`, pero ese cambio abre una regresión nueva (ver 1).

## Issues

1. **medium: el comando `async` permite entregas concurrentes al mismo FIFO, y el lector del hook no se recupera de dos payloads seguidos.**
   - **Antes:** el comando síncrono corría en el main thread, así que dos submits iban en serie.
   - **Ahora:** D2 hace check del registry, después `spawn_blocking`, y solo después "forget on success". Entre el check y el forget pasan el IPC y la escritura.
   - **Cómo se dispara:**
     - El handler de A no filtra `e.repeat` (`PlanPanel.tsx:176-193`), y `canAnnotate` sigue en `true` hasta que resuelve la promesa.
     - Con autorepeat de A, o con click más A, salen dos invokes y los dos pasan el check.
   - **Qué pasa en el lector:**
     - Si las dos escrituras (cada una atómica, < PIPE_BUF) llegan antes de que el lector despierte, el buffer queda `{"approved":true}{"approved":true}`. `from_slice` falla por trailing characters.
     - Con los writers cerrados y el pipe vacío, Linux devuelve `POLLHUP` sin `POLLIN`. `cli.rs:411` lo trata como tick, así que nunca llega al `read` que devolvería EOF.
     - Resultado: el hook espera hasta el timeout de 30 min mientras la UI mostró éxito. Es justo el síntoma de FRI-604.
   - **Variante más probable:** el primer write llega bien, el lector cierra, y el segundo da `EPIPE` o falla el registry check. El usuario ve el toast de error "Claude Code is not waiting" después de un approve que sí funcionó.
   - **Fix:**
     - Reclamar la entrada antes de escribir con un remove condicional (`DashMap::remove_if(sid, |_, p| p == requested)`). Solo el que la reclama escribe. Coherente con D3 ("ningún error es transitorio"), no se reinserta.
     - En el panel: ignorar `e.repeat` y agregar un guard de envío en curso (por ejemplo, poner `submitted` antes del invoke y revertir a `idle` si falla).
     - Agregar un scenario: dos submits concurrentes producen un solo write; el segundo da error y no escribe nada.

2. **medium: D1 compara contra `content`, que el usuario puede editar.**
   - `PlanState` tiene `content` (editable vía `setPlanContentAtom`, `plan.ts:65-72`) y `original` (lo recibido).
   - Si el usuario edita antes de que llegue el preview tardío, la regla lo toma como "new content": borra el decision path y pone `idle` en medio de una review viva.
   - La ventana es corta, porque el preview async llega en ms o cientos de ms. Pero es el tipo de fallo silencioso que este change quiere eliminar, y depende de la latencia de `nergal-conditional.sh`.
   - **Fix:** comparar el payload contra `prevPlan.original`. En el caso same-cycle, conservar también `content` y `mode` del estado previo, para no pisar ediciones. Ajustar spec, scenario y task 2.1.

3. **medium: el límite de 5 s es un SHALL sin test, y un write truncado deja el hook colgado.**
   - La task 1.1 no prueba el deadline ni `EPIPE`, y el helper ya recibe `deadline: Duration`, así que es barato.
   - **Test a agregar:** reader abierto que nunca lee, payload de 100000 bytes y deadline de 200 ms, con `Err` dentro del deadline más un margen.
   - **Efecto de un deadline vencido:** queda JSON parcial en el pipe. Por lo descrito en 1, el lector no detecta EOF y espera 30 min.
   - **Qué ajustar:**
     - D2 debe tratar el deadline como "dead": hacer forget y mostrar el toast.
     - Risks debe decir explícitamente que, tras un write parcial, el hook espera hasta que el usuario responda en el terminal o hasta el timeout. El toast ya apunta ahí, así que basta con documentarlo.

4. **low: el `submit_plan_decision` del adapter es `async fn` y usaría el helper bloqueante.**
   - El plan dice "uses the same helper" sin `spawn_blocking`, así que bloquearía un worker de tokio hasta 5 s.
   - Hay que envolverlo igual que el comando, o dejar explícito que hoy solo lo llaman los tests (comentario en `server.rs:1243-1246`).

5. **low: hace falta una nota de implementación sobre el guard del `Mutex`.**
   - En el comando `async`, el guard de `SharedPlanState` (`std::sync::Mutex`) tiene que soltarse antes del `.await`. Si no, el future no es `Send` y Tauri no compila el comando.
   - Agregarlo a "Per-phase risk".

6. **low: `PlanReview` sin plan resuelto sigue siendo silencioso.**
   - Al quitar el fallback, si `resolve_active_plan` devuelve `None`, el registry queda registrado (`server.rs:1247-1251`) y no se emite nada. El hook bloquea 30 min sin señal en el panel.
   - Es raro, pero conviene un `tracing::warn!` en ese branch y una línea en Non-Goals o Risks.

7. **low: seguridad residual.**
   - `O_NOFOLLOW` cubre solo el último componente del path.
   - El registry acepta cualquier `fifo_path` que mande un proceso del mismo uid por el socket.
   - Con el check del registry, más `fstat` FIFO y sin `O_CREAT`, el gate no se debilita respecto de hoy; lo mejora.
   - Opcional: validar en el handler de `PlanReview` que `fifo_path` tenga como parent `ipc_dir()` antes de registrarlo.

8. **low: rollback.** Sigue siendo reversible: sin cambios de schema, config ni protocolo, y `cli.rs` queda intacto. Sin issue.

VERDICT: REVISE
