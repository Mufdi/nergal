## Why

Plan annotations (comment, replace, delete, insert) son el diferenciador principal de cluihud — ningún otro wrapper tiene edición bidireccional de planes. La implementación actual está rota: el hover CSS interfiere con la interacción, la selección de texto no persiste visualmente al soltar el mouse, `surroundContents()` falla al cruzar múltiples elementos DOM, y las annotations guardadas no se visualizan correctamente. El revise flow no inyecta feedback a Claude de forma confiable. Se necesita reescribir el sistema de annotations con un approach DOM-first (inspirado en plannotator) y persistir en SQLite para sobrevivir recargas.

## What Changes

- **Reescribir AnnotatableMarkdownView.tsx** con approach DOM-first: manipulación directa de `<mark>` elements via TreeWalker en vez de React state para highlights. Solo el toolbar usa React state.
- **Fix text selection**: escuchar `mouseup`, leer `window.getSelection()`, wrappear cada text node individualmente (no `surroundContents()` que falla cross-element), limpiar selection nativa para que el `<mark>` persista.
- **Fix pinpoint click**: `data-pinpoint-active` attribute en DOM element, CSS lo estiliza amarillo. Click → toolbar aparece. Esc limpia estado.
- **Fix hover interference**: hover CSS puro (`:hover` outline naranja) no debe interferir con pinpoint click ni text selection activa.
- **Persistir annotations en SQLite** con tabla dedicada, cargar al abrir plan, guardar al crear/eliminar.
- **Fix revise flow**: serializar annotations como instrucciones estructuradas e inyectar via `UserPromptSubmit` hook (no como HTML comment embebido en el plan file).

## Capabilities

### New Capabilities
- `annotation-persistence`: Persistencia de annotations en SQLite con CRUD backend y carga/guardado automático por sesión

### Modified Capabilities
- `plan-annotations`: Fix completo del sistema de annotations — hover, pinpoint, text selection, toolbar, gutter markers, y revise flow. La spec existente en `openspec/specs/plan-annotations/spec.md` define los requirements correctos; la implementación es lo que está roto.

## Impact

- **Frontend**: `AnnotatableMarkdownView.tsx` (rewrite), `PlanAnnotationToolbar.tsx` (ajustes), `PlanPanel.tsx` (revise flow), `stores/annotations.ts` (sync con backend), `globals.css` (ajustes CSS)
- **Backend**: Nuevo comando Tauri para CRUD annotations, nueva migración SQLite
- **Hooks**: `inject-edits` en `UserPromptSubmit` debe incluir annotations serializadas
- **Specs afectadas**: `plan-annotations` (spec existente, requirements no cambian — solo la implementación)
