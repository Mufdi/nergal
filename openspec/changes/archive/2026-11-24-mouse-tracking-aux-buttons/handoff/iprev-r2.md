Re-review de la ronda 2. Verifiqué contra el código el punto que podía introducir una regresión nueva: el `scrollOffset` stale al entrar a alternate screen.

**Issues previos:**
- **#1 (high): resuelto.** `reportsMouse(mouseTracking, scrollOffset)` gobierna el forwarding y la supresión del context menu. El requirement nombra "scroll offset 0" y hay un scenario y un caso de vitest para offset 30. Revisé el riesgo de que alguien entre a alt screen con scroll hacia arriba y el offset viejo bloquee los botones en vim/htop. No ocurre: `snapshot()` clampa `scroll_offset` a `max_scroll_offset()` (`session.rs:493-494`), y en alt screen ese máximo es 0. En primary no se produce wheel forwarding (`pty.rs:2213-2226`), así que el offset solo sube con scroll local, que es justo el caso que ahora se excluye.
- **#2 (medium): resuelto.** El proposal describe el cambio de comportamiento en Claude Code y el walk 3.5 cubre right/middle click y el caso scrolled-back.
- **#3: resuelto.** El helper identidad desaparece y `reportsMouse` lleva la regla real.
- **#4: resuelto en lo esencial.** El test de emitter fija `"mouseTracking":true` end-to-end. El patrón existe en `src-tauri/src/terminal/emitter.rs:182`.
- **#5: resuelto.** Hay tests de `h → l → h` y de `invalidate()`, y el plan dice dónde se actualiza `last_mouse_tracking`.
- **#6, #7, #8: resueltos.**

**Observaciones nuevas, ninguna bloqueante:**

1. **Low: el wiring sigue sin test automático.** Las dos llamadas a `reportsMouse(entry.mouseTracking, entry.scrollOffset)` (el `mousedown` en `terminalService.ts:742` y el `contextmenu` en `:864-866`) y la asignación en `applyUpdate` quedan cubiertas solo por los walks 3.3 a 3.5. Como la regla vive en un helper puro y los walks recorren los tres estados (tracking on, off y scrolled-back), lo acepto para tier LOW.

2. **Low: el walk 3.5 da por hecho que el middle click en Claude Code "behaves the same" (paste on release).** La única fuente es el comentario de `terminalService.ts:143-145`, que habla del right click. Si Claude Code no hace nada con el middle, el walk fallaría por una expectativa mal escrita y no por un bug. Conviene redactarlo como "a middle click reaches Claude Code and no context menu opens", o dejar el resultado que se observe.

3. **Low: botón sostenido mientras cambia el offset.** Si el usuario presiona right con offset 0 y hace wheel mientras lo sostiene, la vista se desplaza a history. Igual se envía el release en la celda del press, porque `auxRelease` depende de `entry.auxHeld` (`terminalService.ts:149`) y no del modo ni del offset. Es correcto: wezterm recibe un par press/release balanceado. Basta una línea en "Per-phase risk" junto al caso de modo apagado.

4. **Low: el teclado también queda afectado.** La supresión del `contextmenu` alcanza el menú abierto con teclado (Menu key o Shift+F10) mientras hay tracking. Antes pasaba lo mismo en alt screen, así que el comportamiento sigue igual. No requiere acción.

**Rollback:** sigue siendo aditivo y reversible. Es un campo nuevo en `GridUpdate`, sin schema, config ni comandos IPC nuevos.

VERDICT: APPROVED
