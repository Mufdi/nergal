**Review ronda 2: terminal-inline-images (FRI-611)**

Los 14 puntos de la ronda 1 están incorporados en el texto. El fix del punto 4 (thrash del budget) no funciona con el modelo de pintado real del frontend, y apareció un defecto nuevo en quake. Verifiqué lo siguiente:

- **ESC[2J no empuja al scrollback.** `erase_in_display` hace `clear_line` sobre `0..rows` (`term/src/terminalstate/mod.rs:2114-2128`), así que el test de los 16 sixels de 2.1 es válido.
- **El path `swap_out` deja `EncodedFile`.** Con `StorageNotInit` se conserva `EncodedFile` (`termwiz/src/image.rs:363-371`), y Nergal no inicializa `BlobManager`.
- **El comando async es viable.** `grid_snapshot(&mut self)` (`session.rs:488`) permite llenar el registry sin interior mutability, y los guards de `instances`/`session_ptys` son `std::sync::Mutex` (`pty.rs:119,123`), así que soltarlos antes del `await` es obligatorio y el plan ya lo hace.

1. **Medium (bloqueante, el fix de la ronda 1 #4 no sirve): la protección "touched in the current paint pass" no representa lo que está visible.**
   - `applyUpdate` pinta solo `touchedRows` (`terminalService.ts:1329-1360`). Con Claude streameando, la mayoría de los passes son parciales y no incluyen las filas de imagen.
   - La inserción en el cache ocurre cuando resuelve el decode, que es async y cae fuera del pass que pidió la imagen. Para ese momento, el "current pass" es cualquier update parcial posterior.
   - Escenario: A y B de 150 MB visibles, y el decode de B resuelve durante un pass parcial que repinta solo la fila del prompt. A no fue tocado en ese pass y se evicta aunque esté en pantalla. Cuando la fila de A se vuelve a pintar, A falta: la fila se pinta como solo texto (flicker) y se re-fetchea. Su inserción async evicta a B por el mismo mecanismo. El ping-pong de la ronda 1 sigue.
   - Lo mismo pasa con cualquier inserción posterior, aunque sea chica. El overshoot "hasta que termina el pass" se convierte en la eviction de imágenes visibles en la próxima inserción.
   - El test de 3.2 ("both are kept until the next pass") y el scenario del spec ("WHEN the second is inserted… during that pass") codifican el defecto. Además, tasks 3.5 no define qué es un pass: el paint parcial de `applyUpdate`, `paintAll` y el repaint de `ready` quedan ambiguos.
   - Corrección: proteger el conjunto de hashes referenciados por `rowImages` de las filas actuales (el visible set que mantiene `applyRowImages`), no un tick de pass. La eviction solo puede tocar hashes no referenciados, con overshoot mientras el visible set supere 256 MB. Test: dos imágenes de 150 MB referenciadas por `rowImages`, inserción async de una tercera de 10 MB durante un update parcial que no toca esas filas. Ninguna de las dos visibles se evicta; la de 10 MB se inserta igual porque está referenciada, y el cache queda en overshoot hasta que una deja de estar referenciada, momento en que se evicta la menos recientemente dibujada.

2. **Medium-low (nuevo): id de sesión del fetch en quake.**
   - El frontend hace attach con `sessionId: key` (`terminalService.ts:476`), donde para un shell la key es `${sessionId}::${shellId}` (`:532`). La entry además guarda `ownerSessionId`.
   - Implementation § Plan dice `invoke("terminal_image", { sessionId, hash })` sin aclarar cuál de los dos ids.
   - Si se pasa `ownerSessionId`, el backend busca en el registry del terminal principal: "unknown hash" en cada intento. Con la regla nueva de reintentos, la imagen queda como fallo permanente en el quake shell. Walk 4.6 lo detectaría, pero tarde.
   - Fijar en el plan que el fetch usa la misma key que `terminal_attach`, y que `imageCache` se construye con esa key.

3. **Low: facts incompletos del path iTerm2, que afectan al test 2.1.**
   - `iterm.rs:104-121`: si la imagen se achica y el formato no es GIF/PNG/WebP (JPEG, BMP…), wezterm la decodifica en Rust con `load_from_memory` y la guarda como `Rgba8` (kind 0). Solo queda `EncodedFile` si no hay downscale, o si es GIF/PNG/WebP.
   - El fact de implementation.md ("iTerm2 PNG/GIF/WebP → EncodedFile") omite esto.
   - Consecuencia para el test "BMP via OSC 1337 → error": el BMP tiene que caber en 800x480 sin escalar (y sin `width=`/`height=` que lo achiquen). Si no, se sirve como kind 0 y el test falla por la razón equivocada.
   - Un WebP chico llega como `EncodedFile` y da error por el allowlist. Uno que se achica también queda `EncodedFile` y da error. Está bien, pero conviene anotarlo en D3.

4. **Low: orientación EXIF en JPEG.**
   - wezterm calcula el span de celdas con las dimensiones crudas (`dimensions()`, `image.rs:299-310`).
   - `createImageBitmap` aplica la orientación EXIF por defecto (`imageOrientation: "from-image"`). Un JPEG rotado termina con ancho y alto intercambiados respecto de las fracciones `u/v`, y la imagen se dibuja estirada.
   - Pasar `{ imageOrientation: "none" }` en el decode de kind 1, o documentar el caso.

5. **Low: el orden de dispose.**
   - D4 dice que un fetch que resuelve después de `dispose()` "closes its bitmap". Si lo que resuelve es el fetch, todavía no hay bitmap: el código debe chequear `disposed` y saltarse el decode, no decodificar hasta 100 MB para después cerrarlo.
   - Precisarlo en D4 y en el test de 3.2: fake de fetch que resuelve post-dispose y assert de que `decode` no se llamó.

Rollback sin cambios: el cambio es aditivo y se revierte limpio.

VERDICT: REVISE
