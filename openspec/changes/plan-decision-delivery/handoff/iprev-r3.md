**Ronda 3.** Los issues de la ronda 2 quedaron resueltos y los fixes no meten regresiones de severidad media o mayor. Quedan solo puntos low, que se pueden cerrar durante la implementación sin otra ronda.

Revisé contra el código: `server.rs:1237-1285` y `:761-808`, `cli.rs:215-300` y `:348-420`, `commands/plans.rs:113-171`, `platform/mod.rs:69` y `:329-339`, `PlanPanel.tsx:25-127`, `plan.ts:30-75`.

**Cómo quedaron los issues de la ronda 2:**
- **#1 (submits concurrentes):** resuelto. `remove_if` toma el lock del shard, así que el claim es atómico. Además, `decision_path` sale literal de `fifo_path` (`server.rs:1282`) y se registra como `PathBuf::from(fifo_path)` (`:1250`), así que la comparación de igualdad es confiable. Con `e.repeat` y el ref de envío en curso queda cubierto también del lado del panel.
- **#2 (comparar contra `content`):** resuelto. `original` existe en `PlanState` (`plan.ts:43-51`) y `setPlanContentAtom` solo toca `content`.
- **#3 (deadline sin test):** resuelto. Hay scenario y test, y Risks documenta el write parcial. Confirmé que `cli.rs:411` ignora `POLLHUP` sin `POLLIN`, así que lo que dice Risks es exacto.
- **#4 a #7:** resueltos.

## Issues

1. **low: falta decir qué partes de D2 quedan solo para Unix.**
   - `platform::ipc_dir()` existe solo bajo `cfg(unix)` (`platform/mod.rs:68-69`). En Windows, `plan_review_fifo_path` devuelve un nombre de pipe sin parent útil (`:336-338`).
   - Si el check de parent o el claim se implementan sin gate, `windows-check` falla en el PR, y eso se detecta.
   - El riesgo real es otro: gatear el registro para Unix y dejar el claim en todas las plataformas. Ahí Windows rechaza todos los approves sin que ningún gate de CI lo note.
   - **Fix:** que implementation.md diga explícitamente:
     - el check de parent va bajo `cfg(unix)` y en Windows el registro queda como hoy;
     - el claim va solo en la rama Unix (el spec ya dice "On Unix");
     - la rama Windows del comando no cambia (se puede envolver en `spawn_blocking`, pero sin claim).

2. **low: un error de `save_plan_edits_if_dirty` muestra el toast "Claude Code is not waiting" aunque CC sí está esperando.**
   - En el comando, el guardado de ediciones va antes del claim (`plans.rs:122-125`). Si falla, devuelve `Err`, el registro queda intacto y el hook sigue vivo.
   - D3 trata todo error igual: pone `idle` y muestra un texto que en este caso es falso.
   - No se pierde nada, porque el terminal sigue disponible. Pero es el único error que sí es transitorio, y eso contradice la premisa de D3 ("none of these errors is transient").
   - **Fix:** que el toast muestre el error del backend en la descripción, o que este caso mantenga `pending_review`. Basta una línea en D3.

3. **low: un `PlanReview` con el FIFO fuera de `ipc_dir()` igual emite `decision_path`.**
   - El plan gatea el registro, pero `plan:ready` sigue saliendo con `decision_path: fifo_path` (`server.rs:1282`).
   - El panel abre la review y el Approve falla en el claim con el toast. Es visible, no silencioso, pero muestra botones que no pueden funcionar.
   - **Fix:** emitir `decision_path` vacío cuando el registro se rechaza (por D1 queda en `idle`) y loguear el rechazo con `warn!`.

4. **low: tres scenarios de UI no tienen ni test ni walk.**
   - Vitest corre sin DOM, así que "A held key sends one decision", "A failed Revise keeps the annotations" y "Approve without a decision path fails visibly" no tienen test automatizado.
   - De los walks, el 3.5 cubre a medias el tercero.
   - **Fix, una de dos:**
     - extraer la decisión del handler (repeat, en curso, path vacío → acción) a una función pura testeable en vitest;
     - agregar walks: mantener `A` presionado y ver un solo approve en el log del hook; Revise con el FIFO ya muerto (responder en el terminal primero) y comprobar que las anotaciones siguen.

5. **low: la ventana entre el envío de `PlanReview` y el `open(O_RDONLY)` del hook ahora es terminal.**
   - En `cli.rs:259-283`, el hook manda el evento, después lee `gui.pid` y recién ahí abre el FIFO.
   - Un write con `O_NONBLOCK` en esa ventana da `ENXIO`. Como el claim ya consumió el registro, no hay reintento y el hook espera 30 min.
   - Hoy `std::fs::write` bloquea hasta que el lector abre, así que esto es nuevo. La ventana es de microsegundos contra latencia humana, así que no es explotable en la práctica.
   - **Fix:** documentarlo en Risks. Opcional: reintentar `ENXIO` dentro del mismo deadline de 5 s antes de dar error. No debilita nada, porque el claim ya garantiza un solo escritor.

6. **low, preexistente y fuera de scope:** `submit_plan_decision` llama a `get_or_create(&session_id)` con el id de Nergal (`plans.rs:123`), mientras el server guarda el plan con el `session_id` de CC (`server.rs:1259`, `:1266`). D1 conserva bien las ediciones en el frontend, pero que lleguen al disco depende de otro seam que este change no toca. No bloquea: conviene un issue aparte en Linear si en el walk las ediciones no se reinyectan.

7. **Rollback:** sigue siendo reversible. No hay cambios de schema, config ni protocolo, y `cli.rs` queda intacto. Sin issue.

VERDICT: APPROVED
