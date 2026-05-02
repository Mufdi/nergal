## Why

El user actualmente abre Sublime fuera de cluihud para anotar ideas, redactar prompts antes de enviarlos, dejar notas mentales para retomar al día siguiente — un buffer de scratch global, cross-project, cross-session. El flow rompe la inmersión en cluihud: cambia de ventana, pierde el contexto visual del workspace, y el día que se cierra Sublime las notas se mezclan con cualquier otro buffer abierto. La razón histórica era que Claude Code no ofrecía un scratchpad in situ; cluihud puede cerrar esa fricción.

El scratchpad **no es** un editor de notas (eso es Obsidian), no es per-session (eso es el plan annotator), no es per-project (eso es `.claude/notes/`). Es **un buffer global de escritura efímera** que sobrevive entre reinicios y al cual se entra y se sale con un toggle, sin nav, sin friction.

## What Changes

- Nuevo **floating panel** in-cluihud (no ventana secundaria, no Tauri window separada) con z-index alto, posicionable y resizable, que flota encima del workspace **sin bloquear interacción** con los panels detrás (variante de §3.9 popover-as-modal sin backdrop dim).
- **Multi-tab** dentro del panel, cada tab respaldada por un `.md` real en disco. Botón `+` agrega tab, botón close por tab elimina (soft-delete).
- **Tabs numeradas por posición**: "Scratch 1", "Scratch 2", … recomputado dinámicamente. El display name es derivación del orden, no del filename. **Filename estable**: `scratch-{uuid}.md` (UUID v4 embedded). El UUID es el `tab_id` y sobrevive a renames externos, cambios de path, y sync con Obsidian.
- **Path configurable** en settings (`scratchpadPath`, default `~/.config/cluihud/scratchpad/`). El day-after se puede apuntar a un Obsidian vault sin tocar código. **Cambiar el path**:
  1. Hace `flush()` síncrono: await de cualquier autosave en flight + force de la cola debounced.
  2. Cierra todas las tabs abiertas en memoria.
  3. **Borra todas las rows de `scratchpad_meta`** (clean slate; metadata vieja no aplica al path nuevo).
  4. Re-apunta el watcher al nuevo path y re-listea para repoblar `scratchpad_meta` desde los archivos que matchean el patrón en el nuevo dir.
  5. Si el user revierte al path anterior, las notas reaparecen pero sin el orden previo (la reconstrucción usa `ctime` o filesystem order). Trade-off explícito: el orden no se persiste cross-path.
- **Soft-delete** con `.trash/` dentro del scratchpad dir. Al borrar, el archivo se renombra a `scratch-{uuid}-trashed-{epoch_ms}.md` para embedir el timestamp de eliminación (en **milisegundos** Unix epoch) en el filename. Esto sobrevive a `mtime` quirks/clock skew y evita colisiones cuando varias notas se trasean dentro del mismo segundo. Purga automática en startup de archivos con `epoch_ms` correspondiente a `> 30 días`. Sin UI de papelera en v1.
- **Background semi-transparente** vía `rgba(10, 10, 11, 0.9)` (= `--card` `#0a0a0b` con alpha 0.9). **No introduce un cuarto tier de surface** (DESIGN.md §6.1 anti-pattern); reusa `--card` con transparencia. Sin `backdrop-filter` (no garantizable en WebKitGTK Linux).
- **Editor**: CodeMirror 6 con highlight de markdown opcional. Sin preview, sin annotations, sin highlighter — plain text editing prioritario sobre features.
- **Autosave debounced 300ms** con escritura atómica `tmp + rename` **en el mismo directorio que el target** (`.scratch-{uuid}.md.tmp` → `scratch-{uuid}.md`). Esto garantiza atomicidad de `rename(2)` aunque `scratchpadPath` esté en un filesystem distinto al de `/tmp`. Cleanup de `.tmp` huérfanos en startup (residuos de crashes).
- **Watcher** sobre `scratchpadPath` (notify-debouncer-full, 200ms), filtra **solo** archivos que matchean la regex literal `^scratch-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$` (UUID v4 hex con guiones, lowercase, sin verificar version-nibble — más laxo pero permite UUIDs ya generados sin riesgo). Esta regex literal es el contrato compartido con `scratchpad_create_tab` para evitar drift. Excluye explícitamente: `.trash/`, cualquier dotfile (incluido `.scratch-{uuid}.md.tmp` durante renames slow), y archivos > 1 MB (cap defensivo: leer y hashear 10MB en el handler bloquearía el watcher loop). Apuntar el path a un Obsidian vault no muestra notas no-scratchpad. **Own-write tracking via content hash con ring buffer**: cada escritura del editor calcula y agrega el SHA-256 a un ring buffer per-file (last N=8 hashes); cuando llega un notify event, se lee el file y se compara su hash contra el set; si matchea **cualquiera** de los últimos 8 propios → ignorar como own-write. Esto absorbe re-ordering y coalescing de notify events en FUSE/Syncthing/iCloud sin race spurious. Si un tab se edita externamente y el buffer está limpio, refresca; si está dirty, marca conflicto suave (badge visual, no bloqueante).
- **Toggle Ctrl+Alt+L** para mostrar/ocultar (Ctrl+L se evita: el terminal lo usa para `clear-screen` y `attachCustomKeyEventHandler` lo filtraría rompiendo el flow nativo del PTY). Esc oculta cuando el panel está enfocado, con `stopPropagation` y containment check en `event.target` para no colisionar con `AskUserModal`, command palette, plan modal o conflicts dialog. Posición y tamaño del panel persisten en SQLite.
- **Persistencia de UI metadata** (tab activa, orden, posición + tamaño del panel, last-opened) en SQLite vía `db.rs`. Contenido siempre en files; metadata en DB.
- **Sin coupling con sesiones, workspaces, o panels existentes**. El scratchpad vive en su propio store global, no usa `activeSessionIdAtom` ni tabs del right panel.

