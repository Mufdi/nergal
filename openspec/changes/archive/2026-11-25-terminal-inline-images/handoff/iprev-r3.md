## Review ronda 3: terminal-inline-images (FRI-611)

Los cinco puntos de la ronda 2 quedaron resueltos donde importan: D4, implementation paso 7, tasks 3.2/3.5 y el spec. Verifiqué estos tres hechos:

- **Key de attach.** `terminal_attach` se invoca con `sessionId: key` (`terminalService.ts:476`) y la key de un quake shell es `${sessionId}::${shellId}` (`:532`). Los únicos sitios que borran entries son `:521` y `:537`, así que no queda una tercera ruta de destrucción.
- **iTerm2.** El fact corregido coincide con `iterm.rs:104-121`. Las fracciones `u/v` salen del tamaño reducido, pero como están normalizadas, `runRects` sobre el bitmap nativo da el resultado correcto. Este caso de PNG reducido no tiene test; el punto 4 lo agrega.
- **Lock order.** `terminal_attach` (`pty.rs:2432-2449`) toma `session_ptys`, lo suelta, toma `instances` y después el mutex de sesión. El comando nuevo sigue el mismo orden y suelta ambos mapas antes, así que no hay inversión de locks.

Las edits sí dejaron restos que contradicen el fix y un hueco nuevo:

1. **Medium: queda texto basado en "pass" que contradice D4 en tres lugares.** Un implementador que lea cualquiera de ellos reconstruye el defecto de la ronda 2.
   - Design § Risks, primer bullet: "the cache overshoots until the paint pass ends and evicts only images not drawn in that pass". Hay que reemplazarlo por la regla del visible set.
   - Implementation § Per-phase risk: "no eviction of hashes touched in the current pass". Mismo reemplazo.
   - Tasks 3.2, primer caso de budget: "C inserted in a later pass → A dropped". Debe decir que A y B no están en el visible set (`setVisible` sin A). Si no, el test entra en conflicto con el caso siguiente según qué visible set pase el implementador.

2. **Medium: el scenario normativo "Budget evicts the least recently drawn image" está subespecificado frente a "Referenced images are never evicted".**
   - El GIVEN no dice si A y B están referenciados. Si están visibles, la regla nueva dice que A no se evicta y el scenario dice lo contrario.
   - El spec es el que queda archivado como source of truth, así que el GIVEN tiene que decir "neither referenced by a current row".
   - En la misma línea, el requisito dice "a fetch or decode that completes afterwards SHALL release its bitmap". Un fetch todavía no tiene bitmap; debe alinearse con D4 ("a fetch skips the decode").

3. **Medium (recursos): el overshoot no tiene techo.**
   - wezterm limita cada imagen a 100 MB (`w*h*4`), no su forma. Una salida hostil puede mandar imágenes de 1 fila de alto y ~1,25 M px de ancho: cada una ocupa una sola fila, recortada a `cols`, y pesa 100 MB.
   - Con 24 filas visibles serían ~2,4 GB referenciados a la vez. La regla de "overshoot while the visible set exceeds the budget" los decodifica todos en el WebView.
   - El backend ya tiene esa exposición hoy, porque las celdas mantienen los `Arc`. El frontend la duplica y la lleva al proceso web, donde un OOM tira toda la UI.
   - Fix: un techo duro, por ejemplo 2× el budget. Por encima del techo no se decodifica una imagen nueva aunque esté referenciada (la marca como fallo de transporte, reintentable cuando baje el visible set) y sus celdas muestran texto. Agregar el caso a 3.2 y una línea en Risks.

4. **Low: tests de 2.1 que pueden pasar en falso.**
   - Los casos "BMP → error" y "registry entry dropped → error" devuelven error tanto por el allowlist como por "unknown hash". Si wezterm rechaza el BMP o el snapshot no llenó el registry, el test pasa igual.
   - Cada caso debe afirmar primero que la fila tiene un run con ese hash, y distinguir la variante de error (unsupported format, unknown, dropped) en vez de solo `is_err()`.
   - Agregar el PNG reducido (64x48 en una geometría menor, o con `width=`): sigue `EncodedFile`, kind 1, con bytes del tamaño nativo.

5. **Low: falta definir el orden LRU de una entry nunca dibujada.**
   - Si un decode resuelve cuando su fila ya no la referencia, la entry entra sin tick de draw.
   - D4 debe fijar que entra con el tick actual o como la primera candidata a eviction. Si no, dos implementaciones razonables dan resultados distintos con el cache lleno.

Rollback sin cambios: el cambio es aditivo y se revierte limpio.

Los cinco puntos son ediciones de texto o agregados de test chicos dentro del diseño ya revisado, sin cambio de arquitectura. Los apruebo con la condición de aplicar 1–3 antes del build.

VERDICT: APPROVED
