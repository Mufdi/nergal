## Context

Plan annotations es el feature más diferenciador de cluihud — ningún otro wrapper tiene edición bidireccional de planes con feedback estructurado a Claude. La implementación actual (v1) tiene bugs fundamentales:

1. **`surroundContents()` falla** cuando la selección cruza boundaries de elementos DOM (e.g., seleccionar texto que abarca `<strong>` y texto normal)
2. **La selección no persiste** visualmente — al soltar el mouse el highlight desaparece porque el catch block no wrappea nada
3. **Hover CSS interfiere** con pinpoint click — el `:hover` outline compite con `[data-pinpoint-active]`
4. **Annotations no persisten** — son in-memory (Jotai atoms), se pierden al recargar
5. **Revise flow inyecta feedback como HTML comment** embebido en el plan file, en vez de usar el hook `UserPromptSubmit` + `inject-edits`

### Código actual relevante
- `AnnotatableMarkdownView.tsx` — componente principal, 312 LOC
- `PlanAnnotationToolbar.tsx` — toolbar flotante + footer con Approve/Revise
- `stores/annotations.ts` — Jotai atoms (ephemeral), `serializeAnnotations()`
- `globals.css` líneas 199-280 — estilos de hover, pinpoint, marks
- `hooks/cli.rs::inject_edits()` — lee `HookState::take_pending_edit()`, inyecta instrucción en stdin JSON

## Goals / Non-Goals

**Goals:**
- Fix text selection para que persista visualmente (wrappear text nodes individuales, no `surroundContents()`)
- Fix pinpoint click sin interferencia de hover
- Persistir annotations en SQLite por sesión
- Fix revise flow: serializar annotations e inyectar via `inject-edits` hook (no como HTML comment)
- Restaurar annotations guardadas al abrir un plan

**Non-Goals:**
- MDXEditor integration (fase futura, requiere rewrite del editor)
- Annotation collaboration (es single-user)
- Annotation threading/replies
- Diff annotations (solo plan view)

## Decisions

### D1: Wrappear text nodes individuales en vez de `surroundContents()`

**Decisión**: Usar TreeWalker para iterar text nodes dentro del Range y wrappear cada uno en `<mark>` individualmente.

**Alternativas consideradas**:
- A) `range.surroundContents()` (actual) — Falla con `InvalidStateError` cuando el range cruza element boundaries. Es la causa raíz del bug de selección.
- B) `range.extractContents()` + wrap + `range.insertNode()` — Funciona cross-element pero destruye el DOM tree de React-managed nodes, causando inconsistencias en re-renders.
- C) **TreeWalker per-text-node wrapping** (elegida) — Itera los text nodes dentro del range, calcula offset start/end para cada uno, y wrappea con `<mark>`. No destruye estructura DOM, funciona cross-element.

**Trade-off**: C es más código (~30 LOC helper) pero es el único approach robusto. Plannotator usa este mismo patrón.

### D2: Hover ≠ Pinpoint — separación de estados CSS

**Decisión**: Cuando `[data-pinpoint-active]` existe en el DOM, deshabilitar hover outlines globalmente via CSS `body.has-pinpoint .annotatable-el:hover { outline: none }`.

**Alternativas consideradas**:
- A) `pointer-events: none` en hover — bloquea toda interacción
- B) CSS specificity war con `!important` (actual) — Frágil, el hover outline parpadea
- C) **Body class toggle** (elegida) — Al setear `data-pinpoint-active`, agregar `has-pinpoint` al body. CSS desactiva hover. Al cerrar toolbar, remover la class.

### D3: Persistencia en SQLite

**Decisión**: Nueva tabla `annotations` con CRUD via comandos Tauri. Sync bidireccional: Jotai atoms se sincronizan con backend al crear/eliminar annotations.

**Alternativas consideradas**:
- A) Solo Jotai (actual) — Se pierden al recargar
- B) localStorage — No compartido entre ventanas Tauri, no queryable
- C) **SQLite** (elegida) — Consistente con el resto del backend (sessions, etc.), queryable, persiste

**Schema**:
```sql
CREATE TABLE annotations (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK(type IN ('comment','replace','delete','insert')),
  target TEXT NOT NULL,
  content TEXT NOT NULL DEFAULT '',
  position_start INTEGER NOT NULL DEFAULT 0,
  position_end INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_annotations_session ON annotations(session_id);
```

### D4: Revise flow via `inject-edits` hook

**Decisión**: Al clickear "Revise", guardar las annotations serializadas en `HookState` (similar a `pending_edit`). El hook `inject-edits` en `UserPromptSubmit` las lee y las inyecta en el prompt de Claude.

**Alternativas consideradas**:
- A) Embeber feedback como HTML comment en el plan file (actual) — Claude puede ignorar los comments, contamina el archivo
- B) Escribir feedback en archivo separado y referenciar en inject — Más archivos, más complejidad
- C) **Inyectar directamente via `inject-edits`** (elegida) — El hook ya existe para plan edits. Extenderlo para también inyectar annotation feedback como texto plano en el prompt. Más directo, Claude lo ve como instrucción del usuario.

**Implementación**: Agregar `pending_annotations: Option<String>` a `HookState`. `inject_edits()` chequea ambos: `pending_edit` (plan file edits) y `pending_annotations` (structured feedback). Ambos se inyectan en el mismo prompt.

### D5: Annotation restore via DOM marks

**Decisión**: Al cargar annotations desde SQLite, usar la misma función de wrapping per-text-node para restaurar `<mark class="annotation-{type}">` en el DOM.