**No-goals v1**: sync con Obsidian, custom naming, send-to-prompt, multi-instance, recently-closed, drag-reorder de tabs, search en notas. Todos quedan deliberadamente fuera, pero el diseño los habilita (ver §Extensibility hooks).

## Capabilities

### New Capabilities

- `scratchpad`: floating panel in-cluihud para notas rápidas globales, con multi-tab respaldado por archivos `.md` en disco, autosave atómico, soft-delete a `.trash/`, y settings de path configurable.

### Modified Capabilities

- `keyboard-shortcuts`: agrega `scratchpad.toggle` (Ctrl+Alt+L). El handler vive en `stores/shortcuts.ts` y solo dispara el toggle del panel; no interactúa con focus de otras zonas. Ctrl+L se preserva para el PTY (`clear-screen`).

## Impact

- **Frontend (nuevo módulo)**:
  - `src/components/floating/FloatingPanel.tsx` — **wrapper genérico reutilizable** (load-bearing, no aspirational): drag header, resize handles, opacity background, close button, persistencia de geometría keyed por `panelId`. Acepta `children`. ScratchpadPanel lo consume desde día uno.
  - `src/components/scratchpad/ScratchpadPanel.tsx` — usa `<FloatingPanel panelId="scratchpad">` y aloja TabBar + Editor.
  - `src/components/scratchpad/ScratchpadTabBar.tsx` — barra de tabs numeradas con `+`, close-per-tab. Componente independiente de `TabBar` del right panel (modelo distinto: tabs efímeras, no documents). Si la cantidad de tabs excede el ancho disponible, scroll horizontal con sombra de overflow (no truncate, no wrap). Hard cap suave: 50 tabs, badge de aviso al alcanzar.
  - `src/components/scratchpad/ScratchpadEditor.tsx` — wrapper sobre CodeMirror 6 con autosave hook. Expone `currentScratchpadSelectionAtom` (actualizado en cada cambio de selección via `EditorView.updateListener`) — esto destraba el send-to-prompt adapter futuro sin requerir tocar el editor.
  - `src/stores/scratchpad.ts` — atoms para tabs list, active tab, panel geometry, dirty state, content buffers, `currentScratchpadSelectionAtom`.

