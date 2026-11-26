# Revisión adversarial: kitty-graphics-canvas (FRI-613)

Alcance: `git diff 23470f5..HEAD -- src-tauri src`, más proposal, design (Revision 1 y 2), implementation, tasks y el delta de `terminal-wezterm`. Fuente de wezterm: `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2` (abreviado `WZ`). Las decisiones de `interview_answers` no se discuten.

Verificación propia: `cargo test --lib kitty` pasa (71 ok, 1 ignored). Capturé la salida real de `chafa 1.8.0 -f kitty` para una imagen fija y para un GIF animado.

## Hallazgos

### 1. [Important] Un GIF animado de chafa pausa kitty en el noveno frame

- **Dónde:** `src-tauri/src/terminal/session.rs:310-324` y `src-tauri/src/terminal/session.rs:350-354`. La premisa está en `openspec/changes/kitty-graphics-canvas/design.md:99`.
- **Escenario:** El diseño afirma que chafa no apila imágenes. Eso es falso para animaciones. Generé un GIF de 12 frames y lo pasé por `chafa -f kitty -s 4x2`. La salida tiene 12 `a=T`, 11 `ESC [2A`, ningún `a=d` y ningún `i=`. Cada frame vuelve a las mismas celdas y transmite otra imagen con id 0. wezterm solo quita la colocación previa si el id no es 0 (`WZ/term/src/terminalstate/kitty.rs:103`). Después, `attach_image` agrega la imagen a la lista de la celda (`WZ/term/src/terminalstate/image.rs:209`). En el noveno frame, `stack_is_full` encuentra 8 imágenes. La sesión descarta el frame y llama a `mark_capped`. Kitty queda pausado en esa terminal para siempre y aparece el toast. La animación se congela en el frame 8. chafa es la herramienta del walk (tasks 4.3 y 4.8), así que el usuario lo verá.
- **Corrección sugerida:** Pedir una decisión al usuario. Opción A: antes de aplicar una colocación con id 0, la sesión quita de las celdas cubiertas las imágenes con id 0. Así la pila no crece y el costo sigue lineal. Opción B: aceptar el límite y documentarlo en Risks. En ambos casos, el tope de 1024 colocaciones detiene igual una animación larga. Por ejemplo, a 20 fps se detiene a los 51 s. Ese límite también debe constar en Risks.

### 2. [Important] El spec principal contradice el nuevo requisito del parser

- **Dónde:** `openspec/specs/terminal-wezterm/spec.md:13` frente a `openspec/changes/kitty-graphics-canvas/specs/terminal-wezterm/spec.md:5`.
- **Escenario:** El spec vigente exige pasar cada chunk al `wezterm_term::Terminal` "via `advance_bytes`". El delta exige que el parser propio del emulador "SHALL NOT be used". El delta solo trae `ADDED Requirements`. Al archivar, el spec queda con dos SHALL opuestos. Además, `src-tauri/clippy.toml` prohíbe ese método. El spec es la fuente de verdad, así que el próximo agente recibe un contrato roto.
- **Corrección sugerida:** Agregar al delta un `MODIFIED Requirements` para "Backend-owned VT emulation". El escenario debe decir que el chunk pasa por el parser de la sesión, el `KittyGate` y `perform_actions`. Después, correr `openspec instructions specs --change kitty-graphics-canvas` y `openspec validate kitty-graphics-canvas`.

### 3. [Nit] `docs/architecture.md` describe el camino viejo del parser

