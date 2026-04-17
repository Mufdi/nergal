## Context

El panel de diff es hoy el punto de fricción más grande del flujo: cuando Claude produce cambios, el usuario revisa y, si algo está mal, edita directamente el archivo o le escribe al terminal desacoplado del contexto visual. Los code reviews en GitHub resolvieron este patrón hace años: comentarios anclados a línea, agrupados en un review batch, con tipos explícitos (Comment vs Suggestion). Queremos adoptar ese patrón y, además, darle **paridad completa con teclado** — igual que los annotation modes de planes y specs — porque el usuario trabaja primariamente con teclado y xterm.js. A diferencia de los markdown annotations (ancladas a nodos DOM vía `web-highlighter`), las anotaciones de diff se anclan a **líneas específicas** (`{filePath, side, line}`), lo que requiere un sistema de storage y rendering distinto.

## Goals / Non-Goals

**Goals:**
- Default read-only del panel de diff
- Annotation mode toggleable vía botón y shortcut
- Anotaciones single-line y multi-line ancladas a `{filePath, side, line, endLine?}`
- Tipos MVP: `comment` y `suggestion`
- Navegación completa por teclado: mover focus de línea, seleccionar rango, abrir input, submit
- Review batch: varias anotaciones se acumulan y se envían en un solo mensaje al hacer "Submit review"
- Envío vía `inject-edits` (`UserPromptSubmit`) reutilizando `pending_annotations`
- Mantener edit directo como escape hatch

**Non-Goals:**
- Persistencia de anotaciones de diff entre sesiones (MVP in-memory)
- Threading de comentarios o respuestas
- Resolución de comentarios (resolved/unresolved state)
- Sync con GitHub/GitLab PR reviews
- Modo split vs unified configurable (usar lo que ya existe)
- Rich text en comentarios (solo markdown plain)

## Decisions

### Decision 1: Storage separado de `annotationsAtom` de markdown
Los markdown annotations usan posiciones DOM (web-highlighter). Los diff annotations usan coordenadas `{filePath, side, line}`. Unificar ambos modelos es contraproducente porque las operaciones (render, clear, restore) divergen completamente. Decisión: nuevo atom `diffAnnotationsAtom: Record<string, DiffAnnotation[]>` keyed por `diff:<filePath>`.

```ts
type DiffAnnotation = {
  id: string;
  filePath: string;
  side: 'before' | 'after';
  startLine: number;
  endLine: number;
  kind: 'comment' | 'suggestion';
  body: string;
  suggestionReplacement?: string; // only for kind='suggestion'
  createdAt: number;
};
```

**Alternatives considered:**
- *Unificar con `annotationsAtom`*: descartado — el modelo de anclaje no es compatible
- *Tabla SQLite desde MVP*: descartado — YAGNI; in-memory suficiente

### Decision 2: Anclaje por línea, no por texto seleccionado
GitHub PR reviews anclan a línea (no a selección de texto). Esto se alinea con cómo se lee diff: línea por línea. Multi-line annotations se crean con Shift+click en la línea final o con Shift+j/k en teclado. El `endLine` es opcional; ausencia implica single-line.

**Alternatives considered:**
- *Anclaje por texto seleccionado dentro de una línea*: descartado — complica la serialización y no aporta vs comentario sobre la línea completa
- *Anclaje por hunk entero*: descartado — demasiado grueso

### Decision 3: Paridad teclado completa con modelo "focused line"
En annotation mode, una línea siempre tiene focus (renderizada con un borde lateral). El usuario navega con:
- `j` / `k` o `↓` / `↑`: mover focus a siguiente/anterior línea
- `Shift+j` / `Shift+k`: extender rango de selección
- `c`: abrir input de comentario sobre la línea/rango focused
- `s`: abrir input de suggestion sobre la línea/rango focused (con preview del código a reemplazar)
- `Enter` dentro del input: submit anotación
- `Escape`: cerrar input / limpiar selección / salir de annotation mode (en este orden)
- `Ctrl+Enter`: submit review batch

El mouse funciona en paralelo: click en gutter = focus + abrir input; shift+click = rango.

