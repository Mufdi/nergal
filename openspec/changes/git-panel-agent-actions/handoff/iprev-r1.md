**Revisión del change `git-panel-agent-actions` (FRI-609)**

Revisé contra el código en `05c263b`. Casi todos los facts de implementation.md se sostienen. Encontré un conflicto de teclado real que el plan no ve y un hueco de specs, así que pido revisión antes de construir.

### Issues

1. **high: `leader m` (Complete Merge) también abre la confirmación de merge del PR.** Afecta a D1 y a la tarea 2.2.
   - El dispatcher global y el listener de `PrViewer` están ambos en `window` en capture phase. El dispatcher resuelve el chord con `e.preventDefault(); e.stopPropagation()` (`useKeyboardShortcuts.ts:119-123`). `stopPropagation` no frena otros listeners del mismo target.
   - El handler de `PrViewer` (`PrViewer.tsx:600-608`) no mira `e.defaultPrevented` ni `leaderPendingAtom`. Por eso `Ctrl+Space` y luego `m` con el PR viewer activo corre `complete-merge` (`shortcuts.ts:786`) y además abre "Merge PR #N?".
   - `ConfirmHost` confirma con Enter (`ConfirmHost.tsx:71-75`), así que un Enter de más deja un merge irreversible a una tecla.
   - Hoy ya pasa lo mismo con `leader a` y annotate, pero `M` lo convierte en un riesgo de merge.
   - Lo que sigue en este punto es inferencia mía y no lo probé en ejecución: el orden de los listeners no es fijo. El global se re-registra cuando cambian `registry/overrides/paletteOpen/zenOpen` (`:369`) y `PrViewer` con sus propios deps. Por eso no es seguro el "global listener registers first" de D1.
   - Fix: en el handler, `if (e.defaultPrevented || appStore.get(leaderPendingAtom)) return;`. Agregar un caso al walk: `leader m` en el PR viewer no abre la confirmación.

2. **medium: la decisión sobre closures stale quedó abierta.** Afecta a "Per-phase risk" y a D1.
   - El plan dice "add them to the effect deps or read them through a ref, and cover with the walk". Hay que elegir.
   - Hoy los deps (`PrViewer.tsx:696`; el plan cita `:695`, que es el `eslint-disable`) no incluyen `state`, `merging`, `baseRefName`, `prNumber`, `title` ni `performMerge`. Si el PR pasa de OPEN a MERGED por un refresh y ningún otro dep cambia, `M` abre la confirmación sobre un PR ya mergeado.
   - Además `requestMergeConfirm` hace `await confirm()`, y al volver usa `performMerge` y `state` de la captura vieja.
   - Fix: usar refs con el valor más reciente para `state`, `merging` y `performMerge`, y volver a chequear `state === "OPEN" && !merging` después del `await`. El walk solo no alcanza como gate.

3. **medium: falta un delta para el spec `ship-flow`.**
   - `openspec/specs/ship-flow/spec.md:139-140` ("Requirement: Apply-with-Claude tooltip…") exige el label `Apply with Claude (N)`. El change modifica `git-panel-v2` a `Apply with Agent (N)` pero no toca `ship-flow`.
   - Al archivar, dos master specs quedarían contradiciéndose. Falta un bloque MODIFIED en `specs/ship-flow/spec.md`, y `ship-flow` falta en "Modified Capabilities".
   - De paso: `conflict-resolution/spec.md:82` dice "triggers 'Ask Claude to resolve'", dentro de un requirement que el delta no toca.

4. **medium: el picker de Conflicts queda fuera, y el requirement dice "Every".**
   - El requirement nuevo dice "Every keyboard hint bar in the git panel SHALL use one shared hint bar… and to the PR viewer file picker".
   - El picker de archivos en conflicto (`ConflictsPanel.tsx:1084-1090`) es una copia exacta del picker del PR viewer: header sticky con `j/k move · Enter pick · Esc close` en 9px. No aparece en ninguna tarea.
   - Lo mismo pasa con `ConflictsPanel.tsx:1057` ("Ctrl+Shift+R sends") y los hints de pane (`:1456`).
   - Hay que incluir el picker en 3.6 o acotar el texto del requirement. Si no, el change archiva un spec que el código no cumple.

5. **medium: lista de Impact y tareas incompleta en el rename `onAskClaude` → `onAskAgent`.**
   - La prop vive en `src/hooks/useConflictResolution.ts` (`:81`, `:117`, `:265`, `:272`). Ese archivo no está en Impact ni en la tarea 1.4. `tsc` lo detectaría, pero el plan debería nombrarlo.
   - Faltan también los comentarios que nombran el símbolo viejo: `PrViewer.tsx:264` ("Apply-with-Claude") y `:716` ("applyWithClaude is the same path…"). Tras el rename quedan apuntando a una función que no existe.

