# linear-mirror

## Why

Nergal corre alrededor del loop agente↔humano, pero para muchos equipos el trabajo que ese loop ejecuta vive en **Linear**, fuera de Nergal. Ya trajimos ClickUp adentro (3 changes archivadas); Linear es el segundo tracker de primera clase. Esta es la primera de tres changes encadenadas que replican el patrón ClickUp para Linear: `linear-mirror` construye el cimiento — autenticación, un **cliente GraphQL**, un mirror local SQLite de teams/states/issues, un poller que lo mantiene fresco, y un panel read-only para leer issues con la misma UX keyboard-first que ya canonizamos. Las dos changes siguientes (`linear-agent-integration`, `linear-writeback`) construyen sobre este mirror: meter issues al loop del agente y escribir de vuelta.

Entrega valor sola: ver tus issues de Linear dentro de Nergal, agrupadas y filtradas, con notificación cuando te asignan algo — sin abrir el navegador.

### Invariante de diseño (no negociable)

La estructura de Linear (los **workflow states por-team** con su `type` nativo, los **labels**, las **priorities**, los **projects/cycles**, el árbol de **sub-issues**) es **dato descubierto y sincronizado en runtime, nunca constante de código**. El usuario puede agregar un estado, un label o un team cualquier día sin que Nergal toque código ni migre schema. A diferencia de ClickUp (REST v2, estados por-List, custom fields arbitrarios), Linear expone una **API GraphQL** con un modelo de datos fijo (sin custom fields ni checklists): esta change adapta el patrón, no lo copia.

### Auth extensible (decisión heredada de ClickUp)

ClickUp arrancó solo con Personal API token y dejamos OAuth para después; Linear sigue la misma estrategia. Esta change implementa **Personal API key** (`Authorization: <key>`, sin `Bearer`), pero la capa de auth se diseña para que agregar OAuth2 más adelante (`Authorization: Bearer <token>`) sea aditivo — un `AuthMode` que el header-builder consume — sin reescribir el cliente ni la persistencia. Ver design.md D2.

## What Changes

- **Token + auth**: Personal API key de Linear guardada en el keyring del sistema (secret-service vía crate `keyring`, ya presente por ClickUp) bajo `cluihud` / `linear-token`, con fallback atómico a `~/.config/cluihud/linear.toml` mode `0600` y disclosure a la UI. Comando de configuración + validación contra la query `viewer { id name email }`. Header builder switchable por `AuthMode` (personal ahora; oauth-bearer reservado).
- **Cliente GraphQL** (Rust, reusa `reqwest` ya presente): wrapper tipado sobre `https://api.linear.app/graphql` para las queries de lectura (viewer, teams con workflow states + labels, issues paginadas con sus relaciones anidadas, comments). Una sola query trae issues con su state/assignee/labels/project/cycle anidados — ventaja GraphQL que simplifica el poller. Rate-limit aware: Linear devuelve **HTTP 400 con error GraphQL `RATELIMITED`** (no 429/`Retry-After`) y headers `X-RateLimit-{Requests,Complexity}-{Remaining,Reset}` (reset en epoch-ms); el backoff espera hasta el `Reset` con cap.
- **Mirror SQLite**: tablas nuevas que modelan teams → workflow-states (con `type` nativo) + labels + issues (con `parent_id` self-FK para sub-issues) + issue↔label join + comments + projects/cycles (metadata mínima) + sync-state. Migración nueva `023`.
- **Poller + reconcile**: refresca teams y issues en intervalo configurable (default 45s, floor 10s), reconcilia contra el mirror en una transacción, tombstonea issues ausentes, emite eventos de cambio. `notify-send` cuando un issue nuevo se asigna al usuario. Scope de poll acotado (ver design.md D4): issues activos de los team(s) seleccionados actualizados dentro de una ventana, + siempre los asignados al viewer — Linear puede tener decenas de miles de issues, polear todos es inviable.
- **Panel read-only** (`linear` right-panel view): icono en TopBar + shortcut, selector de Team persistente con "Todos", group-by (state/project/assignee), filtro assigned-to-me, lista keyboard-navegable, y un **módulo flotante** para el detalle completo de un issue (description markdown, sub-issues, comments, labels/priority chips, attachments). Read-only en esta change; los writes llegan en `linear-writeback`.

