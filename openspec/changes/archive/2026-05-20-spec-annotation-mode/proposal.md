## Why

El panel de specs (`SpecViewer`) actualmente permite edición directa del markdown, lo que puede dañar contratos vivos del proyecto (OpenSpec specs) sin dejar rastro del *por qué* del cambio. El flujo correcto es que el usuario observe, anote y Claude proponga cambios vía un OpenSpec change. Necesitamos replicar el modelo "user as reviewer" que ya funciona en plan annotations, pero adaptado al contexto de specs donde *el usuario inicia* la conversación (no Claude).

## What Changes

- `SpecViewer` abrirá en modo **solo lectura** por defecto (como `PlanPanel` fuera de plan review)
- Nuevo modo **annotation** activable mediante botón en el toolbar del spec viewer o shortcut dedicado
- Reutilización del componente `AnnotatableMarkdownView` (ya usado en plan annotations) con targeting y tipos de anotación idénticos (Comment, Replace, Delete, Insert)
- Nuevo botón **"Send to Claude"** que serializa las anotaciones y las inyecta al chat como mensaje del usuario vía `UserPromptSubmit` hook (no vía FIFO de plan-review, porque no hay request bloqueante)
- El modo edit directo permanece disponible como escape hatch detrás de un botón secundario
- **BREAKING**: el comportamiento default del `SpecViewer` cambia de *edit* a *read-only*

## Capabilities

### New Capabilities
- `spec-annotations`: modo de revisión con anotaciones para specs activas, con envío estructurado de feedback a Claude vía chat injection

### Modified Capabilities
<!-- ninguna -->

## Impact

- **Frontend**: `src/components/spec/SpecViewer.tsx` (refactor a read-only default), nuevo toolbar, integración con `AnnotatableMarkdownView`
- **State**: nuevo atom `specAnnotationModeAtom` y reutilización de `annotationsAtom` con namespace por spec path
- **Shortcuts**: nuevo shortcut para toggle annotation mode en specs (verificar colisiones en `stores/shortcuts.ts`)
- **Backend**: reutilización del canal `UserPromptSubmit` (`inject-edits` hook) ya existente, extendido para aceptar source = "spec"
- **No cambios en DB**: anotaciones de specs usan la misma tabla que plan annotations, diferenciadas por `target_type`
