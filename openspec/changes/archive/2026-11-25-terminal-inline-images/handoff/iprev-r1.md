## Review: terminal-inline-images (FRI-611)

Revisé el plan contra el código en `8910e96`, el checkout de wezterm `5046fc2` y tauri `2.11.1`. La mayoría de los anclajes `path:line` de implementation.md son correctos: `types.rs`, `session.rs:488/515-537/341`, `differ.rs:39/101/107`, `pty.rs:921/2253-2285/2427-2456`, `emitter.rs:38/104`, `terminalService.ts:53/1316/1377/1385/1434/1457`, `terminalstate/image.rs:159-217` e `iterm.rs:104-122`. Las excepciones están en el punto 9.

1. **High: `terminal_image` bloquearía el main thread y el lock global de PTYs.** Lo que falla es la ejecución del plan, no la decisión de usar pull.
   - Design § Risks dice: "the copy runs on the command thread, not under the session mutex". Es falso para un `#[tauri::command] pub fn` sync como los de `pty.rs`: el propio repo dice en `pty.rs:111` que "Sync `#[tauri::command]`s run on the main thread".
   - El patrón de lookup de todos los comandos vecinos (`terminal_attach`, `terminal_scroll`) mantiene `state.instances.lock()` durante todo el cuerpo. Ese es el mapa de todos los terminales.
   - Consecuencia: copiar hasta ~100 MB (más el header y el armado del `Response`) congela la UI GTK y frena `terminal_input`, resize y scroll de todos los terminales mientras dura.
   - Implementation § Execution order paso 4 debe decir: `async fn` (o `#[tauri::command(async)]`), clonar el `Arc<Mutex<TerminalSession>>` y soltar `session_ptys` e `instances` antes de tocar la sesión, upgrade del `Weak` bajo el mutex de sesión, y la copia en `spawn_blocking`.

2. **High: la regla de fallos produce un loop de fetch y decode.**
   - El hash es de contenido (`ImageData::with_data` → `compute_hash`), así que un fallo de decode para un hash es determinista y permanente.
   - wezterm solo valida el header (`dimensions()` en `terminalstate/image.rs:299-310`). Un PNG truncado, o un formato que el crate `image` acepta y WebKitGTK no (TIFF, TGA, PNM…), entra como `EncodedFile` y falla en `createImageBitmap` cada vez.
   - Implementation paso 7 y tasks 3.5 llaman `resetFailures()` en cada grid update. El emitter emite hasta ~120 Hz mientras hay output (`emitter.rs:115`). Con Claude streameando y la imagen en pantalla, eso es refetch y redecode en cada tick.
   - Además, design D4 ("next grid update **that references it**") contradice a tasks/impl ("on the next grid update").
   - Corrección: los fallos de decode son permanentes por hash mientras viva el cache. Los errores de transporte se reintentan con límite o backoff.

3. **Medium (seguridad): bytes no confiables van a los decoders nativos de WebKit sin allowlist.**
   - Cualquier proceso que escriba al PTY (un `cat` de un archivo hostil, output remoto vía ssh) entrega bytes arbitrarios a libpng, libjpeg, libwebp o gif dentro del WebView. El precedente que importa es CVE-2023-4863 en libwebp.
   - Antes de este cambio esos bytes solo los tocaba el probe de header en Rust.
   - El plan no menciona esta superficie. Mínimo: sniff de magic bytes en el backend (PNG/JPEG/GIF/WebP) antes de devolver kind 1, error para el resto, y documentarlo en design § Risks. Esto además acota el punto 2.

4. **Medium: thrash del budget con imágenes visibles.**
   - Design § Risks cita "the failed-until-next-update rule stops a fetch loop" como mitigación. Esa regla no aplica a entradas *evicted*: task 3.2 exige explícitamente que un hash evicted se vuelva a pedir en el próximo `request`.
   - Con varias imágenes visibles que juntas pasan 256 MB (cada una puede llegar a 100 MB por el cap de wezterm), insertar una evicta otra dibujada en el mismo frame. El próximo update de esa fila la vuelve a pedir y evicta la primera. Es un ping-pong de fetch y decode de 100 MB por update en un TUI activo.
   - La decisión del user (256 MB, LRU) se respeta igual si no se evicta lo tocado en la generación de paint actual (overshoot temporal), o si se limita el refetch de un hash recién evicted. Agregar el caso a 3.2.

5. **Medium: race de dispose.**
   - Un fetch o decode en vuelo que resuelve después de `dispose()` inserta un bitmap en un cache muerto (leak, nunca `close()`) y dispara `ready` sobre una entry ya removida (`entries.delete` + `container.remove()`).
   - Falta un flag `disposed` que cierre los bitmaps tardíos sin notificar, más un test en 3.2.
   - Además hay dos sitios de destrucción, `destroy` (`terminalService.ts:516`) y la remoción de shell (`:532-537`). Tasks 3.5 dice "when the terminal is destroyed" sin nombrar ambos; el quake shell es justamente el que se olvida.

