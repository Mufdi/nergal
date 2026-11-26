Ronda 2: el rediseño cierra la clase entera de diferenciales de parsing (round-1 #1, #2, #3, #11 y #16). También resuelve el accumulator (#6), el `m=0` sintético (#8) y las respuestas `I=` (#9), porque todo sale con Quiet y la verbosidad Quiet hace `return` en `kitty.rs:366-368`. Pero el presupuesto de memoria (D4) se apoya en un supuesto falso: que el modelo del gate coincide con el estado de wezterm. Hay al menos cuatro formas de separarlos, y eso devuelve la amplificación de memoria sin cota que el round 1 marcó en #5 y #7.

## Estado de los issues del round 1

| # | Estado |
|---|---|
| 1, 2, 3, 11, 16 | ✅ Resueltos: el gate ve exactamente las `Action` que wezterm ejecutaría (`parser/mod.rs:216-222`, `terminal.rs:164-173`) |
| 4 | ✅ Resuelto para `w`/`x`/`h`/`y` explícitos (con `clamp_pixel_geometry`, `session.rs:68-86`, `cell_pixel_width >= 1`). Sigue dependiendo de que el modelo del gate sea exacto (ver #2) |
| 5, 7 | ❌ Siguen abiertos por caminos nuevos (#1-#5 abajo) |
| 6, 8, 9 | ✅ Resueltos |
| 10 | ⚠️ Se movió a Non-Goal, pero mal descrito (#9) |
| 12 | ⚠️ Mejoró, pero el oráculo del fuzz es tautológico y hay un test imposible (#6, #10) |
| 13 | ✅ Resuelto (tests de borde de z y texto sobre placement) |
| 14, 15 | ⚠️ Parcial (#12, #13) |

## Issues

1. **critical: `i=0` esquiva el remapeo a id privado y vuelve el apilamiento de placements.** D4 asigna id privado solo a "a transmission without an image id". Pero `i=0` parsea como `image_id: Some(0)` (`apc.rs:10-12`, `geti`). wezterm trata el id 0 aparte en dos lugares:
   - `kitty_img_place` no borra placements previos (`kitty.rs:103-105`, `if image_id != 0`).
   - `record_id_to_data` no saca la data vieja del contador (`kitty.rs:38`).

   El gate interpreta "same id without a placement id" como reemplazo y actualiza una sola entrada, mientras wezterm acumula. Repetir `ESC _ G a=T,i=0,f=32,s=1,v=1,c=80,r=24,C=1;AAAAAA== ESC \` 5000 veces deja 5000×1920 `ImageCell` (`attach_image` inserta sin dedupe, `cell.rs:444-455`), con el gate creyendo que hay 1 placement vivo. Fix: rechazar `i=0` o remapearlo a id privado, con test Red que cuente las celdas **en wezterm**, no en el gate.

2. **critical: retransmitir a un id existente fija la data vieja, y el presupuesto de bytes decodificados no la cuenta.** `record_id_to_data` (`kitty.rs:37-44`) saca el `Arc` del map pero no quita los placements. Cada `ImageCell` guarda su propio `Arc<ImageData>` (`image.rs:196-207`). Secuencia: `a=T,i=1,p=1` (64 MiB), luego `a=T,i=1,p=2` con otra data. `kitty_img_place` solo borra `(1, Some(2))` (`kitty.rs:313-317`). Las celdas de `p=1` siguen reteniendo la imagen A. El gate ve "imagen 1, 64 MiB". Si se repite con `p=3..1024`, wezterm retiene unos 64 GiB y el gate cuenta 64 MiB. Esto también rompe la premisa de D2 de que el gate conoce el tamaño de la imagen colocada: si el gate y wezterm discrepan en las dimensiones de un id, `x < width` se valida contra el tamaño equivocado. Fix: antes de reenviar una transmisión a un id ya rastreado, inyectar `Delete{ByImageId{id, placement_id: None, delete: true}}`, o descartar la transmisión. Falta test Red.

3. **critical: el borrado de placements de wezterm no es exacto, así que eviction y reemplazo no liberan nada.** `kitty_remove_placement_from_model` (`kitty.rs:292-310`) solo despega en `screen_mut()`, que es la pantalla **activa** (`terminalstate/mod.rs:154-162`). Además solo recorre el rango `first_row..first_row+rows` registrado al colocar (`PlacementInfo`, `image.rs:13-17`, `:127`). Cuatro formas de dejar celdas huérfanas que retienen el `Arc`:
   - `ESC [?47h` / `ESC [?47l` cambian de pantalla **sin limpiarla** (`mod.rs:1670-1695`). Se colocan imágenes en alt, se vuelve a primary, el gate hace eviction y wezterm despega en primary. Alt sigue con las imágenes, en memoria y visibles al volver.
   - `CSI L` / `CSI M` y LF dentro de un DECSTBM mueven líneas dentro de la región. El índice estable de las líneas movidas cambia, así que el rango registrado ya no las cubre.
   - El resize hace rewrap del primary y `first_row` queda viejo. Esto también le pasa a un usuario legítimo que redimensiona la ventana.
   - `Delete All` (`kitty.rs:340-349`) vacía el map entero y despega solo en la pantalla activa.

   Resultado: el gate cree que el presupuesto está bajo los 256 MiB, 1024 placements y 131072 celdas, mientras wezterm retiene sin cota (limitado solo por el scrollback y las dos pantallas). La línea de Risks "[Deleted placements in scrollback] → the builder verifies…" deja una propiedad de seguridad en manos del builder, sin test. Fix: el presupuesto debe ser sólido frente a borrados inexactos. Opciones:
   - Contar como retenida toda data transmitida hasta un evento que la libere de forma demostrable (por ejemplo RIS o recrear el terminal, a verificar), con un tope acumulado por terminal.
   - Que el gate, al ver en el stream de acciones un cambio de pantalla (47/1047/1049), IL/DL, scroll con márgenes o un resize, borre las imágenes antes de reenviar.

   Cualquiera de las dos necesita un test Red con oráculo sobre el estado real de wezterm en ambas pantallas.

4. **high: el gate no puede calcular la huella de un placement sin `c`/`r`.** Sin `c`, `fullcells_width = draw_width / cell_pixel_width` (`image.rs:101-107`). Sin `r`, la altura sale igual y hace `new_line` por fila (`:160-223`). `clamp_pixel_geometry` permite `pixel_width == cols`, o sea `cell_pixel_width = 1`, así que una imagen de 4096×4096 ocupa 4096×4096 celdas y 4096 scrolls por placement. D2 solo acota "explicit `c`/`r`". La regla de 131072 celdas suma "columns × rows" y no define el caso sin ellos. Además `admit(actions, cols, rows)` no recibe la geometría en píxeles, así que ni siquiera podría calcularla. Fix: exigir `c` y `r` explícitos con `1 <= c <= cols` y `1 <= r <= rows` (chafa siempre los manda), o pasar `pixel_width`/`pixel_height` al gate y calcular la huella exacta.

5. **high: el presupuesto de 256 MiB está subestimado al menos 5x.**
   - `raw_image_to_image_data` guarda cada imagen en `image_cache: LruCache::new(16)` (`image.rs:257-270`, `mod.rs:571`). Después del `Delete` sintético, las 16 imágenes más recientes siguen retenidas por el cache: hasta 16×64 MiB = 1 GiB extra por terminal con `max_side = 4096`.
   - RGB se guarda expandido a RGBA (`kitty.rs:789-797`), pero D2 cuenta `width × height × bpp` con bpp=3. Eso subcuenta 33% y además dispara el `prune_unreferenced` de 320 MB de wezterm por su cuenta (`kitty.rs:46-68`), con lo que el modelo del gate diverge otra vez.
   - Es por terminal, y Nergal tiene N sesiones más los quake shells.

   Fix: contar a 4 bpp, contar el cache como retenido (presupuesto ≥ data viva + 16 × la imagen máxima) y bajar `max_side` o el tope por imagen.

6. **high: el scenario de `U=1` no se puede cumplir.** termwiz no parsea `U` en esta revisión (no aparece en `KittyImagePlacement::from_keys`, `apc.rs:618-636`). La información se pierde antes de llegar al gate. `a=T,U=1,f=32,s=1,v=1;AAAAAA==` es una transmisión directa válida: el gate la reenvía y sí cambia el grid. El scenario "Compression, animation, PNG and placeholders are dropped … none SHALL change the grid" y la task 1.1 ("`U=1` dropped") contradicen D1: el test Red quedaría en rojo para siempre o se debilitaría a escondidas. Hay que decidir y documentarlo: aceptar que un `U=1` se dibuja como placement normal (residual hasta FRI-624, poco probable sin respuesta a la query) o explicar cómo se obtendrían los bytes crudos.

7. **medium: las reglas de continuación de chunks están sin especificar.** Un chunk de continuación (`Gm=1;…`) parsea como `TransmitData` con todos los campos en default. D3 no define:
   - qué cuenta como continuación y qué como transmisión nueva (un `a=t` con `s`/`v` en medio de una transferencia);
   - que la continuación debe tener `KittyImageData::Direct`, compresión `None` y el resto de los campos en default (si no, se descarta la transferencia);
   - qué pasa con un `t=f` en una continuación.

   Además wezterm decodifica base64 **por chunk** (`kitty.rs:892-907`). Si el gate concatena los strings y reenvía `Direct(String)`, un chunk con padding intermedio (`"AAA=" + "AAA="`) no decodifica en wezterm, aunque el gate haya validado el tamaño. Fix: reenviar `DirectBin(decoded)`, decodificando por chunk.

8. **medium: el rango de ids privados no está protegido.** Nada impide que el atacante mande `i=0xFF000000+k`, choque con los ids privados y corrompa la tabla del gate (reemplazo de data, borrado de imágenes ajenas, desincronización del orden de eviction). Fix: descartar las acciones con `i` del atacante en el rango reservado (y `i=0`, ver #1). También hay que exigir en `Display` que `image_number` sea `None` e `image_id` sea `Some`, porque D2 solo lo exige para las transmisiones.

9. **medium: la meta "no unbounded memory … through kitty" es falsa tal como está escrita.** El buffer APC de vtparse (`vtparse/src/lib.rs:593-601`) no tiene cota también para `ESC _ G …` sin terminador. El gate recibe la acción recién al despachar, así que un solo APC kitty de 10 GB llega entero a memoria. El Non-Goal lo describe como "for non-kitty APCs", y eso es incorrecto. Es pre-existente y el usuario no pidió arreglarlo, pero Goals y Non-Goals deben decirlo bien. Tampoco se cuenta la transferencia en vuelo: hasta unos 89 MB de base64 más 64 MiB decodificados por terminal, fuera del presupuesto.

10. **medium: el fuzz no prueba la propiedad de presupuesto.** "asserts every forwarded kitty action satisfies the allowlist and the budget" compara contra el **propio modelo del gate**, así que es tautológico y no detecta #1, #2, #3 ni #5. Hace falta un oráculo sobre wezterm: contar las `ImageCell` y los bytes distintos de `ImageData` en primary con scrollback y en alt (el test puede alternar con `?47h`/`?47l` para inspeccionar). El corpus debe incluir acciones no kitty que muevan líneas: `?47`, `?1049`, IL/DL, DECSTBM con LF, RI y resize. Faltan tests Red para `i=0`, la retransmisión a un id existente, el choque con el rango reservado, los placements sin `c`/`r` y las continuaciones con medio no directo.

11. **medium: la semántica de eviction tiene huecos.**
    - No dice qué pasa si la imagen más vieja es la que usa la acción pendiente: el gate la borraría y después reenviaría un `Display` de un id borrado.
    - No dice qué pasa si una acción sola ya excede el límite (debería descartarse).
    - El spec dice "either limit" con tres límites.
    - El gate debe replicar la semántica de `delete`: `d=i` minúscula conserva la data, y `d=a` borra los placements de las dos pantallas en el map aunque despegue solo en una.

12. **low: siguen pasando acciones que solo generan logs.** `Delete` se reenvía siempre, y las variantes que wezterm no implementa emiten `log::warn!` por cada secuencia (`kitty.rs:270-272`). Los `Display` de ids que wezterm ya podó por su cuenta emiten `log::error!` (`performer.rs:266-268`). Conviene descartar las variantes no implementadas.

13. **low: nada impide un camino futuro sin gate, y el rollback es parcial.** `enable_kitty_graphics` es global en la config. Cualquier llamada futura a `self.terminal.advance_bytes` (hoy solo existe `session.rs:270`) se salta el gate. La regla vive solo como "Trap" en el doc. Conviene un guard mecánico: `clippy::disallowed_methods` sobre `Terminal::advance_bytes`, o un test que falle si alguien lo llama. Además el rollback no puede revertir el cambio de parser sin un release, y eso debe quedar documentado.

14. **low: `f` ausente es ambiguo.** D2 dice "format `Rgb` or `Rgba`", mientras implementation dice "`f` absent = None, which kitty treats as RGBA". Hay que definir si `None` se acepta, y con qué bpp se valida y se cuenta.

VERDICT: REVISE