**Trade-off**: Si el plan content cambió desde que se creó la annotation, `findTextInContainer()` puede no encontrar el target. Aceptable — la annotation sigue en la sidebar list como referencia, simplemente no tiene highlight visual.

## Risks / Trade-offs

- **[DOM manipulation + React]** → Mitigación: Solo manipulamos el DOM dentro del container de `react-markdown` output. React no gestiona esos nodos internos directamente (son generados por el markdown renderer). El toolbar sigue siendo React state.
- **[Plan content change invalida annotations]** → Mitigación: Las annotations persisten por `target` text. Si Claude reescribe el plan, las annotations viejas quedan en sidebar pero sin highlight. El usuario puede limpiarlas.
- **[Performance con muchas annotations]** → Mitigación: TreeWalker es O(n) sobre text nodes. Para un plan típico (<500 nodos), imperceptible.

## Open Questions

- ¿Debemos limpiar automáticamente annotations cuando Claude envía un plan nuevo? (propuesta: sí, con toast de confirmación)

---

## Revision 1: Adoptar `web-highlighter` en vez de DOM manual

### Qué cambió

Las decisiones D1 (TreeWalker wrapping), D2 (body class hover), y D5 (annotation restore) se reemplazan por el uso de la librería [`web-highlighter`](https://github.com/alienzhou/web-highlighter). Las decisiones D3 (SQLite) y D4 (inject-edits) se mantienen sin cambios — ya están implementadas y son correctas.

### Por qué

La implementación manual de D1 tenía los mismos problemas que `surroundContents()`: nuestro `wrapRangeInMarks()` custom es frágil con edge cases de boundary splitting, y la restauración via text matching (`findTextInContainer`) es posicional — si el DOM cambia ligeramente, no encuentra el target.

Al investigar [plannotator](https://github.com/backnotprop/plannotator) — la referencia original del feature — descubrimos que no usa DOM manual. Usa `web-highlighter`, una librería de ~5KB que resuelve exactamente estos problemas:

1. **Cross-element selections**: `highlighter.fromRange(range)` wrappea cross-element sin errores
2. **Serialización posicional**: `HighlightSource` con `startMeta`/`endMeta` codifica la posición como DOM path + offset, sobrevive recargas incluso si el texto cambia ligeramente
3. **Restauración**: `highlighter.fromStore(startMeta, endMeta, text, id)` reconstruye marks desde datos serializados
4. **Eventos**: `CREATE`, `CLICK`, `HOVER`, `HOVER_OUT`, `REMOVE` — reemplaza nuestros event listeners manuales
5. **Pinpoint via `fromRange()`**: crear un Range programáticamente y pasarlo al highlighter, que dispara `CREATE`

### Decisiones revisadas

#### D1-R: `web-highlighter.fromRange()` en vez de wrapping manual

**Decisión**: Usar `web-highlighter` para toda la lógica de highlight — text selection y pinpoint.

**Alternativas re-evaluadas**:
- A) `surroundContents()` (v0) — Falla cross-element. Descartada.
- B) `wrapRangeInMarks()` custom (v1) — Funciona en teoría pero frágil en edge cases, requiere mantenimiento continuo, y la restauración por text matching es unreliable.
- C) **`web-highlighter`** (v2, elegida) — Librería probada, usada en producción por plannotator. Serialización posicional robusta. Zero-maintenance para nosotros.

**Trade-off**: Dependencia externa (~5KB). Aceptable — es MIT, sin dependencias propias, y elimina ~100 LOC de código propio propenso a bugs.

#### D2-R: Hover via JS mousemove + data attribute (plannotator pattern)

**Decisión**: Reemplazar el hover CSS `:hover` por detección JavaScript en `mousemove` que setea `data-pinpoint-hover` attribute. CSS estiliza ese attribute.

**Por qué**: Plannotator usa este patrón porque CSS `:hover` es stateless — no puede "desactivarse" cuando hay un pinpoint activo sin hacks de body classes. Con JS-driven hover, simplemente dejamos de actualizar el attribute cuando hay selección activa.

#### D5-R: Persistencia via `HighlightSource` en SQLite

**Decisión**: Almacenar `startMeta`, `endMeta`, y `text` de `web-highlighter` en la tabla `annotations` existente. Restaurar con `highlighter.fromStore()`.

**Cambio en schema**: Agregar columnas `start_meta TEXT` y `end_meta TEXT` (JSON serializado) a la tabla `annotations`. Las columnas `position_start`/`position_end` se reemplazan por estas — la posición DOM path es más robusta que offsets numéricos.

### Decisiones que NO cambian

- **D3 (SQLite persistence)** — Ya implementada, correcta. Solo cambia el formato de serialización posicional.
- **D4 (inject-edits hook)** — Ya implementada, correcta. El flujo de revise no cambia.

### Impacto del revert

Las siguientes tareas implementadas en v1 deben revertirse:
- `wrapRangeInMarks.ts` — eliminar (reemplazado por web-highlighter)
- `AnnotatableMarkdownView.tsx` — reescribir selection/pinpoint/restore con web-highlighter API
- `globals.css` — simplificar hover rules (ya no necesita body.has-pinpoint/has-selection)
- Schema migration — agregar columnas `start_meta`/`end_meta`

Las siguientes se mantienen:
- `003_annotations.sql` migration — base válida, se extiende
- `db.rs` CRUD methods — se ajustan para nuevas columnas
- `state.rs` HookState extension — sin cambios
- `cli.rs` inject_edits — sin cambios
- `commands.rs` Tauri commands — se ajustan signatures
- `stores/annotations.ts` Jotai sync — se ajusta el shape
- `PlanPanel.tsx` revise flow — sin cambios
