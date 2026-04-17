## Context

`SpecViewer` hoy renderiza specs de OpenSpec (`openspec/specs/**/spec.md`) con edición directa habilitada, lo que permite al usuario alterar contratos vivos del proyecto sin dejar rastro del *por qué*. Paralelamente, `plan-annotations` ya implementa un flujo maduro de anotaciones (Comment/Replace/Delete/Insert) sobre markdown, con serialización estructurada e inyección al prompt de Claude vía hook. Ese stack cubre ~80% de lo que necesitamos para specs; lo único que cambia es **quién inicia** el flujo (el usuario, no Claude) y **el canal de salida** (no hay FIFO bloqueante esperando respuesta).

## Goals / Non-Goals

**Goals:**
- Default read-only en `SpecViewer` (el usuario ya no puede romper un spec por accidente)
- Toggle a annotation mode vía botón y shortcut dedicado
- Reutilizar `AnnotatableMarkdownView`, `PlanAnnotationToolbar`, `serializeAnnotations()` y `annotationsAtom` sin duplicar lógica
- Envío de feedback a Claude vía el mismo mecanismo `inject-edits` (`UserPromptSubmit` hook)
- Mantener edit directo como escape hatch explícito

**Non-Goals:**
- Modificar el flujo de plan annotations (no debe haber regresiones)
- Crear un nuevo canal de IPC (se reutiliza `pending_annotations` en `HookState`)
- Edición colaborativa o sync multi-sesión de anotaciones
- Persistencia de anotaciones de spec entre cierres de app (MVP: in-memory + SQLite si sale gratis)

## Decisions

### Decision 1: Default mode = read-only, annotation mode behind toggle
Replicar exactamente el patrón de `PlanPanel` cuando no hay plan review activo. El `SpecViewer` muestra el markdown renderizado sin interacciones. Un botón en el toolbar + shortcut activa annotation mode.

**Alternatives considered:**
- *Default annotation mode*: descartado — confunde al usuario que solo quiere leer la spec
- *Sin toggle, anotar siempre disponible*: descartado — las interacciones de hover/selection compiten con scroll/copy normal

### Decision 2: Reutilizar `AnnotatableMarkdownView` tal cual
El componente ya acepta `content: string` y emite anotaciones vía el store. No necesita conocer el origen (plan vs spec). El `SpecViewer` lo instancia igual que `PlanPanel`, pasando el contenido del `.md` actual.

**Alternatives considered:**
- *Fork del componente*: descartado — duplicaría bugs y divergencia de UX
- *Prop `source: 'plan' | 'spec'`*: descartado para el componente de rendering; el source solo importa en el momento de serializar/enviar

### Decision 3: Namespace en `annotationsAtom` por target ID
El atom de anotaciones se extiende para aceptar un `targetId` compuesto: `plan:<sessionId>` o `spec:<specPath>`. Así plan y spec annotations coexisten sin colisión y el `serializeAnnotations()` puede filtrar por target.

**Alternatives considered:**
- *Atom separado `specAnnotationsAtom`*: descartado — duplica la lógica de persistencia/clear
- *Un solo atom plano*: descartado — mezclaría anotaciones de contextos distintos

### Decision 4: Canal de salida = `UserPromptSubmit` (inject-edits), NO plan-review FIFO
Plan annotations usan el FIFO porque hay un `PermissionRequest[ExitPlanMode]` bloqueante esperando decisión. En specs **no hay request pendiente** — el usuario inicia el flujo. Por eso el envío debe ir por el canal asíncrono de `inject-edits`, que ya sabe leer `pending_annotations` del `HookState` y lo inyecta al siguiente prompt del usuario.

El botón "Send to Claude" hace:
1. Serializa anotaciones vía `serializeAnnotations()` (mismo helper)
2. Escribe resultado + contexto (`spec path`) en `HookState.pending_annotations`
3. Limpia anotaciones locales
4. Muestra toast "Feedback queued — será enviado en tu próximo mensaje"
5. El próximo `UserPromptSubmit` de Claude lo inyecta automáticamente

**Alternatives considered:**
- *Nuevo Tauri command para enviar directo*: descartado — requeriría un nuevo endpoint al CLI sin ganancia clara
- *Escribir al PTY directamente*: descartado — acopla UI con el transport del terminal
- *Crear un PermissionRequest sintético*: descartado — rompe la semántica del hook

### Decision 5: Edit mode como escape hatch, no default
Mantener el editor markdown existente detrás de un botón "Edit directly" en el toolbar. Cambiar a edit mode muestra un warning inline: "Editing spec directly bypasses OpenSpec change flow". Esto preserva la capacidad pero la hace consciente.

**Alternatives considered:**
- *Eliminar edit mode*: descartado — hay casos legítimos (typos, formato)
- *Edit mode oculto tras settings*: descartado — demasiada fricción

### Decision 6: Shortcut propuesto = `a` (cuando SpecViewer tiene foco)
Mismo patrón que el modo annotation de plans. Verificar en `stores/shortcuts.ts` que no colisione con otro shortcut global/contextual. Si hay colisión, alternativa: `Shift+A`.

## Risks / Trade-offs

- **[Risk]** Usuario activa annotation mode en un spec muy largo y pierde el scroll position al renderizar marks → **Mitigation**: preservar `scrollTop` durante el re-render del toggle
- **[Risk]** Feedback inyectado vía `inject-edits` se pierde si el usuario no envía un mensaje pronto → **Mitigation**: toast visible + indicador persistente "N pending annotations will be sent" hasta que se envíen o se descarten
- **[Risk]** Confusión entre "annotations en spec activo" vs "annotations en plan activo" si ambos tienen anotaciones simultáneas → **Mitigation**: el namespace en `annotationsAtom` mantiene aisladas las listas; UI solo muestra la del panel activo
- **[Risk]** Reutilizar `AnnotatableMarkdownView` acopla specs al ciclo de vida del componente de plans → **Trade-off aceptado**: el acoplamiento es en el sentido correcto (un solo componente, múltiples contextos), y las modificaciones futuras benefician a ambos
- **[Risk]** Edit mode directo sigue disponible y puede seguir siendo usado por costumbre → **Mitigation**: warning inline + default read-only + onboarding implícito vía descubrimiento del botón annotation
