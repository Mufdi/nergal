# Revisión frontend: kitty-graphics-canvas (FRI-613)

Alcance: `git diff 23470f5..HEAD -- src`. Sin cambios de código. Hallazgos verificados contra el código.

## Resumen

Orden de pintado correcto en las tres capas. Sixel e iTerm2 no cambian. Reverse video y celdas anchas siguen equivalentes al código anterior. Hay 4 Nits y ningún Critical ni Important.

## Verificación sin hallazgos

- Orden por fila en `src/components/terminal/terminalService.ts:1445-1491`: limpieza de fila, `underBackground`, fondos, `underText`, glifos y subrayado, `overText`. Coincide con el spec.
- Reverse video: el fondo usa `cell.fg` con fallback `TERM_THEME.foreground` (`:1452`). El primer plano usa `cell.bg` con fallback `TERM_THEME.background` (`:1469`). El resultado es idéntico al swap anterior.
- Celdas anchas: ambos bucles avanzan `x += 1` cuando `span === 2` (`:1455`, `:1488`). El fondo cubre `span` celdas y el placeholder no pisa el glifo.
- Cursor y selección: se pintan después de `paintRow` en `paintAll` y `repaintImageRows` (`:1429-1430`, `:1530-1531`). Una imagen `overText` queda bajo el cursor, igual que antes.
- Sixel e iTerm2 con `z = 0` caen en `overText` (`src/components/terminal/imageLayers.ts:16`). El filtro anterior `z >= 0` los dibujaba tras los glifos. No hay cambio visible.
- Los límites coinciden con el spec: `z < -1073741824` va bajo los fondos, `[-1073741824, 0)` bajo el texto (`imageLayers.ts:14-16`).
- El orden es estable y ascendente por z. `Array.prototype.sort` es estable, así que z igual conserva el orden de fila (`imageLayers.ts:18-20`). Los tests lo cubren (`imageLayers.test.ts`).
- Costo de `rgbaToCss`: el número de llamadas por celda no cambia. Antes se llamaba una vez para fg y otra para bg. Ahora cada bucle llama una vez.
- Listener: `terminal:kitty-capped` se agrega a `unlisteners` (`src/stores/hooks.ts:449-455`). `setupHookListeners` devuelve ese arreglo (`:523`), así que se desregistra con los demás.
- Toast: usa `toastsAtom` con `type: "info"` (`hooks.ts:453`). La duración es el valor por defecto de sileo, igual que los demás toasts. El texto va en español neutro, sin voseo ("Ábrela de nuevo").
- Reset del set: `pty_id` incluye milisegundos (`src-tauri/src/pty.rs:1154-1160`, `:1586-1592`). Reabrir la terminal crea una clave nueva y vuelve a avisar. El backend emite el aviso una sola vez por terminal (`take_capped_notice`).

## Hallazgos

1. [Nit] `src/components/terminal/imageLayers.ts:11-22`, `terminalService.ts:1445`
   - Escenario: `paintRow` corre en cada repintado de fila, y casi todas las filas no tienen imágenes. Cada llamada crea un objeto, tres arreglos y tres `sort` vacíos.
   - Impacto: pequeño frente a las operaciones de canvas. Es presión de GC evitable en un camino caliente.
   - Fix: devolver una constante congelada compartida cuando `runs.length === 0`. Como `drawImageRuns` ya sale con `runs.length === 0`, la constante no se muta.

2. [Nit] `src/stores/kittyCapped.ts:12-15`
   - Escenario: la rama de duplicado copia el set entero (`new Set(prev)`) para devolver `next` sin cambios.
   - Impacto: copia innecesaria. El tamaño es mínimo, pero la copia no aporta nada.
   - Fix: devolver `prev` en esa rama. Ajustar el tipo de `next` a `ReadonlySet<string>`.

3. [Nit] `src/stores/hooks.ts:448`
   - Escenario: `kittyNotified` solo crece, una clave por terminal que alcanza el tope. Nunca se purga al cerrar la terminal.
   - Impacto: despreciable (una cadena corta por terminal topada). El backend ya emite una sola vez, por lo que el set es una defensa redundante.
   - Fix: aceptar el costo y dejar el set tal cual. Si se quiere purgar, escuchar el cierre de PTY y borrar la clave.

4. [Nit] `src/stores/kittyCapped.ts:1-2`
   - Escenario: los demás toasts de la app están en inglés ("Task ID copied", "Permission denied"). Este va en español.
   - Impacto: idioma mezclado en la UI. El copy en español neutro cumple la regla del usuario, pero rompe la coherencia interna.
   - Fix: confirmar con el usuario si el copy de toasts migra a español. Si no, dejar el texto en inglés.

VERDICT: PASS