- **Backend (Rust)**:
  - `src-tauri/src/scratchpad/mod.rs` — submodule con FS ops (read, write, list, soft-delete, purge), watcher integration.
  - **Path validation** (en **toda** operación: read, write, list, delete, watcher event handling): canonicalize tanto `scratchpadPath` como el target file path; assert `target.starts_with(canonical_scratchpadPath)`; **refuse symlinks en todas las ops** (`metadata().is_symlink()` previo a cualquier read/write/list/emit). Si un symlink aparece en la dir (vía Obsidian plugin u otra herramienta), `scratchpad_list_tabs` lo omite y el watcher ignora sus eventos. Esto previene path traversal y disclosure de info (ej. symlink a `~/.ssh/id_rsa`).
  - Nuevos commands en `commands.rs`: `scratchpad_list_tabs`, `scratchpad_read_tab`, `scratchpad_write_tab`, `scratchpad_create_tab`, `scratchpad_close_tab`, `scratchpad_set_path`, `scratchpad_get_geometry`, `scratchpad_set_geometry`, `scratchpad_reveal_in_file_manager` (vía `Command::new("xdg-open").arg(canonical_path)` — args, no shell).
  - DB migration (versión bump en `db.rs` siguiendo el pattern de migrations existente; forward-only):
    - `scratchpad_meta(tab_id TEXT PRIMARY KEY, position INTEGER NOT NULL, created_at INTEGER NOT NULL, last_modified INTEGER NOT NULL)` — `tab_id` = UUID v4, mismo que el del filename. Path-independent.
    - `floating_panel_geometry(panel_id TEXT PRIMARY KEY, geometry_json TEXT NOT NULL, opacity REAL NOT NULL DEFAULT 0.9)` — multi-row keyed por panel para soportar futuros floating tools. Scratchpad usa `panel_id = 'scratchpad'`. **Nota**: este es el único nombre canónico de la tabla en todo el proposal.
  - Watcher: notify-debouncer-full sobre `scratchpadPath` con eventos emitidos al frontend (`scratchpad:tab-changed`, `scratchpad:tab-deleted-externally`).

- **Settings**:
  - Nueva key `scratchpadPath` en `config.rs` (default resolvable via `dirs` crate, `~/.config/cluihud/scratchpad/`).
  - Entrada en `SettingsPanel.tsx` con input de path + button "Reveal" (abre el folder en file manager).
  - Cambiar el path **no migra** archivos: re-apunta y muestra toast informativo. Las notas anteriores quedan en su path antiguo.

- **Shortcuts**: **Ctrl+Alt+L** registrado en `stores/shortcuts.ts` (verificado libre — Ctrl+L se descarta por colisión con `clear-screen` del PTY/terminal). Handler con containment check sobre el árbol del FloatingPanel para que Esc solo cierre cuando el panel está enfocado y no compita con otros consumers de Esc (modals, palette, conflicts dialog).

- **Pointer-events del floating panel**: el card visible (`bg-[rgba(10,10,11,0.9)]`) tiene `pointer-events: auto`; el shadow/margen exterior tiene `pointer-events: none` para que clicks fuera del chrome pasen al workspace debajo.

- **Off-screen rescue**: al cargar la geometría persistida, si las coords están fuera del viewport actual (multi-monitor disconnect, resize agresivo, fractional scaling de Wayland), clamp + reset a centered default. Evita panel inalcanzable.

- **Empty-state y dir-missing behavior** (cubre los dos casos comunes de fallo):
  - **First-launch sin `scratchpadPath` configurado**: crear el directorio silenciosamente al primer toggle.
  - **`scratchpadPath` configurado pero el dir no existe** (typo en settings, dir borrado entre sesiones, vault desmontado): en `scratchpad_set_path` y en cada toggle, `mkdir -p` el path antes de listar/watch. Si la creación falla (permission denied, parent missing, mount unavailable) → toast con error específico + revert al path anterior si lo había.
  - **`scratchpadPath` borrado externamente con panel abierto**: watcher detecta delete del root; toast informativo + ofrecer `recreate` o `change path`.
  - Sin tabs: panel muestra "No notes yet — press +" centrado (`text-xs text-muted-foreground` per DESIGN.md §7.4).

- **Spec scaffold**: `openspec/specs/scratchpad/spec.md` se crea durante archive (`/openspec-sync archive`), no pre-implementación. Conviene con la convención del repo.

- **No cambia**: terminal, panels existentes, plan flow, sessions, Claude hook flow, git panel. Cero coupling.

## Sprint Contract

### Qué construyo