6. **Medium: los requisitos visibles del frontend no tienen cobertura de test.**
   - Como `GridRow.images` se omite cuando está vacío (`skip_serializing_if`), `applyUpdate` debe hacer `rowImages[i] = row.images ?? []`. Si no, la imagen sigue dibujada después de `ESC[2J`.
   - Tasks 3.5 solo dice "stores runs". Ese es el único lugar donde se cumple el scenario "Overwriting the cells removes the placement" del lado frontend.
   - También hay que reshapear `rowImages` junto con `reshapeGrid` (`:1317-1325`) y recortar runs a `cols`, como hace `normalizeRow`.
   - Nada de esto se testea, porque terminalService no tiene unit tests. Extraer `applyRowImages(rowImages, update, cols)` a `imagePlacement.ts` con test.

7. **Medium: test 2.1 no verifica el requisito "image no longer held".**
   - `image_cache` de wezterm es un `LruCache` de 16 por hash (`term/src/terminalstate/mod.rs:360,571`) y el output de texto no lo afecta. "ESC[2J plus enough output" nunca evicta.
   - La cláusula "if wezterm's LRU keeps it alive, assert the registry entry is still served — record which" deja el requisito del spec ("SHALL return an error for … an image no longer held") sin test, por diseño.
   - Corrección: después de `ESC[2J`, emitir 16 sixels distintos y verificar error. Alternativa: testear el registry con un `Arc` propio dropeado.

8. **Medium-low: falta el test de overwrite parcial, que es la razón de D1.**
   - D1 rechaza whole-image placements porque "an app can overwrite part of an image… per-row runs show exactly what remains". Task 1.1 solo testea el overwrite *total* de la fila 0.
   - Agregar: imprimir 2 caracteres en las columnas 2-3 de la fila 0, lo que da dos runs (col 0 span 2, col 4 span 3) con `u0/u1` contiguos correctos. Esto ejercita la regla de agrupación y la condición de corte.

9. **Low: facts incorrectos en implementation.md.**
   - "padding only on the last column/row" es falso para left/top. `assign_image_to_cells` pasa `cell_padding_left` y `cell_padding_top` a **todas** las celdas (`terminalstate/image.rs:196-207`); solo right y bottom dependen de la última. Para sixel e iTerm2 valen 0, así que hoy no rompe nada.
   - Pero el spec define padding como "left of the first cell… top", y eso es otra semántica que la de wezterm. Con offsets X/Y de kitty (FRI-613) probablemente haya que cambiar el formato, contra el goal "FRI-613 adds to it without changing it". Decidirlo ahora o anotarlo como riesgo.
   - Cita: `ImageData::hash()/data()` está en `termwiz/src/image.rs:499-570`, no en `:199-372` (ese rango es `ImageDataType`).

10. **Low: `GridSnapshot.images` como vector paralelo.**
    - Plan § Plan dice "update struct literals… with `images: vec![]`", pero un outer vec vacío con N rows hace panic en `snapshot.images[index]` dentro de `compute_update`.
    - O se especifica `.get(i)` con fallback vacío, o los runs van dentro de una struct de fila (estado inválido irrepresentable). Lo segundo es preferible.

11. **Low: semántica de columna.**
    - `cell_index()` es columna real, pero `rows[i]` se arma desde `visible_cells()`, que omite la celda de continuación de un wide char (`vecstorage.rs:92-100`). Encima `paintRow` salta `x+1` tras un wide char (`terminalService.ts:1427-1430`). Esa inconsistencia es preexistente.
    - En filas con wide chars antes de la imagen, texto e imagen quedan desalineados. Documentar en el tipo que `col` es columna de terminal, no índice en `cells`.

12. **Low: hex del hash.** Exigir `{:02x}` (64 chars). El `HexSlice` de wezterm (`termwiz/src/image.rs:505-513`) usa `{byte:x}` sin padding, así que no reutilizar su `Debug`.

13. **Low: padding en px fijados al momento del placement.** Tras un zoom de fuente o un cambio de DPR (nuevas `metrics` → nuevo `cellWidth`), `runRects` resta padding calculado con el tamaño de celda viejo. Aceptable, pero debe ir en Risks y en el walk 4.5.

14. **Low: inconsistencia en los gates.** Implementation § Verification usa `work-gates.sh main …` y tasks 4.2 usa `mufdidev/fri-609-git-panel-agent-actions`. Unificar según el estado del stack.

Rollback: el cambio es aditivo (sin config, schema ni deps), se revierte limpio. El transporte raw está bien en Linux con tauri 2.11.1: `ipc/protocol.rs:390-407` entrega `Raw` por channel, que llega como `ArrayBuffer`.

VERDICT: REVISE
