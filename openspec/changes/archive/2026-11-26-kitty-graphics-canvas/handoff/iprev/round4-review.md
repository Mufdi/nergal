# Review ronda 4: kitty-graphics-canvas

El fix de origen en 0 cierra bien el abort del round 3. Revisé wezterm: sixel e iTerm2 cachean solo imágenes con dimensiones ≥1 o con data vacía (`iterm.rs:37`, `sixel.rs:13`), y kitty nunca hashea data vacía, así que no hay cache hit con dimensión 0. Lo que sigue abierto es el round-3 #3 (CPU). Los count caps no lo acotan: el costo depende del producto entre la cantidad de placements y las celdas que ocupa cada una, y una de las rutas se repite en cada snapshot.

## Estado de los issues del round 3

| # | Estado |
|---|---|
| 1 (cache de dimensiones) | ✅ Para la división por cero. Queda la multiplicación u32 (#3 abajo) |
| 2 (overhead por transmisión) | ✅ El cobro mínimo de 4 KiB y el cap de 4096 transmisiones lo cierran, y ya no hay mapa en el gate |
| 3 (CPU cuadrático) | ❌ Mal acotado y mal testeado (#1 abajo) |
| 4 (`DirectBin`) | ✅ wezterm no decodifica nada. Verifiqué que decodifica por chunk (`kitty.rs:893-906`) |
| 5 (oráculo) | ✅ Lo no observable queda documentado como argumento aritmético |
| 6 (cota escrita) | ⚠️ Escrita, pero incompleta (#5) |
| 7 (clippy allow) | ✅ |
| 8 (`a=T`) | ✅ Se descarta entera si no entra en algún cap |

## Issues

1. **high: el CPU crece con placements × celdas, y el costo del snapshot se repite en cada frame.**
   - **PoC:** una transmisión `i=1`, luego 4096 veces `a=p,i=1,p=N,c=16,r=8,C=1` con N distinto. Cada una pesa unos 35 bytes, unos 140 KB en total.
     - Cobra 4096 × 128 = 524.288 celdas, justo el cap.
     - Con `C=1` el cursor no se mueve, así que todas quedan apiladas en las mismas 128 celdas (`image.rs:137-141`).
   - **Costo puntual en wezterm:**
     - `assign_image_to_cells` clona en profundidad la celda (`image.rs:191-195`) e inserta en el Vec (`cell.rs:452`). Son Σ 128·k ≈ 128 × 4096² / 2 ≈ **1,07×10⁹ clones de `Box<ImageCell>`**.
     - Un `a=d,d=i,i=1` hace el mismo orden de trabajo con `retain` por celda (`kitty.rs:296-306`, `cell.rs:434-438`).
   - **Costo repetido en el snapshot:**
     - Por cada celda hay 4096 imágenes. Cada una hace `runs.iter_mut().rev().find(continues)` (`session.rs:663`). Como las runs son idénticas, la imagen k recorre unas k runs antes de encontrar la suya, y cada comparación empieza por el hex de 64 bytes, que coincide entero.
     - Total: 16 columnas × 4096²/2 por fila, × 8 filas ≈ **1,07×10⁹ comparaciones por snapshot**. Además, `attrs.images()` clona las 524k celdas (`cell.rs:521-527`).
     - El snapshot corre con el mismo lock de sesión que usa el reader del PTY (`emitter.rs:192`, `pty.rs:892`). El terminal queda colgado varios segundos por frame mientras el stack esté en pantalla, y ningún lifetime cap lo frena.
   - **El test no lo ve:** "Stacked placements stay cheap" usa 1×1 con `c=1,r=1` (A=1), cuyo costo es solo 4096²/2.
   - **Fix:**
     - (a) en el snapshot, indexar las runs abiertas con un `HashMap` por (hash, z, col final, u1, v0, v1, pad_left, pad_top), así cada celda cuesta O(1);
     - (b) escribir en D4 la cota de wezterm, que es cells_cap × placements_cap / 2. Bajar los caps hasta que ese producto entre en un presupuesto medido (por ejemplo ≤5×10⁷), o medir el costo actual y aceptarlo de forma explícita;
     - (c) cambiar el test Red al peor caso (el PoC de arriba, varios snapshots y el delete), con tiempo acotado.

2. **medium: los artefactos se contradicen y el test-writer va a construir contra una interfaz vieja.**
   - La task 1.1 sigue pidiendo "unknown image 99 dropped" a nivel gate. D2 y el spec dicen ahora que el gate la reenvía y wezterm solo loguea. Un test Red bloqueado así obliga al builder a reintroducir el mapa de ids que D2 eliminó.
   - Otros restos viejos:
     - `implementation.md` § Builder coordination: `GateLimits { max_side, max_bytes, max_cells }` (ahora son 6 campos);
     - § Per-phase risk: "distinct image bytes inside wezterm" (2.1 ya no lo mide);
     - el diagrama de D5: "synthetic deletes" y "budget" (desde el round 2 no hay eviction).

3. **medium: el test de cached dimensions prueba el caso fácil, y D2 no cubre la multiplicación u32.**
   - **El test:** el escenario solo verifica que `x=2` se descarta, y eso ya es un hecho del gate. El argumento de D2 trata del caso que sí se reenvía. Falta un test que mande `a=T,f=32,s=4,v=1,w=4,c=1,r=1` con los mismos 16 bytes y verifique que no hay panic y que aparece una run.
   - **La multiplicación:** `(cols * cell_pixel_width) as u32 * params.image_width` (`image.rs:98`, igual en `:114`) usa el ancho de la imagen cacheada.
     - Un sixel diminuto (`!2795520~`, con bandas de 6 px) da una imagen de unos 2,8 M × 6.
     - Una transmisión kitty de 4096×4095 con los mismos bytes RGBA (unos 64 MiB, dentro de los caps) devuelve ese ancho.
     - Con un terminal de unos 1600 px de ancho, el producto pasa de 2³².
   - **Efecto:**
     - En debug (`cargo test`, `pnpm tauri dev`) es `attempt to multiply with overflow`, un panic.
     - En release no hay overflow checks: el valor da la vuelta, el divisor puede quedar en 0 y `x_delta` vale inf, que `NotNan` acepta. No hay crash, pero la imagen sale mal dibujada.
   - **Fix:** corregir la frase "cannot be zero" de D2 para que diga que vale en release por wrapping, y documentar el abort en debug o testearlo explícitamente.

4. **medium: "rare in interactive use" no tiene sustento, y al llegar al cap se descartan también los deletes.**
   - El canvas usa píxeles de dispositivo (`terminalService.ts:1277`), y chafa escala la imagen a esos píxeles.
     - Una imagen de pantalla completa en 4K/HiDPI pesa unos 3840×2000×4 ≈ 30 MiB, o sea **unas 8 imágenes por vida del terminal**.
     - En 1080p (unos 1500×900) son unas 47.
   - Una sesión larga de Claude Code (y FRI-624, que hereda estos caps) llega al cap sin que el usuario lo note: solo queda un log.
   - No discuto los lifetime caps. Pido que D4 diga el número real de imágenes que entran y que el usuario decida con ese dato.
   - Además, después del cap también se descarta `a=d`. Una imagen que el programa intenta borrar queda en pantalla. Conviene dejar pasar los deletes hasta su propio cap aunque el cap de bytes o de celdas se haya alcanzado.

5. **low: a la cota escrita le faltan copias.**
   - Transitorias por transmisión: la conversión `Rgb` → RGBA (`kitty.rs:789-797`), la copia IPC de `payload_for_bounded`, y `ImageData` + `ImageBitmap` en WebKit.
   - Por frame: el clon de hasta 524k `ImageCell` de `images()`.
   - Hay que decir también que "× sesiones" depende de cuántos terminales se abren, y que el agente puede abrirlos vía MCP `create_worktree_session`. No los abre el PTY, pero conviene escribirlo.

6. **low: nada verifica que el guard de clippy funcione.** `wezterm_term::Terminal` es un re-export. Si clippy no resuelve el path, `disallowed-methods` no hace nada y no falla de forma confiable con `-D warnings`. Falta un paso en las tasks: agregar una llamada temporal, ver que clippy falla y quitarla.

7. **low: el rollback necesita un release.** Para un parser de riesgo CRITICAL no hay forma de apagarlo en runtime. Un flag de config (`enable_kitty_graphics` leído del config de Nergal) permitiría desactivarlo sin esperar a CI.

VERDICT: REVISE