6. **medium: la verificación no cubre la mayoría de los escenarios del requirement `M`.**
   - El spec exige que `M` no haga nada con el PR no abierto, durante un merge, con un dialog abierto, en annotation mode y en un text field. El body con "3 annotations are unresolved" también es un escenario.
   - `prViewerKeyAction` no recibe `prState` ni `merging`, así que esos guards no se pueden testear con vitest. Y el walk 6.3 solo cubre Cancel y `Ctrl+Enter`.
   - Fix: sumar `canMerge` al `ctx` del helper con casos Red (`canMerge: false` → `null`). Agregar al walk: `M` sobre un PR mergeado, `M` con foco en la terminal (debe escribir `m`), el body con conteo de anotaciones y el `leader m` del issue 1.
   - El test de 2.1 tampoco cubre `Space` → `"toggle-collapse"` ni `Space` con modificadores, que el helper sí devuelve.

7. **low: el chip `M` invita a presionar `Shift+M`, y `Shift+M` no hace nada.**
   - `formatKey` pasa las letras a mayúscula (`kbd.tsx:26`), y spec, proposal y chips dicen "M". El test 2.1 fija `Shift+M` → `null`.
   - Hay que decidirlo explícitamente: aceptar `Shift+M` también o dejarlo documentado. Hoy es un default implícito.

8. **low: "dos `M` seguidas" está mal razonado en Per-phase risk.**
   - `confirm()` encola: si hay uno activo, el nuevo va a `queue` (`confirm.ts:63-64`). Una segunda pulsación antes de que React monte `DialogContent` deja dos confirmaciones encoladas. Si se confirma la primera, aparece la segunda y puede disparar un `gh_pr_merge` sobre un PR ya mergeado.
   - Es poco probable, pero conviene un guard explícito: una ref `mergeConfirmOpen` o ignorar `e.repeat`, en vez de depender del timing de render.

9. **low: el submit settled no tiene guard de in-flight.**
   - `deliver_prompt_to_session` resuelve antes del CR. Dos Apply en menos de 250 ms (doble click, o `Ctrl+Shift+R` dos veces) dan paste, paste, CR, CR: el prompt duplicado se envía como un solo turno.
   - En `ConflictsPanel`, `sending` se libera casi al instante porque el invoke vuelve rápido, y `applyWithAgent` no tiene ningún guard.
   - El error del submit se descarta (`pty.rs:2137`, `let _ =`), y el toast "Sent" sale igual.
   - Con submit automático, un borrador que el usuario tenga a medias en el input de CC se envía junto al prompt. La decisión del submit ya está tomada; esto es solo un caso borde a anotar.

10. **low: varias referencias path:line están corridas en uno.** Las demás que revisé coinciden (`pty.rs` 2008/2071/2097-2113/2122/2664/2681, `lib.rs:533`, `shortcuts.ts:704`, `useKeyboardShortcuts.ts:341`, `ConfirmHost.tsx:62`, `PrViewer.tsx:502/505/1128/1160/1161/1166-1170/1214/1083-1090`, `PrsChip.tsx:258-260/315-317`, SpecPanel y Plan).

    | Lo que dice el plan | Lo que hay en el código |
    |---|---|
    | listener `:596-693` | effect `:597-696` |
    | guards `:600-608` | `:601-608` |
    | `a` en `:666-671` | `:665-671` |
    | deps `:695` | `:696` |
    | `handleMergeClick` `:584-591` | `:585-592` |
    | gate de `merging` `:1205` | `disabled` en `:1207` |
    | `askClaude` `:407-428` | `:407-429` |
    | `writeToSession` `:420` (implementation.md y design D3) | `:421` |
    | toast `:421` | `:422` |

11. **low: la tarea 4.4 no hace nada.** Los strings de los tooltips de Apply (`PrViewer.tsx:1166-1170`) no contienen "Claude". Hay que borrarla o reapuntarla a los comentarios del issue 5.

### Verificado sin observaciones

- `paste_and_submit_settled` rechaza ids de aux shell antes de escribir nada, así que el escenario "write nothing to any PTY" se cumple.
- El input de la terminal es un `TEXTAREA`, así que el guard `inField` de `PrViewer` protege a `M` mientras se escribe en la terminal.
- Los non-goals de FilesChip y StashesChip con `Ctrl+Enter` se sostienen: el handler de la lista sale con Ctrl en `StashesChip.tsx:165`.
- `terminal_deliver` tiene un solo caller en el frontend.

VERDICT: REVISE
