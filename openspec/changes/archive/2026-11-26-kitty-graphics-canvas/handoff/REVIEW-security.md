# Revisión de seguridad: kitty-graphics-canvas (FRI-613)

Nivel de riesgo: CRITICAL. Alcance: `git diff 23470f5..HEAD -- src-tauri src`, más design (D1 a D6, Revision 1 y 2), el delta de `terminal-wezterm`, implementation y evidence. Fuente de wezterm: `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2` (abreviado `WZ`). Las decisiones de `interview_answers` no se discuten. No repito los hallazgos de `REVIEW-adversarial.md`.

Modelo de amenaza: cualquier programa de la terminal controla los bytes del PTY. Solo reporto lo que lee o borra archivos, escribe al PTY, causa panic o abort, o supera el peor caso escrito de memoria o CPU.

## Hallazgos

### 1. [Important] Un cursor en la columna `physical_cols` evita el tope de pila de 8

- **Dónde:** `src-tauri/src/terminal/session.rs:337-340` y `src-tauri/src/terminal/session.rs:348-354`.
- **Causa:** `stack_is_full` recorta las columnas con `.min(screen.physical_cols)`. wezterm deja el cursor en `x == physical_cols` tras un CUP fuera de rango. El límite absoluto admite una columna extra (`WZ/term/src/terminalstate/mod.rs:1004-1010`). CUP usa ese camino (`WZ/term/src/terminalstate/mod.rs:2454-2458`). Con `first_col == physical_cols`, el rango `first_col..last_col` queda vacío. La función devuelve `false` siempre.
- **Qué hace wezterm:** `assign_image_to_cells` escribe en `cursor_x + x`, fuera del ancho visible (`WZ/term/src/terminalstate/image.rs:191-216`). `Line::set_cell` alarga la línea para esas celdas (`WZ/termwiz/src/surface/line/line.rs:807-814`). Cada colocación clona la lista entera de la celda y agrega una imagen (`WZ/termwiz/src/cell.rs:444-455`). El snapshot recorre esas celdas en cada frame (`session.rs:643`, `session.rs:653`).
- **Escenario:** Un programa envía 1024 `a=t,f=32,s=1,v=1,i=N`. Luego envía `ESC [1;999H`. Después envía 1024 `a=p,i=N,c=16,r=8,C=1`. El gate acepta cada colocación (`kitty_gate.rs:407-420`). El control de pila no descarta ninguna. Es el test `the_worst_stacking_of_1024_placements_stays_under_two_seconds` con un CUP antes.
- **Reproducción:** Armé una sonda aparte con el mismo tag de wezterm. Repliqué el parser propio, `perform_actions` y `stack_is_full` tal como están. Resultado en release: cursor en x=80 con 80 columnas, 0 colocaciones descartadas, 1024 imágenes en una celda, 131.072 entradas, 11,3 s de colocación.
- **Impacto:** Vuelve el costo O(N²) que Revision 2 eliminó (`design.md:90-92`). El hilo lector retiene el lock de la sesión unos 11 s. Además, cada frame clona hasta 1024 imágenes por celda mientras la pila está en pantalla. El peor caso escrito es 10⁶ clones y 8 imágenes por celda (`design.md:92`). La memoria sigue acotada por el tope de celdas.
- **Corrección:** No recortar las columnas al ancho visible. Recorrer `first_col..first_col.saturating_add(columns as usize)`. `line.get_cell` devuelve `None` para una celda que no existe, así que no hay panic. El costo sigue acotado porque `c` no supera `cols`. Agregar un test Red: la secuencia del escenario termina con 8 imágenes por celda como máximo y con el aviso.

## Verificado sin hallazgo

