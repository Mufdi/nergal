# clickup-sync

## Why

Nergal corre alrededor del loop agente↔humano, pero el trabajo que ese loop ejecuta vive hoy en ClickUp, fuera de Nergal. El usuario debe context-switchear a la web de ClickUp para ver qué tasks tiene asignadas, en qué estado están, y qué pide cada una. Esta es la primera de tres changes encadenadas que traen ClickUp adentro: `clickup-sync` construye el cimiento — autenticación, un cliente REST, un **mirror local** de la jerarquía y las tasks, un poller que lo mantiene fresco, y un panel read-only para leerlas con una UI más clara que la de ClickUp (referencia Linear). Las dos changes siguientes (`clickup-task-integration`, `clickup-writeback`) construyen sobre este mirror: meter tasks al loop del agente y escribir de vuelta.

Entrega valor sola: ver tus tasks de ClickUp dentro de Nergal, agrupadas y filtradas, con notificación cuando te asignan algo — sin abrir el navegador.

### Invariante de diseño (no negociable)

La estructura de ClickUp (árbol `Workspace → Space → Folder → List → Task → Subtask`, los **estados por-List**, los **custom fields**, sus **tipos**) es **dato descubierto y sincronizado en runtime, nunca constante de código**. El usuario puede agregar Folders, Lists, estados o custom fields cualquier día sin que Nergal toque código ni migre schema. El snapshot del workspace actual (2 Spaces, Lists folderless, 0 Folders) sirvió para validar shapes de payload, no para fijar estructura.

## What Changes

- **Token + auth**: Personal API token de ClickUp guardado en el keyring del sistema (secret-service vía crate `keyring`), con fallback a archivo de config en `~/.config/cluihud/`. Comando de configuración + validación contra `GET /user`.
- **Cliente REST** (Rust, reusa `reqwest` ya presente): wrapper tipado sobre ClickUp API v2 para los endpoints de lectura (`GET /team`, `GET /space`, `GET /folder`, `GET /list`, `GET /list/{id}` para estados, `GET /team/{team_id}/task` filtrado, `GET /task/{id}`, `GET /task/{id}/comment`). Rate-limit aware.
- **Mirror SQLite structure-agnostic**: tablas nuevas que modelan el árbol genérico completo + estados-por-list + custom-field defs/values + checklists + comentarios + metadata de attachments. Migración(es) nuevas.
- **Poller + diff**: refresca jerarquía y tasks en intervalo configurable (~30-60s), diffea contra el mirror, emite eventos de cambio. `notify-send` cuando una task nueva se asigna al usuario.
- **Panel read-only** (`clickup` right-panel view): icono en TopBar + shortcut, selector de Space persistente con "Todos", group-by (status/list/assignee), filtro assigned-to-me, lista keyboard-navegable, y un **módulo flotante** para el detalle completo de una task (description markdown, subtasks, checklists, comentarios, attachments como chips). Read-only en esta change; los writes llegan en `clickup-writeback`.

## Impact

- **Affected capabilities**: `clickup-mirror` (ADDED), `clickup-task-panel` (ADDED). Greenfield — no specs existentes de ClickUp.
- **Affected code**:
  - Rust: nuevo módulo `src-tauri/src/clickup/` (client, mirror, poller, auth), migración(es) en `src-tauri/migrations/` registradas en `db.rs:132`, comandos Tauri, dep nueva `keyring`.
  - React: nuevo `clickup` `TabType`/panel view en `src/stores/rightPanel.ts`, componentes en `src/components/clickup/`, icono en `src/components/layout/TopBar.tsx`, módulo flotante en `src/components/floating/`.
- **No dependencies**: no depende de las changes del context-bridge. Corre en paralelo.

## Build contract

### Qué construyo

1. Capa de auth: crate `keyring` + fallback config file; comandos `clickup_set_token`, `clickup_clear_token`, `clickup_validate_token` (llama `GET /user`, devuelve el user del token).
2. Cliente REST tipado en `src-tauri/src/clickup/client.rs` sobre `reqwest`, con manejo de rate-limit (429 + `Retry-After`) y paginación.
3. Mirror SQLite structure-agnostic (migración nueva, número asignado en build-time = siguiente libre tras `014`): tablas `clickup_spaces`, `clickup_folders`, `clickup_lists`, `clickup_statuses`, `clickup_tasks`, `clickup_custom_field_defs`, `clickup_task_custom_values`, `clickup_checklists`, `clickup_checklist_items`, `clickup_comments`, `clickup_attachments`.
4. Poller + diff engine: refresca jerarquía + tasks por Space, reconcilia el mirror (incluye tasks que se mueven de List), emite eventos `clickup:changed`. `notify-send` en nueva asignación al usuario del token.
5. Panel read-only: registro del view `clickup`, selector de Space ("Todos" default), group-by + filtro assigned-to-me, lista con focus-zone + arrow-nav, módulo flotante de detalle.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests Rust: parseo de payloads reales (fixtures del workspace validado), diff engine (task nueva / task movida de List / estado cambiado / task des-asignada), rate-limit backoff, assembler de jerarquía con Folders presentes y ausentes.
- Walk manual: configurar token → ver Spaces y tasks → cambiar Space en el picker → agrupar por status → abrir detalle flotante de una task con subtasks/comentarios → asignarme una task desde la web y ver el `notify-send`.

### Criterio de done

- Con un token válido, el panel lista las tasks de los Spaces del workspace, agrupables y filtrables, leyendo del mirror (render instantáneo, sin llamada live por render).
- Agregar un Folder/List/estado/custom-field en ClickUp aparece en el siguiente poll sin cambio de código ni migración.
- Una nueva asignación dispara `notify-send`.
- El token nunca toca disco en plaintext si el keyring está disponible.
- Sin `unwrap()`/`expect()` fuera de tests; sin TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 22
- risk_tier: critical
- tags: [migration, security, feature]
- visibility: private
- spec_target: clickup-mirror, clickup-task-panel
