## Why

El panel de diff actualmente ofrece edit directo sobre los cambios activos, lo que rompe el modelo "user as reviewer" de cluihud: el usuario se convierte en co-autor silencioso y Claude no se entera del *por qué* de la corrección. Los code reviews en GitHub resuelven esto con comentarios anclados a línea y *suggestions* agrupados en un review batch. Queremos replicar ese patrón en el panel de diff, con soporte completo de teclado (equivalente al annotation mode de planes/specs) además de mouse.

## What Changes

- Nuevo modo **annotation** en el panel de diff, activable vía botón o shortcut, que permite agregar comentarios anclados a línea (single-line y multi-line) sobre cualquier hunk del diff
- Navegación por teclado entre líneas del diff (j/k o flechas), selección de rango con Shift, abrir input de comentario con una tecla dedicada (ej. `c`) — paridad funcional con el uso de mouse
- Soporte de dos tipos de anotación para el MVP: **Comment** (nota anclada) y **Suggestion** (bloque con reemplazo propuesto de código)
- Review batch: las anotaciones se acumulan en el panel y se envían juntas al hacer click en **"Submit review"**, que inyecta el feedback estructurado vía `UserPromptSubmit` hook
- El modo edit directo permanece disponible detrás de un botón secundario como escape hatch
- **BREAKING**: el comportamiento default del panel de diff cambia de *edit* a *read-only diff*

## Capabilities

### New Capabilities
- `diff-annotations`: modo de review sobre diffs con anotaciones ancladas a línea (comment, suggestion), navegación por teclado, y envío como review batch a Claude

### Modified Capabilities
<!-- ninguna -->

## Impact

- **Frontend**: refactor del panel de diff (`src/components/**/DiffView*` o equivalente) para renderizar con gutters interactivos, handlers de teclado, y estado de review batch
- **State**: nuevo atom `diffAnnotationsAtom` keyed por `diff:<filePath>:<side>` para anotaciones de diff, separado del `annotationsAtom` de markdown (el modelo de anclaje es distinto: línea vs DOM node)
- **Shortcuts**: nuevos shortcuts contextuales al panel de diff (toggle annotate, navegar líneas, agregar comment, submit review) — verificar colisiones en `stores/shortcuts.ts`
- **Backend**: reutilización de `inject-edits` / `pending_annotations` con un nuevo formato de payload que incluye `filePath`, `line`, `side`, `body`, `kind`, y snippet de contexto
- **No cambios en DB**: MVP in-memory only