1. Floating panel React component con drag/resize/opacity, persistente en SQLite.
2. Multi-tab system con `+`, close-per-tab, naming dinámico por posición.
3. CodeMirror 6 editor con autosave debounced atómico.
4. Backend Tauri commands para CRUD de tabs sobre filesystem.
5. Soft-delete a `.trash/` + purga automática 30 días en startup.
6. Watcher sobre scratchpadPath para refresh de cambios externos.
7. Setting `scratchpadPath` en SettingsPanel con default `~/.config/cluihud/scratchpad/`.
8. Shortcut Ctrl+Alt+L (toggle) + Esc (close-when-focused, con containment check).

### Cómo verifico

```bash
cd src-tauri && cargo clippy -- -D warnings
cd src-tauri && cargo test
cd src-tauri && cargo fmt --check
npx tsc --noEmit
pnpm dev   # smoke: Ctrl+L → panel aparece, escribir → file aparece en disk, Ctrl+L → desaparece, reabrir cluihud → contenido persiste
```

Verificación manual mínima:
- Crear 3 tabs, escribir en cada una, cerrar la del medio → renumeración a "Scratch 1" + "Scratch 2".
- Cerrar tab → archivo se mueve a `.trash/`.
- Editar `.md` con vim mientras cluihud corre → buffer refresca si está limpio, marca conflicto si está dirty.
- Cerrar cluihud y reabrir → tabs persisten, posición y tamaño del panel persisten.
- Cambiar `scratchpadPath` en settings → tabs antiguas desaparecen, tabs del nuevo path aparecen.
- Test de purga: crear archivo en `.trash/` con epoch embebido en filename `> 30d`, abrir cluihud → archivo eliminado y toast informativo. Crear otro con epoch reciente → preservado.

### Criterio de done

- Ctrl+Alt+L muestra/oculta el panel sin colisiones con shortcuts existentes ni con el `clear-screen` del PTY (Ctrl+L sigue limpiando la terminal).
- Tabs persisten entre reinicios.
- Autosave nunca pierde datos en crash (verificado matando el proceso mid-edit y re-abriendo).
- Soft-delete recuperable manualmente desde `.trash/` por 30 días.
- Cambiar path en settings no rompe ni mueve archivos existentes.
- Cero coupling: deshabilitar el módulo de scratchpad no rompe nada en otros panels.
- Verificación full pasa: `cargo clippy -D warnings`, `cargo test`, `tsc --noEmit`.

### Estimated scope

- files_estimate: 12
- risk_tier: low
- tags: [feature]
- visibility: private
- spec_target: scratchpad

## Extensibility hooks

Decisiones explícitas que destraban features futuras **sin breaking changes**:

### Stable identity (provider abstraction deferred)

> **Honest framing**: en v1 NO hay storage abstraction; los Tauri commands hablan directo con filesystem. Lo que sí está construido es **identidad estable** vía UUID, que es el prerequisito para que una abstraction futura no rompa metadata. La abstraction misma sería un refactor cuando se construya.

El contenido vive en `.md` files; la metadata en SQLite. **Identidad estable** = UUID v4 embedded en filename (`scratch-{uuid}.md`) y como `tab_id` en DB. Esta separación + identidad permite:

- **Sync con Obsidian** sin migración: cambiar `scratchpadPath` a un folder dentro del vault. Los archivos siguen siendo `.md` standard, Obsidian los abre nativo. El UUID en el filename es feo para humanos pero estable contra renames; el display name "Scratch N" lo abstrae visualmente.
- **Renames externos**: si Obsidian renombra `scratch-{uuid}.md` a otro filename que **sigue matcheando** el patrón `scratch-{uuid}.md` (rename a un UUID distinto), el watcher trata el viejo como delete y el nuevo como create. Si el rename es a un filename **fuera del patrón** (ej. `01 - my scratch.md`), el archivo se vuelve invisible al scratchpad: el watcher lo ignora, la row de DB queda dangling. **Comportamiento explícito**: en el próximo emit del watcher, las rows de `scratchpad_meta` cuyo UUID no aparece en el listing actual se borran (zombie cleanup). El user pierde la nota desde la perspectiva de cluihud, pero el file sigue en disco. No es bidireccional con renames de Obsidian; es un trade-off explícito de v1.
- **Drop-in de provider alternativo (future shape, NOT implemented in v1)**: si en el futuro se quiere agregar provider tipo Linear, la dirección sería extraer un `Provider` interface frontend-side con métodos `list / read / write / delete / watch`, y tener `LocalMdProvider` (v1) como una implementación. Esto **sí** sería un refactor, no un drop-in. Lo registramos honestamente como dirección probable, no como hook listo en v1. La UI actual habla directo con los Tauri commands de filesystem.