- **Dónde:** `docs/architecture.md:67`. También el doc comment de `src-tauri/src/terminal/session.rs:281-283`.
- **Escenario:** El doc dice que "`wezterm-term` parses PTY bytes server-side". También dice que el pre-scan corre "before wezterm parses a chunk". Ahora parsea el `termwiz::Parser` de la sesión. Luego el `KittyGate` filtra las acciones y `perform_actions` las aplica con el control de pila. Un agente que lea el doc no sabe que existe el gate. Tampoco sabe que clippy bloquea `Terminal::advance_bytes`.
- **Corrección sugerida:** Reescribir esa frase de `architecture.md:67` con el camino real: parser de la sesión, `KittyGate`, control de pila, `perform_actions`. Mencionar el guard de `clippy.toml`. Ajustar el doc comment de `session.rs:282` de la misma forma.

### 4. [Nit] Ningún test usa la forma exacta de los bytes de chafa

- **Dónde:** `src-tauri/src/terminal/kitty_gate_tests.rs:432` y `src-tauri/src/terminal/kitty_gate_tests.rs:570`.
- **Escenario:** chafa 1.8 manda la cabecera `a=T,f=32,s=..,v=..,c=..,r=..,m=1` sin `;` ni payload. Luego manda chunks `m=1;<base64>` y cierra con `m=0` sin `;` ni payload. El código lo acepta hoy. termwiz convierte el payload ausente en `Direct("")` (`WZ/termwiz/src/escape/apc.rs:1078` y `:116`). `decode_chunk("", 0)` devuelve un vector vacío (`kitty_gate.rs:424-430`). Pero los tests siempre ponen datos en el chunk `m=0`. Un cambio futuro en `decode_chunk` o en `continuation` puede romper chafa sin que falle ningún test.
- **Corrección sugerida:** Agregar un test con un fixture capturado de `chafa -f kitty` a tamaño chico. El test debe verificar que el gate entrega una sola `TransmitDataAndDisplay` con el tamaño exacto. Si los tests están bloqueados, pasarlo por el test-writer.

## Verificado sin hallazgo

- **Lecturas partidas:** el `Parser` es un campo de la sesión y guarda su estado entre llamadas (`session.rs:300`). El test `a_csi_split_across_reads_is_parsed_once` lo cubre.
- **OSC 7501 y modo sync:** siguen corriendo sobre los bytes crudos, antes del parser (`session.rs:285-299`). El orden de la respuesta 7501 frente a DA1 no cambia.
- **Respuestas al PTY:** `perform_actions` usa el mismo `Performer` que `advance_bytes` (`WZ/term/src/terminal.rs:164-173`). DA1, XTVERSION y similares siguen llegando al writer.
- **Allowlist:** `kitty_gate.rs` cumple D2 y el delta. Revisé medios, compresión, formato, lados 1..=4096, `I=`, `x`/`y`, `w`/`h`, `c`/`r`, `Delete` y `Quiet`.
- **Chunks:** las continuaciones se reconocen bien (`kitty_gate.rs:384-398`). Cada chunk se limita al tamaño restante. El tamaño exacto se exige al final. Toda acción kitty que no es continuación descarta la transferencia.
- **Control de pila:** vacía `ready` antes de cada colocación, así el cursor está al día (`session.rs:313-316`). Lee las líneas de a una. Comprobé que `with_phys_lines` sí indexa mal un ring buffer partido (`WZ/term/src/screen.rs:924-932`).
- **Índice multimapa del snapshot:** cada índice vive en una sola lista a la vez. Extender un run lo saca de la clave vieja y lo pone en la nueva. No se pierden ni se duplican runs (`session.rs:806-836`).
- **Aviso y locking:** `take_kitty_capped_notice` se toma bajo el mismo lock que `advance_bytes`. El `emit` ocurre fuera del lock (`pty.rs:890-914`). El `pty_id` incluye milisegundos, así que el dedupe del frontend no tapa una terminal nueva.
- **Capas del painter:** sixel e iTerm2 usan z 0 y siguen sobre el texto. El sort es estable. El swap de reverse da los mismos colores que antes (`terminalService.ts:1447-1491`).
- **Toast:** el texto está en español neutro, sin voseo (`src/stores/kittyCapped.ts:1-2`).

VERDICT: FAIL