**Alternatives considered:**
- *Solo mouse*: descartado explícitamente por el usuario
- *Vim-style con más bindings*: descartado — MVP minimalista; se puede expandir después

### Decision 4: Review batch con submit explícito
Las anotaciones no se envían una por una. Se acumulan en un panel lateral o footer ("3 comments pending") y el usuario hace click en "Submit review" para enviarlas todas juntas. Esto replica el flujo de GitHub y evita spamear a Claude con mensajes individuales.

Al hacer submit:
1. Se serializa el batch completo (agrupado por archivo, en orden de línea)
2. Se escribe a `HookState.pending_annotations` con source = "diff-review"
3. Se limpian las anotaciones locales
4. Toast: "Review submitted — será enviado en tu próximo mensaje"
5. El próximo `UserPromptSubmit` lo inyecta

Formato serializado:
```
Code review del diff pendiente:

## src/foo.ts

**[comment] line 42:**
Esto podría romper X porque...

**[suggestion] lines 58-60:**
Reemplazar por:
```ts
const result = await fetchSafe(url);
```

## src/bar.ts
...
```

**Alternatives considered:**
- *Enviar cada comentario inmediatamente*: descartado — ruidoso y rompe el patrón de PR review
- *Botón individual "Send" por comentario*: descartado — misma razón

### Decision 5: Gutter interactivo como superficie principal
El anclaje visual vive en el gutter (columna de números de línea). Click en el número de línea → focus + input. Hover → cursor pointer + outline sutil. Los comentarios existentes se indican con un icono en el gutter de su línea. Click en ese icono muestra el comentario en un popover.

**Alternatives considered:**
- *Input inline below line*: se usará para el editor del comentario, pero el anclaje visual permanente es en el gutter
- *Sidebar con lista de comentarios*: complemento, no reemplazo

### Decision 6: Suggestions con preview pero sin auto-apply
El tipo `suggestion` permite al usuario escribir un reemplazo de código. Se muestra un preview diff (old vs proposed) en el input, pero cluihud **no aplica** el cambio automáticamente. Claude recibe la suggestion en el review batch y decide qué hacer (aplicar, pedir clarificación, rebatir). Esto evita que cluihud modifique archivos sin el loop de Claude.

**Alternatives considered:**
- *Auto-apply de suggestions*: descartado — acopla cluihud al sistema de archivos y rompe el modelo de "Claude siempre sabe qué cambió"

### Decision 7: Virtualización obligatoria en diffs grandes
Para archivos con >500 líneas, el rendering del diff debe estar virtualizado (react-virtual o equivalente) para que la navegación por teclado siga siendo fluida. El focus por línea debe hacer scroll automático para mantener la línea visible.

## Risks / Trade-offs

- **[Risk]** Multi-line selection con teclado puede ser confusa si el usuario no ve el rango resaltado → **Mitigation**: fondo distinto para el rango seleccionado, indicador numérico "lines 10-14 selected"
- **[Risk]** Interacción con scroll del panel durante navegación por teclado → **Mitigation**: `scrollIntoView({ block: 'nearest' })` en cada cambio de focus
- **[Risk]** Shortcuts (`j`, `k`, `c`, `s`) colisionan con otros contextos de la app → **Mitigation**: registrar como context-scoped solo cuando el panel de diff tiene foco Y annotation mode está activo
- **[Risk]** Usuario activa annotation mode en un diff enorme y lag al virtualizar → **Mitigation**: lazy init del focus system solo cuando annotation mode está on
- **[Risk]** Formato serializado del review batch crece mucho con muchos comentarios y excede el prompt budget → **Trade-off aceptado**: documentar límite sugerido (~20 comentarios), usuario decide
- **[Risk]** Suggestions sin auto-apply pueden frustrar al usuario que esperaba el patrón GitHub completo → **Mitigation**: doc UX clara "Claude decides how to apply", y toast explicativo la primera vez
- **[Risk]** Divergencia conceptual entre `annotationsAtom` (markdown) y `diffAnnotationsAtom` (diff) confunde a futuros contribuidores → **Trade-off aceptado**: el modelo de anclaje lo justifica, documentar en CLAUDE.md