### Naming layer desacoplado del filesystem

El display name "Scratch N" es **función pura del orden** en runtime, no del filename. El filename interno es estable (UUID o timestamp). Esto permite:

- **Custom naming** futuro vía migration aditiva low-risk: `ALTER TABLE scratchpad_meta ADD COLUMN custom_name TEXT NULL`, mostrar `custom_name ?? "Scratch ${position}"`. Los archivos no cambian. **No es "sin migración"**; es una migration de una sola línea, forward-compatible.
- **Drag-reorder** sin renombrar archivos: solo se actualiza `position` en la DB.
- **Pin de tabs** (eventual): agregar columna `pinned BOOLEAN`, ordenar pinned-first.

### Floating panel chrome reutilizable (load-bearing)

`ScratchpadPanel.tsx` se construye sobre un wrapper genérico `<FloatingPanel panelId="scratchpad" />` que acepta children + persiste geometría/opacity en `floating_panel_geometry` keyed por `panel_id`. Schema desde día 1 ya es multi-row, no singleton. Si se agrega un segundo "floating tool" (ej. mini terminal flotante, calculadora) usa `panelId="terminal-mini"` y persiste su propia geometría sin migración SQL.

### Watcher como base de sync

`notify-debouncer-full` sobre `scratchpadPath` se implementa para hot-reload de edits externos, con own-write tracking para evitar loops. Esto destraba:

- **Sync con Obsidian read-mostly**: vault apuntado, edits externos refrescan tabs limpios, conflictos suaves (badge) si dirty.
- **Multi-instance read-mostly**: dos cluihud sobre el mismo path se ven los cambios del otro vía notify. **Concurrent writes no son seguros en v1** (race en `tmp + rename`); se requiere file lock o merge protocol para v2 si el caso aparece. El watcher es la base, no la solución completa.
- **Recently-closed** UI: leer `.trash/`, listar archivos con `epoch > now - 30d`, mostrar como "Restore". El epoch en el filename hace esto trivial sin DB.

### Send-to-prompt deliberadamente fuera de scope, pero habilitado (sin romper no-coupling)

El editor expone `currentScratchpadSelectionAtom` desde día uno (actualizado en cada cambio de selección via `EditorView.updateListener`). Esta es la **seam concreta** que destraba el adapter futuro: el store del scratchpad publica selection state, y un adapter file separado (`src/components/scratchpad/sendToPromptAdapter.ts`) lo compone con `activeSessionIdAtom`:

```ts
const selection = get(currentScratchpadSelectionAtom);
const sessionId = get(activeSessionIdAtom);
if (sessionId && selection) invoke("terminal_input", { sessionId, text: selection });
```

El store del scratchpad nunca importa estado de sesión; el adapter sí. Esto preserva "scratchpad puro y global" mientras habilita la integración como composición opcional, y **no** requiere tocar `ScratchpadEditor.tsx` el día que se construya.

### Settings hook estándar

`scratchpadPath` se agrega usando el patrón existente de `config.rs` y `SettingsPanel.tsx`. Cuando se agreguen settings de scratchpad futuros (purga days, opacity level, default tab content), siguen el mismo patrón sin breaking.

### Soft-delete como convención de carpetas

`.trash/` con purga 30d es una convención de filesystem, no de schema. Si en el futuro se agrega:

- **Restore UI**: leer `.trash/`, listar, mover de vuelta.
- **Configurable retention**: setting `scratchpadTrashDays`, lee mismo `.trash/`.
- **Disable trash**: setting boolean, delete duro skipea el `mv` a `.trash/`.

Ninguno requiere migración.

### No coupling con session/workspace state

El store `scratchpad.ts` no importa `activeSessionIdAtom`, `activeWorkspaceAtom`, ni nada del tab system del right panel. Esto preserva la propiedad "global, cross-project, cross-session" sin posibilidad de regresión accidental.

## Deferred (acceptable risks for v1)

Issues levantados por iprev round 3, evaluados como aceptables para shipping v1:

