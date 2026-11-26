# Review ronda 3: kitty-graphics-canvas

Los lifetime caps cierran bien la clase de bugs de borrados inexactos del round 2 (#1, #3 y #5). Pero el argumento "retained ≤ forwarded" falla de dos formas. Además el gate supone que conoce las dimensiones de cada id, y eso es falso por el image cache de wezterm. Ese error lleva a un **abort del proceso** con unos 120 bytes de input.

## Estado de los issues del round 2

| # | Estado |
|---|---|
| 1 (`i=0`), 2 (retransmisión), 3 (borrados inexactos) | ✅ En memoria, los caps lo acotan. La premisa de dimensiones de #2 se rompe por otra vía (nuevo #1) |
| 4 (`c`/`r`) | ✅ Resuelto, se exigen explícitos |
| 5 (4 bpp, cache) | ✅ El cache solo guarda `Arc` de data reenviada y contada |
| 6 (`U=1`) | ✅ Aceptado y documentado |
| 7 (chunks) | ⚠️ Falta definir la transmisión de un solo chunk (#4) |
| 8, 11 | ✅ Ya no aplican (no hay ids privados ni eviction) |
| 9 | ✅ Non-Goal corregido |
| 10 (oráculo) | ⚠️ Mejoró, pero no puede observar todo lo que afirma (#5) |
| 12, 13, 14 | ✅ Resueltos (Display de ids podados sigue logueando, acotado por el cap) |

## Issues

1. **critical: el image cache de wezterm devuelve otras dimensiones, el gate valida `x`/`y` contra las suyas, y wezterm divide por cero.**
   - **Causa en wezterm:**
     - `raw_image_to_image_data` (`term/src/terminalstate/image.rs:257-270`) busca en `image_cache` por `compute_hash()`.
     - Para `Rgba8`, el hash cubre solo los bytes, no `width`/`height` (`termwiz/src/image.rs:304-310`).
     - Si hay hit, devuelve el `Arc<ImageData>` **viejo, con sus dimensiones viejas**, y `kitty_img_place` usa esas (`kitty.rs:114`).
   - **Supuesto falso en D2:** "the gate keeps each forwarded id's width and height … the same way wezterm replaces its data". El gate valida `x < width` contra sus propias dimensiones.
   - **PoC** (16 bytes cero = `AAAAAAAAAAAAAAAAAAAAAA==`):
     ```
     ESC _ G a=t,i=1,f=32,s=1,v=4 ; AAAAAAAAAAAAAAAAAAAAAA== ESC \
     ESC _ G a=T,f=32,s=4,v=1,x=2,c=1,r=1 ; AAAAAAAAAAAAAAAAAAAAAA== ESC \
     ```
   - **Qué pasa:**
     - El gate ve 4×1 con `x=2 < 4` y la reenvía.
     - wezterm recibe la imagen cacheada de 1×4. `image_max_width = 1.saturating_sub(2) = 0`, así que `draw_width = 0` (`image.rs:81-86`).
     - `(cols * cell_pixel_width) as u32 * image_width / draw_width` (`image.rs:98`) es una división entera por cero. Hace panic también en release y, con `panic = "abort"` (`Cargo.toml:169`), **muere todo Nergal**.
     - Con `y` y `image.rs:114` pasa lo mismo. También funciona retransmitiendo al mismo id y con `a=T` sin id ("uses the transmission's own size").
     - El cache se comparte con sixel, así que una imagen sixel también puede imponer sus dimensiones.
   - **Fix:** exigir `x` e `y` ausentes o 0. Con eso `draw_width = min(w ∨ W, W) ≥ 1` para cualquier dimensión que tenga wezterm, y el gate no depende de su mapa de dimensiones (chafa no manda `x`/`y`).
   - **Test Red:** el PoC a través de `advance_bytes`. El test del gate solo no lo puede detectar.
   - **Riesgo aceptado a documentar:** dos imágenes legítimas con los mismos bytes y distintas dimensiones (por ejemplo imágenes uniformes) se dibujan con las dimensiones equivocadas. Es un bug upstream que el fix no evita.

2. **high: "retained ≤ forwarded" es falso para el overhead fijo de cada transmisión.**
   - D4 cobra `w*h*4` por transmisión, así que una imagen 1×1 cuesta 4 bytes. El cap de 512 MiB permite unos **134 M transmisiones** a ids distintos.
   - Cada una deja:
     - una entrada en el mapa id → (w,h) **del propio gate**: hashbrown con 2^28 buckets, unos 3,5 GB;
     - una entrada en `id_to_data` de wezterm (`kitty.rs:43`). La pixel data se comparte por cache hit, pero las entradas no.
   - `used_memory` suma solo `data.len()` (4 bytes), así que `prune_unreferenced` recién actúa con unos 84 M entradas, es decir unos 2-3 GB solo en wezterm.
   - Desde ahí, cada transmisión hace un `retain` O(n) sobre todo el mapa (`kitty.rs:52-59`). El terminal queda colgado en CPU.
   - Total: unos 6 GB por terminal más un hang, con un stream generado por un loop de shell por ssh. La cota existe, pero es más de 10 veces la que afirma el plan.
   - **Fix:**
     - cobrar `max(w*h*4, OVERHEAD)` (por ejemplo 4 KiB) o poner un cap de cantidad de transmisiones en la vida del terminal;
     - acotar o eliminar el mapa del gate (tras #1 solo sirve para "unknown id", y wezterm ya lo rechaza sin crash).
   - **Test:** N transmisiones mínimas a ids distintos llegan al cap de cantidad.

3. **high: hay costo de CPU cuadrático acotado solo por el cap de 1 M celdas, y los deletes no se cuentan.**
   - `assign_image_to_cells` hace `get_cell().cloned()`, que clona en profundidad todo el `Vec<Box<ImageCell>>` de la celda, y luego inserta (`image.rs:191-216`, `cell.rs:444-455`). Apilar k placements en una celda (ids o `p` distintos, o `a=T` sin id, con `c=1,r=1` y `\r`) cuesta O(k²). Con k = 1 M son unos 5×10¹¹ allocs.
   - `kitty_remove_placement` sin `p` recorre todas las keys de `placements` (`kitty.rs:319-324`). Pasa en cada `a=p` sin `p` y en cada `a=d,d=i`. Los **deletes no se cobran**: con 1 M placements vivas, cada delete de unos 14 bytes cuesta unos 1 ms, sin límite.
   - El snapshot de Nergal:
     - hace deep-clone de las líneas visibles en cada frame (`screen.rs:888`);
     - por cada imagen busca con `runs.iter_mut().rev().find` (`session.rs:663`), que es O(runs²) por fila cuando las runs no se fusionan (z distintos);
     - manda cada run por IPC y el frontend las pinta. Con el cap entero concentrado en pantalla son cientos de MB por frame.
   - **Fix:** cap de cantidad de placements en la vida del terminal (por ejemplo 4096-16384), con el peor caso calculado en el design, y cobrar los deletes (contador propio o costo en celdas).
   - **Test Red con tiempo acotado:** el máximo de placements apiladas en una celda, más un `grid_snapshot`, más N deletes, en menos de 1 s.

4. **medium: falta especificar la transmisión de un solo chunk.**
   - D3 dice `DirectBin` solo para transfers armados en el gate. Una transmisión de un chunk podría reenviarse como `Direct(String)`.
   - El decoder de wezterm es `pub(crate)` (`termwiz/src/escape/osc.rs:1233-1242`): alfabeto STANDARD, padding canónico y `allow_trailing_bits(true)`.
   - Si el gate decodifica con una config más permisiva, valida un tamaño que wezterm después rechaza. Hoy eso solo cuesta una imagen faltante; con #1 sin arreglar es otra vía al abort.
   - **Fix:** reenviar siempre `DirectBin(decoded)` y documentar la config del decoder (igual o más estricta).

5. **medium: el oráculo de la task 2.1 no puede observar lo que afirma.**
   - `id_to_data` e `image_cache` son privados, así que "distinct image bytes held by the emulator" solo se puede medir desde las celdas. No ve la data transmitida sin colocar (`a=t`), ni el cache, ni el overhead de #2.
   - La pantalla inactiva solo se inspecciona alternando con `?47h`/`?47l`. `?1049h` limpia la pantalla alt al entrar, así que esa vía no sirve.
   - **Fix:** documentarlo, y dejar explícito que la parte no observable descansa en el argumento aritmético. Faltan tests para:
     - el PoC de #1;
     - muchas transmisiones mínimas (#2);
     - el apilamiento con tiempo acotado (#3).

6. **low: la cota real por terminal no está escrita.**
   - Suma: 512 MiB, más 1 M × (`Box<ImageCell>` de unos 64 B + slot del Vec + `FatAttributes` + storage de la celda) que da unos 100-200 MB, más el transfer en vuelo de 64 MiB con su copia decodificada, más el overhead de #2.
   - Hay que escribirla en bytes y multiplicarla por las N sesiones y quake shells.

7. **low: el guard de clippy choca con el comparison test.** El test de la task 2.1 llama a `Terminal::advance_bytes` para comparar con el parser del emulador. `cargo clippy --all-targets -D warnings` lo va a rechazar. Hay que preverlo con un `#[allow(clippy::disallowed_methods)]` acotado a ese test.

8. **low: `a=T` sin definir cuando pasa un cap y no el otro.** Si los bytes entran y las celdas no (o al revés), no está dicho si se descarta la acción entera, si se cobra la parte que entraba ni si se activa `capped`.

VERDICT: REVISE
