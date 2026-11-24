**Claims verificados:** los anchors de implementation.md coinciden con el código (`types.rs:45-53/64-75`, `session.rs:547-556`, `differ.rs:61-80`, `terminalService.ts:87/441/1318/742/864-866`). En wezterm-term (`5046fc2`), `is_mouse_grabbed()` (`term/src/terminalstate/mod.rs:748-750`) es exactamente la condición con la que `mouse_button_press`/`mouse_button_release` deciden emitir (`mouse.rs:144`, `mouse.rs:195`). No depende de la pantalla. RIS la limpia (`performer.rs:681-687`) y DECSTR no (`mod.rs:1254+`). Como el snapshot lee el estado vivo, ambos casos salen bien. `openspec validate` pasa. La premisa del cambio es correcta.

1. **High: primary screen con scrollback desplazado manda coordenadas incorrectas.** El cambio habilita forwarding en primary screen, el único lugar donde existe `scrollOffset > 0`. `mouseToCell` (`terminalService.ts:1020-1028`) devuelve la fila del *viewport*. `TerminalSession::mouse_button` (`session.rs:438-446`) la pasa tal cual como `y` a wezterm, que la interpreta como fila de la pantalla viva. Escenario concreto: app con `?1000h` en primary, el usuario sube 30 líneas con la wheel (`pty.rs:2213-2226`, rama `scroll_by`) y hace right click sobre la fila 2. La app recibe un click en la fila 2 de la pantalla viva, que es otro contenido. wezterm GUI y xterm no reportan el mouse a la app mientras el viewport no está en el bottom. En alt screen esto no pasaba porque ahí no hay scrollback local. El requirement del spec delta ("SHALL forward … on the primary or the alternate screen") dejaría este comportamiento como contrato. Fix: la regla pasa a ser `mouseTracking && scrollOffset === 0`, tanto en `auxButtonToForward` como en la supresión del context menu. Hace falta un scenario "Scrolled-back primary screen does not forward" y un caso de vitest. `scrollOffset` ya llega en cada update (`terminalService.ts:1314`), así que no requiere cambio en el backend.

2. **Medium: el cambio de comportamiento en la sesión principal de Claude Code no se reconoce ni se recorre.** Según `terminalService.ts:143-145` ("CC pastes on release") y el doc de `terminal_mouse_button` (`pty.rs:2232-2234`), CC habilita mouse tracking. Si lo hace en primary screen, con este cambio cada right o middle click en cualquier pane de CC pasa a reenviarse y el context menu deja de abrirse. Es el caso más frecuente del producto, y ni Why/Impact ni los walks 3.3/3.4 lo mencionan (shell, less, htop, vim). Falta un paso de walk en una sesión CC con su renderer default: right y middle click producen lo esperado (paste on release, una sola vez) y sin context menu. También el mismo caso con scroll hacia arriba, ligado al punto 1.

3. **Low: `suppressContextMenu(mouseTracking)` es una función identidad y su test (2.1) es tautológico.** No protege nada. Conviene que encapsule la regla real del punto 1 (`tracking && scrollOffset === 0`) para que el helper y su test tengan sentido, o eliminarlo.

4. **Low: el wiring y la serialización no tienen test determinístico.** Tres puntos quedan cubiertos solo por el walk manual:
   - la asignación en `applyUpdate`,
   - el argumento de `auxButtonToForward` en `mousedown`,
   - la key JSON `mouseTracking`, que depende de `serde(rename_all = "camelCase")` en `GridUpdate`.

   Hay un patrón barato ya disponible: `emitter.rs` `attached_channel_receives_session_updates`. Se le alimenta `ESC[?1000h` y se afirma que el JSON del channel trae `"mouseTracking":true`. Eso cubre el emit sin redraw end-to-end y fija el nombre del campo.

5. **Low: los tests del differ (1.1) cubren un solo flip.** Faltan dos casos:
   - una secuencia `h → l → h` sin cambio de filas que emita tres veces. Esto verifica que `last_mouse_tracking` se actualiza en la rama de emisión junto a `last_cursor`/`last_title` (`differ.rs:64-66`), y no fuera.
   - el valor actual presente tras `invalidate()`, que es el camino de `terminal_attach` (`pty.rs:2392-2395`).

   La tarea también debería decir explícitamente dónde se actualiza el campo.

6. **Low: el synchronized output retrasa la propagación del modo.** El hold de DECSET 2026 (`emitter.rs:131-142`) demora el cambio de modo hasta `SYNC_OUTPUT_CAP`. Un right click en esa ventana muestra el context menu una vez. Es aceptable, pero "Per-phase risk" solo cubre el caso pre-snapshot. Conviene anotarlo junto a ese.

7. **Low: el walk 3.3 no se puede ejecutar tal como está escrito.**
   - Con `cat -v` en foreground, `printf '\e[?1000l'` no se puede correr sin salir antes con Ctrl+C. Entre medio, readline recibe los reportes como input basura.
   - El paso tiene que ordenarse así: activar, click, Ctrl+C, desactivar, click.
   - Agregar el caso scrolled-back del punto 1.

8. **Low: hay un cambio ajeno en el working tree.** `docs/shortcuts.md` está modificado en la branch (ruta del vault) y no pertenece a FRI-506. No debe entrar en el commit de este change.

Rollback: es reversible y aditivo. Un campo nuevo en `GridUpdate`, sin schema, config ni comandos IPC nuevos. Revertir el commit restaura el gating por `isAltScreen`. Las demás reglas de alt-screen (wheel `pty.rs:2213`, left drag, selection) quedan fuera de scope de forma coherente.

VERDICT: REVISE