- **Frontend test coverage narrow**: solo tests Rust (FS ops) + 1 regression test de teclado (Ctrl+L → PTY). No hay React component tests para geometry persistence, soft-delete UI, watcher event handling. Trade-off: cluihud es uso personal, las suites de tests de frontend no están establecidas en el repo, agregar Vitest config para esto solo aumenta scope sin valor proporcional. Re-evaluar si la feature gana usuarios externos.
- **Off-screen rescue solo en geometry load**: si el user desconecta un monitor mid-session, el panel queda inalcanzable hasta el próximo restart. El clamp solo corre al cargar; un listener de `resize` / display-change re-clamp es nice-to-have pero no crítico. Defer.

## Open questions

Ninguna bloqueante. Decisiones reversibles si el feedback de uso lo justifica:

1. **Default opacity 0.9 vs slider en settings**: arrancamos con valor fijo. Si el user pide ajuste, agregar setting `scratchpadOpacity` (extensión sin breaking, ya hay column en `floating_panel_geometry`).
2. **Tab order: por modificación reciente vs por creación**: arranco con **orden de creación** (más predecible, menos visual jitter). Cambiar a "modificación reciente" después es solo un `ORDER BY` distinto.
3. **Watcher en initial dir solo, o re-watch en cambio de path**: re-watch (más simple mentalmente, costo trivial).
4. **Renumbering trade-off acknowledged**: cerrar Scratch 2 → Scratch 3 pasa a ser Scratch 2. El user perdió la asociación mental "mi idea está en Scratch 3". Decisión de diseño tomada en conversación: el scratchpad es **efímero por diseño**, sin identidad estable. Si la identidad mental empieza a doler en uso real, el switch a "creation order con gaps" (Scratch 1, Scratch 4, Scratch 7) es trivial post-launch (solo cambia la fórmula del display name).

## Implementation steps (high-level, refinable en Mode B)

1. **Backend FS ops**: módulo `scratchpad/mod.rs` con read, write (atomic via `tmp + rename`), list, create (UUID v4 + filename), soft-delete (rename con epoch embedded). Tests unitarios sobre tmpdir cubriendo: (a) `tmp + rename` atomicidad bajo simulación de partial-write, (b) `mv` a `.trash/` con epoch correcto en filename, (c) purga lee epoch del filename, ignora `mtime`, (d) watcher: un notify event cuyo hash está en el ring buffer per-file (own-write set) no dispara reload — un notify event con hash distinto sí lo dispara.
2. **DB schema**: nueva migration `migrations/006_scratchpad.sql` (current head: v5) creando `scratchpad_meta` + `floating_panel_geometry`. Reusar pattern de migrations existentes (registro en el array de `db.rs`, version bump automático vía índice).
3. **Tauri commands**: registrar en `lib.rs`, validar paths (no escape de `scratchpadPath`).
4. **Watcher**: notify integration con eventos emitidos. Debounce 200ms. **Re-watch en path change** sigue ordering estricto: (a) ensure new dir exists (`mkdir -p`) — si falla, abort + toast + revert; (b) drop watcher viejo; (c) attach watcher nuevo; (d) emit listing inicial. Falla en cualquier paso → revert al path anterior, no estado mixto.
5. **Frontend store**: `stores/scratchpad.ts` con atoms + listeners de eventos backend.
6. **FloatingPanel chrome**: drag, resize, opacity, geometry persistence.
7. **TabBar component**: render de tabs + `+` + close. Renumeración dinámica.
8. **CodeMirror integration**: editor wrapper, autosave hook, dirty tracking.
9. **Settings UI**: input de path + reveal button. Re-apunta sin migrar.
10. **Shortcut Ctrl+Alt+L**: registrar en `shortcuts.ts`, bindear toggle. Verificar que `attachCustomKeyEventHandler` del terminal **no** intercepte Ctrl+Alt+L (debe seguir intercepting Ctrl+L → PTY). **Regression test**: agregar a la suite de tests automáticos del frontend un test que valida que en el handler del terminal, una key event `Ctrl+L` retorna `false` (no swallow) y `Ctrl+Alt+L` retorna `false` o no llega al PTY. Esto previene regresión silenciosa de `clear-screen`.
11. **Purga 30d**: tarea en startup que escanea `.trash/` y borra archivos cuyo **epoch embebido en el filename** (`scratch-{uuid}-trashed-{epoch}.md`) es `< now - 30 días`. **No usa `mtime`** (que persiste en `mv` y daría false positives si el file original era viejo). Toast informativo "Cleared N old scratchpad notes" si N > 0.
12. **Verify full**: clippy, test, tsc, smoke manual.
