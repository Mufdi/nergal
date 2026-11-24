## Revisión ronda 3: statusline-pr-signal (FRI-473)

**Estado de la ronda 2:** #2 (orden de respuestas), #4 (`PR_INFO_TTL_MS` único) y #5 (texto de la spec) quedan resueltos. La tarea 1.2 ahora nombra el arm legacy `StatusLine`. #1 y #3 no quedan resueltos, y el fallback trae una regresión nueva.

1. **medium: el path de fallo de D6 nunca se ejecuta para un error de red. El pill se sigue ocultando hasta 60 s.**
   - **Qué dice el plan:** D6 conserva `data` y limpia `signal` cuando `invoke` rechaza, y el scenario "A failed fetch keeps the pill and retries" pone como ejemplo un error de red.
   - **Qué hace el código:** `pr_status` (`src-tauri/src/worktree.rs:1152-1162`) convierte en `Ok(None)` cualquier salida no exitosa de `gh`: red caída, rate limit, auth expirada o "no PR". Que `gh` no se pueda lanzar también da `Ok(None)`. `get_pr_status_blocking` solo devuelve `Err` cuando no encuentra la sesión o falla la DB (`src-tauri/src/commands/ship_pr.rs:207-212, 226`).
   - **Consecuencia:** con PR 12 abierta, un error transitorio de `gh` resuelve con `null`. La acción escribe `{ data: null, signal: "pr:12" }` y `shouldRefreshPr` da false con cada update de CC. Es exactamente el caso #1 de la ronda 2.
   - **Por qué los tests no lo ven:** el test de store de 2.1 mockea un `invoke` rechazado, así que pasa sin cubrir el caso real.
   - **Fix (elegir uno):**
     - En el backend, distinguir "no pull requests found" (stderr de `gh`) de los demás fallos y devolver `Err` en esos. Afecta también a `mcp/tracker.rs:336`, que reusa la función, y hay que declararlo en Impact.
     - En el frontend, no grabar la firma cuando la respuesta la contradice (firma `pr:N` y `data` null o con otro número) y conservar el `data` previo.
   - **Test:** cubrir el caso con un `invoke` que resuelve `null` mientras la firma es `pr:12`, no solo con uno que rechaza.

2. **medium: el fallback real es de hasta ~120 s, no de 60 s.**
   - **El problema:** el intervalo de 60 s corre `shouldRefreshPr`, que solo refresca si la entrada tiene más de 60 s.
   - **Ejemplo:** un fetch por señal en t=10 s deja la entrada con 50 s de edad en el tick de t=60. El tick no refresca y el siguiente fetch ocurre en t=120, 110 s después del último.
   - **Qué contradice:**
     - el scenario "Fallback poll" ("WHEN 60 seconds pass since the last fetch THEN … SHALL call");
     - el walk 3.5 ("within about 60 s");
     - la decisión cerrada de un fallback de 60 s igual al de `list_prs`, que hoy refresca en cada tick sin gate de edad.
   - **Retry tras un fallo:** como el effect de firma no vuelve a correr, el retry también depende de este intervalo y hereda el mismo peor caso.
   - **Fix (elegir uno):**
     - que el intervalo de 60 s refresque siempre, sin gate de edad;
     - un intervalo de chequeo más corto (por ejemplo 10 s) contra la edad de 60 s.
   - **Test:** agregar uno con fake timers que fije el peor caso.

3. **low: la mitad merge de D3 no tiene ningún path que la ejecute, y el walk 3.4 no se puede observar.**
   - **Por qué no se ejecuta:** `cleanup_merged_session` solo devuelve `deleted: true` (`src-tauri/src/commands/workspaces_sessions.rs:938`, el único `Ok`). La rama "`deleted: false`" de la tarea 2.3 es código muerto. Sin `owningSessionId` no hay sesión a la que pasarle `refreshPrInfoAction`.
   - **El único caso en que la sesión sobrevive** es el `.catch` del cleanup (`src/components/git/PrViewer.tsx:539-545`), y el plan no refresca ahí. En ese caso el pill queda stale hasta el fallback, con CC idle.
   - **El walk:** 3.4 pide "Merge it from the PR viewer; the pill changes right away", pero en el flujo normal la sesión se borra y no queda ningún pill que mirar.
   - **Fix:**
     - refrescar en el `.catch` del cleanup;
     - sacar la rama `deleted: false` de D6 y de la tarea 2.3;
     - reescribir el walk 3.4: el merge con cleanup exitoso cierra la sesión, y con el cleanup fallido el pill pasa a MERGED enseguida.

4. **low: `pr_status` devuelve PRs cerradas o mergeadas, y la spec no lo dice.** `gh pr view <branch>` encuentra PRs en cualquier estado, así que después del merge el refresh devuelve `state: MERGED`, no `null`.
   - **Qué está bien:** la regla de firma funciona igual, porque `none` difiere de `pr:12` y dispara un fetch.
   - **Qué falta:**
     - El scenario "The pull request merges or closes" debería decir qué espera ver: PR con estado MERGED o CLOSED.
     - El fix del punto 1 por el lado frontend no puede tratar "firma `none` con `data` presente" como una contradicción.

**Rollback:** sin cambios. Es aditivo (`serde(default)`) y vuelve atrás con una línea en `GitPanel.tsx:194-198`. Si se adopta el fix backend del punto 1, ese cambio también hay que revertirlo, junto con su efecto en la tool MCP `get_pr_status`.

VERDICT: REVISE