## Impact

- **Affected capabilities**: `linear-mirror` (ADDED), `linear-task-panel` (ADDED). Greenfield — no specs existentes de Linear.
- **Affected code**:
  - Rust: nuevo módulo `src-tauri/src/linear/` (auth, client, model, mirror, poller), migración `023` en `src-tauri/migrations/` registrada en `db.rs`, comandos Tauri registrados en `lib.rs`. `keyring` ya es dep (ClickUp).
  - React: nuevo `linear`/`linear-task` `TabType`/panel view en `src/stores/rightPanel.ts`, componentes en `src/components/linear/`, icono en `src/components/layout/TopBar.tsx`, atoms en `src/stores/linear.ts`, módulo flotante. Reusa los componentes compartibles `StatusIcon`/`PriorityIcon` (Linear-style ya) y el patrón dual-shell detail/tab donde aplique.
- **No dependencies**: no depende de ClickUp ni de las changes del context-bridge. Corre en paralelo. Reusa la dep `keyring` y los patrones canonizados (`docs/patterns.md` §2/§8-13, `docs/design.md` §3.16).

## Build contract

### Qué construyo

1. Capa de auth: keyring `cluihud`/`linear-token` + fallback `linear.toml` 0600 atómico; comandos `linear_set_token`, `linear_clear_token`, `linear_validate_token` (query `viewer`, devuelve el user). `AuthMode` switchable (personal ahora).
2. Cliente GraphQL tipado en `src-tauri/src/linear/client.rs` sobre `reqwest`, con manejo de rate-limit (HTTP 400 `RATELIMITED` + `X-RateLimit-*-Reset`) y paginación cursor (`pageInfo { hasNextPage endCursor }`, `first/after`).
3. Mirror SQLite (migración `023`): tablas `linear_teams`, `linear_workflow_states`, `linear_labels`, `linear_projects`, `linear_cycles`, `linear_users`, `linear_issues`, `linear_issue_labels`, `linear_comments`, `linear_sync_state`.
4. Poller + reconcile: refresca teams + issues por team seleccionado (scope acotado), reconcilia el mirror (issues que cambian de state/assignee/team), emite `linear:changed`. `notify-send` en nueva asignación al viewer.
5. Panel read-only: registro del view `linear`, selector de Team ("Todos" default), group-by + filtro assigned-to-me, lista con focus-zone + arrow-nav, módulo flotante de detalle.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests Rust: parseo de payloads GraphQL reales (fixtures), reconcile (issue nuevo / issue reasignado / state cambiado / issue des-asignado → tombstone), rate-limit backoff (parse de `X-RateLimit-*-Reset` + error `RATELIMITED`), cursor pagination (hasNextPage true con página corta), header builder por `AuthMode`.
- Walk manual: configurar API key → ver teams e issues → cambiar Team en el picker → agrupar por state → abrir detalle flotante de un issue con sub-issues/comments → asignarme un issue desde la web y ver el `notify-send`.

### Criterio de done

- Con una API key válida, el panel lista los issues de los team(s) seleccionados, agrupables y filtrables, leyendo del mirror (render instantáneo, sin llamada live por render).
- Agregar un state/label/team en Linear aparece en el siguiente poll sin cambio de código ni migración.
- Una nueva asignación dispara `notify-send`.
- La API key nunca toca disco en plaintext si el keyring está disponible.
- Agregar OAuth más adelante es aditivo sobre `AuthMode`, sin reescribir el cliente.
- Sin `unwrap()`/`expect()` fuera de tests; sin TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 22
- risk_tier: critical
- tags: [migration, security, feature]
- visibility: private
- spec_target: linear-mirror, linear-task-panel