- **Archivos, temporales y shm:** solo `load_data` toca el sistema de archivos (`WZ/termwiz/src/escape/apc.rs:181-265`). `KittyImageData` no tiene `Drop`, y el parser no llama a `load_data`. El gate reenvía solo `DirectBin` (`kitty_gate.rs:237-242`, `kitty_gate.rs:291`). `Display` y `Delete` no llevan datos. `Query`, `TransmitFrame` y `ComposeFrame` se descartan (`kitty_gate.rs:184-186`).
- **Respuestas al PTY:** wezterm responde solo en `kitty_send_response` (`WZ/term/src/terminalstate/kitty.rs:351-388`). Lo llaman la query, los frames y una transmisión con `I=` (`kitty.rs:838-846`). Las tres se descartan. Toda acción reenviada lleva `Quiet` (`kitty_gate.rs:172`, `kitty_gate.rs:214`, `kitty_gate.rs:293`), y `Quiet` retorna antes de escribir (`kitty.rs:366-368`). El aviso de tope es un evento de Tauri, no un write (`pty.rs:904-914`).
- **Descompresión y PNG:** se descartan en `kitty_gate.rs:308-321`. El acumulador de wezterm nunca recibe nada: todo lo reenviado lleva `more_data_follows` en false (`kitty_gate.rs:292`).
- **Aritmética del gate:** `continue_transfer` resta sin desborde. `collected` nunca supera `expected_len` (`kitty_gate.rs:239-240`, `kitty_gate.rs:263`, `kitty_gate.rs:429`). Los píxeles caben en u64 y el resultado cabe en usize (`kitty_gate.rs:327-329`). `try_charge` suma solo después de `fits`, con totales bajo los topes (`kitty_gate.rs:348-357`). `decode_chunk` limita el largo antes de decodificar y no hace panic (`kitty_gate.rs:424-430`).
- **Aritmética de wezterm:** `clamp_pixel_geometry` da al menos 1 píxel por celda (`session.rs:86-104`), así que `cell_pixel_width` no es 0. Con `x`/`y` en 0 y `w`/`h` sobre 0, `draw_width` es al menos 1 y el divisor de `image.rs:98, 114` no es 0. `width * height * 4` cabe en u32 con lados hasta 4096 (`kitty.rs:801-808`). El release no activa `overflow-checks` (`src-tauri/Cargo.toml:166-170`), así que el desborde de sixel aceptado en D2 no aborta.
- **Control de pila línea por línea:** `phys_row` recorta la fila (`WZ/term/src/screen.rs:461-467`). `lines_in_phys_range` usa `skip`/`take` y no indexa un slice (`WZ/term/src/screen.rs:888-895`). `get_cell` devuelve `Option`. No hay panic. El costo por colocación queda acotado por el tamaño de la pantalla.
- **Guard de clippy:** solo `Terminal::advance_bytes` usa el parser del emulador (`WZ/term/src/terminal.rs:152-162`). Nergal construye un solo `Terminal` (`session.rs:194`). La sesión expone solo `&Terminal` (`session.rs:444-446`). El crate es único, sin workspace. CI corre `cargo clippy --all-targets -- -D warnings` (`.github/workflows/ci.yml:195`). evidence.md:45 muestra que el lint falla ante una llamada directa. El único `allow` está en un test (`session_kitty_tests.rs:198`).
- **Chunks, claves duplicadas, C1 y lecturas partidas:** el `Parser` de la sesión resuelve todo eso antes del gate (`session.rs:300`). Una continuación debe ser un `TransmitData` sin otras claves (`kitty_gate.rs:384-398`). Cualquier otra acción kitty descarta la transferencia y se juzga sola (`kitty_gate.rs:142-147`). El texto entre chunks no toca la transferencia, y el resultado final pasa las mismas reglas.
- **Memoria:** hay una sola transferencia en vuelo, de 64 MiB como máximo. Los topes de vida cubren cada transmisión y colocación reenviada, sin reembolso.
- **Nota sin hallazgo:** con el cursor bajo una región DECSTBM, `new_line` no avanza (`WZ/term/src/terminalstate/mod.rs:1092-1098`). Una sola colocación sin `C=1` apila `r` imágenes en la última fila. La sonda midió 24 imágenes en una celda en 14 ms. La profundidad queda bajo filas + 7 y el tope de celdas la limita. No lo reporto.

## Resumen

- Critical: 0
- Important: 1
- Nit: 0

Prioridad de corrección: el hallazgo 1, con su test Red.

VERDICT: FAIL
