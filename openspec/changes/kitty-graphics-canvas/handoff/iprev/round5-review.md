# Review ronda 5: kitty-graphics-canvas

Revisé de nuevo contra wezterm (`image.rs:65-200`, `cell.rs:434-527`, `kitty.rs:241-320`) y contra `session.rs:530-684` y `Cargo.toml`. El high del round 4 queda cerrado. No encontré ningún camino critical ni high a crash, acceso a archivos, respuesta al PTY, ni memoria o CPU por encima de la cota escrita. Lo que queda es medium o low, y lo pueden resolver el builder y los code reviewers.

## Estado de los issues del round 4

| # | Estado |
|---|---|
| 1 (CPU por placements × celdas) | ✅ Los caps bajan a 131.072 celdas y 1024 placements. Con eso, la cota en wezterm queda en unos 6,7×10⁷ clones, una sola vez. El delete `d=i` sin `p` hace el mismo orden de trabajo y después ya no queda nada que recorrer (`kitty.rs:296-306`). El snapshot pasa a indexar las runs abiertas y el test Red usa el peor caso. Quedan dos detalles en #1 y #2 abajo |
| 2 (artefactos contradictorios) | ⚠️ Se corrigieron los restos que listé, pero el cambio de los deletes agregó contradicciones nuevas (#3) |
| 3 (dimensiones cacheadas y u32) | ✅ Hay un test del caso que se reenvía y el wrap queda documentado. Verifiqué que el release no tiene `overflow-checks` (`Cargo.toml` `[profile.release]`). `clamp_pixel_geometry` (`session.rs:68-87`) asegura al menos 1 px por celda, así que no hay `0/0` que dé NaN en `NotNan`. Con el wrap, el resultado es `inf` sin NaN, y no hay crash |
| 4 (capacidad real, deletes después del cap) | ✅ D4 da los números, los deletes siguen pasando y hay aviso visible |
| 5 (copias transitorias, MCP) | ✅ |
| 6 (verificar el guard de clippy) | ✅ Task 4.2 |
| 7 (rollback en runtime) | ✅ `NERGAL_DISABLE_KITTY=1` |

## Issues

1. **medium: el índice del snapshot tiene que ser un multimap, y el test no lo verificaría.**
   - En el peor caso (1024 placements idénticas apiladas), cada celda tiene 1024 imágenes con la misma clave de merge. Un `HashMap<Key, usize>` simple pierde runs: la primera imagen extiende la run y le cambia la clave (la columna final avanza), y las otras 1023 abren runs nuevas en cada columna.
   - Resultado: unas 131.072 runs por snapshot en vez de 8192. Eso significa varias decenas de MB de IPC y unos 131k `drawImage` en cada repintado de esas filas. No hay crash y el costo está acotado, pero el UI se cuelga.
   - El test solo mide tiempo. Fix:
     - en `implementation.md` §3, pedir `HashMap<Key, Vec<run_idx>>` (o un contador) que saque una run por cada match;
     - en el test del peor caso, agregar que cada fila trae exactamente 1024 runs.

2. **medium: el límite de 2 s del test de stacking no está medido y no tiene plan B.**
   - Son unos 6,7×10⁷ clones profundos de `Box<ImageCell>` (una alocación y un incremento de `Arc` cada uno, `image.rs:191-195`). Eso puede pasar de 2 s en hardware modesto.
   - Si el test falla, el builder no puede tocar los caps, porque son decisión del user.
   - Durante ese tiempo el reader del PTY tiene tomado el lock de la sesión, una sola vez.
   - Fix: en la fase Red, medirlo antes de bloquear el umbral. Si no entra, escalarlo al user con el número medido, sin aflojar el test.

3. **medium: los artefactos se contradicen sobre los deletes después del cap.** Un test-writer que lea las tasks puede bloquear un test opuesto al spec:
   - tasks 1.1: "a capped gate drops later kitty actions". Tiene que decir "later transmissions and placements";
   - `implementation.md` § State: "a `capped` flag that, once set, drops every later kitty action";
   - el último párrafo de D4: "When a cap is reached, the terminal drops every later kitty action and logs once".

4. **low: restos viejos en los artefactos.**
   - D4 § Risks: todavía dice "524,288 placed cells or 4096 … placements or deletes" y "Expected to be rare in interactive use", lo que contradice el párrafo "What fits".
   - tasks 2.1: dice "task 5.1", pero es la 4.2.
   - Builder coordination: dice "group B (frontend: 3)", pero ahora los layers son la sección 4, y la sección 3 mezcla Rust con el toast del frontend.
   - proposal § Impact: dice "no IPC change", pero `terminal:kitty-capped` es un evento nuevo de Tauri.
   - Título del scenario "Placing an unknown image is dropped": la acción ahora se reenvía (el THEN sobre el grid está bien).

5. **low: no está definido qué pasa con un transfer en curso después del cap.** Si el gate ya está en estado capped, debería descartar los chunks de inmediato y no decodificarlos hasta 64 MiB para después tirarlos. No afecta la cota de memoria, pero desperdicia CPU en base64. Conviene dejarlo escrito en D3/D4 y cubrirlo en el test "a capped gate drops".

6. **low: no está definido si el cap de deletes muestra el aviso y bloquea las transmisiones.** Hoy dice "if anything would pass a cap … set `capped`", así que llegar al cap de deletes también pausaría las imágenes. Es razonable, pero hay que decirlo para que el test de `take_capped_notice` no quede ambiguo.

Con estos seis puntos no hace falta otra ronda de plan. Los #1-#3 conviene arreglarlos en los artefactos antes de bloquear los tests Red.

VERDICT: APPROVED
