## Revisión ronda 2: statusline-pr-signal (FRI-473)

**Estado de los hallazgos de la ronda 1.** Los diez quedaron resueltos. Lo verifiqué contra el código:

- **#1 (Ship/merge refrescan):** D3 lo resuelve con una acción compartida. Coincide con `ShipDialog.tsx:301-314` y `PrViewer.tsx:520`.
- **#2 (nombre del campo):** el campo ahora es `github_pr`, en `workspace.ts:196-224`.
- **#3 (literales de `AgentStatus`):** Impact lista todos los sitios. El arm legacy `StatusLine` (`server.rs:1381-1416`) también construye un `AgentStatusEmit` y queda cubierto por "every other literal". Igual conviene nombrarlo en la tarea 1.2.
- **#4 (baseline y remount):** D1 reemplaza el baseline y el nonce por una regla pura, `shouldRefreshPr(entry, signal, now)`. Con ella, el reinicio con CC idle y la PR que cambia mientras el panel muestra otra sesión quedan cubiertos por la regla y por el scenario.
- **#5 (log del walk):** D4 deduplica el log en el backend. Lo limpia junto a `model_cache` (`server.rs:700`), con la misma key `csid`.
- **#6 (cadencia de CI):** D5 queda declarado en el proposal y en el walk 3.5.
- **#7 a #10:** todos resueltos.

Los problemas que quedan vienen de que el ciclo de autocorrección pasó de 5 s a 60 s. Antes, cualquier escritura mala en `prInfoMap` se corregía sola en 5 s. Ahora dura hasta 60 s, y el plan no protege la acción compartida contra eso.

1. **medium: un error transitorio de `gh` oculta el pill durante hasta 60 s.**
   - **Qué pasa hoy:** el `.catch` de `refreshPr` (`GitPanel.tsx:141-143`) escribe `{ data: null }`. Según D1, `refreshPrInfoAction` grabaría además `signal = "pr:12"`.
   - **Qué causa:** si hay un error de red o un rate limit con la PR 12 abierta, la entrada queda en `data: null` con `signal: "pr:12"`. `shouldRefreshPr` devuelve false con cada update de CC, porque la firma no cambia. El pill y el poll de CI desaparecen hasta que pasan 60 s. Hoy eso dura 5 s.
   - **Fix:** en el path de error, conservar el `data` anterior y no grabar la firma (o grabar `signal: undefined` con el `fetchedAt` anterior), para que el siguiente update reintente. Sumar un caso de vitest: `invoke` rechaza, la entrada mantiene el `data` previo y `shouldRefreshPr` vuelve a dar true con la misma firma.

2. **medium: la acción compartida acepta respuestas fuera de orden.**
   - **El problema:** D3 hace que tres orígenes llamen a la misma `refreshPrInfoAction`: el effect de cambio de firma, el intervalo de 60 s y el éxito de Ship o merge. La acción no tiene guardia de secuencia.
   - **Escenario concreto:** el fallback de 60 s lanza `gh pr view` justo antes de que termine `git_ship`, y esa respuesta sale sin PR. Ship lanza su refresh y obtiene la PR 12. Si la primera respuesta llega última, pisa a la segunda y deja `data: null`. Como la firma de CC sigue en `none` porque CC está idle, nada vuelve a refrescar hasta 60 s después. Es exactamente el flujo que D3 quería arreglar.
   - **Fix:** que `refreshPrInfoAction` guarde `startedAt` (o un contador por sesión) y descarte la respuesta si la entrada ya tiene un fetch iniciado después. Agregar un test con dos `invoke` resueltos en orden inverso.

3. **low: el refresh después del merge compite con `cleanup_merged_session`.**
   - **El problema:** `performMerge` (`PrViewer.tsx:527-538`) borra la sesión dueña justo después del merge. Si `refreshPrInfoAction(owningSessionId)` corre en paralelo, puede pasar esto:
     - Si escribe después de `pruneSessionStateAction` (`sessionScope.ts:30-39`), revive una entrada zombie en `prInfoMap` para una sesión que ya no existe.
     - Si `get_pr_status` falla porque la sesión ya no existe, escribe `null` en esa entrada.
   - **Fix:** refrescar solo cuando la sesión sobrevive (sin `owningSessionId`, o si el cleanup falla o no borra). Otra opción es que la acción no escriba si la sesión ya no está en el workspace.

4. **low: el plan no dice qué pasa con `PR_INFO_TTL_MS`.** `git.ts:188` fija 30 s y el gate de mount actual lo usa (`GitPanel.tsx:181`). La spec delta pide 60 s. Hay que decidir explícitamente si la constante pasa a 60 s y la comparten `shouldRefreshPr` y el intervalo, o si se elimina. Si queda como está, aparecen dos TTL distintos para la misma entrada.

5. **low: la spec y la implementación no describen el mismo disparador.**
   - **Qué dice cada una:** la spec dice que el chequeo corre "on each status update for the session". Implementation §3 lo engancha a un effect sobre `[sessionId, current signal]`, que solo corre cuando la firma cambia.
   - **Por qué importa:** el resultado es el mismo, porque si la firma no cambia la regla devuelve false salvo por edad, y la edad ya la cubre el intervalo. Pero un test escrito contra la spec esperaría llamadas por cada update.
   - **Fix:** alinear el texto de la spec con "on signature change".

**Rollback:** sin cambios respecto a la ronda 1. Es aditivo (`serde(default)`) y se revierte en una línea en GitPanel.

VERDICT: REVISE
