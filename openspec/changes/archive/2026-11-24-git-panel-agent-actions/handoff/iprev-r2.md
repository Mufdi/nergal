**Revisión ronda 2: `git-panel-agent-actions` (FRI-609)**

Revisé contra el código en `05c263b`. Los 11 hallazgos de la ronda 1 están resueltos. Las referencias path:line corregidas coinciden: `PrViewer.tsx` 597-696, 601-608, 665-671, 672-677, 696, 585-592, 502, 505, 716, 721 y `ConflictsPanel.tsx` 407-429, 421, 422, 483, 936, 1052, 1056, 1057, 1068, 1089. También coinciden `useConflictResolution.ts:81/117/265/272` y `pty.rs` 2008, 2071, 2097, 2122, 2137.

Los deltas MODIFIED respetan el header y todos los escenarios del master spec en `conflict-resolution`, `git-panel-v2`, `ship-flow` y `terminal-wezterm`. El guard del leader cubre los dos órdenes de listeners: en `continuation` el dispatcher llama `cancelLeaderPending()` y además `preventDefault` (`useKeyboardShortcuts.ts:119-123`). Si PrViewer corre antes, ve `leaderPendingAtom` con valor y retorna.

Las ediciones abren tres huecos nuevos de nivel medio.

### Issues

1. **medium: el guard `applying` no cubre la ruta `Ctrl+Shift+R`.** Afecta a la tarea 1.3, a Plan › `PrViewer.tsx` y al escenario "a second click before the delivery resolves sends nothing".
   - `onApply` se registra en un effect cuyos deps son `[isActive, listenerActive, annotations.length, owningSessionId, isOwningSessionActive]` (`PrViewer.tsx:719-725`, con `eslint-disable`).
   - Si `applying` es un `useState`, ese listener queda con `applying = false` de la captura vieja. Dos `Ctrl+Shift+R` dentro de los 250 ms mandan paste, paste, CR, CR, que es justo el caso del issue 9 de la ronda 1.
   - El plan aplica latest-value refs a `state`, `merging` y `performMerge`, pero no al guard de Apply.
   - Fix: un `applyingRef` que se lee y se setea de forma sincrónica al entrar a `applyWithAgent`, y el state solo para el `disabled` del botón. Agregar al walk 6.6: `Ctrl+Shift+R` dos veces seguidas envía una sola vez.

2. **medium: con `performMergeRef` el confirm puede mergear un PR distinto del que nombra el dialog.** Afecta a D1 (guards) y a la tarea 2.3.
   - `PrViewer` no se monta con key (`PrsChip.tsx:234`, `ZenMode.tsx:176`). El spec ya contempla que `prNumber` y `workspaceId` cambien sobre la misma instancia ("PR viewer results are scoped to the displayed PR", `git-panel-v2/spec.md:204`).
   - `performMerge` cierra sobre el `prNumber` actual (`:519-583`), y la ref siempre apunta al render más reciente.
   - Si los props cambian mientras "Merge PR #7?" está abierto, confirmar dispara `gh_pr_merge` sobre el PR nuevo. El re-check solo mira `state === "OPEN" && !merging`, y eso no detecta el cambio de identidad.
   - Es una acción irreversible.
   - Fix: guardar `{ workspaceId, prNumber }` al abrir el confirm y abortar después del `await` si no coinciden con las refs actuales. Sumar ese caso al escenario "PR merged while the confirmation is open" o crear uno propio.

3. **medium: el contrato central del comando nuevo no tiene test automático.** Afecta a la tarea 1.1 y al escenario "Prompt delivery submits after the paste".
   - 1.1 solo prueba los dos caminos de error, y deja para el walk 6.6 la secuencia paste → CR separado → resolve después del CR.
   - Esa secuencia se puede testear: `SharedWriter` es `Arc<Mutex<Box<dyn Write + Send>>>` (`pty.rs:16`). Los tests ya arman un `PtyInstance` real con su pty pair (`pty_pixel_tests.rs:37-62`), así que basta un `CapturedWriter` como `writer`.
   - Con `settle = Duration::ZERO` (o 10 ms), el test registra la sesión en `session_ptys`, llama `deliver_prompt(..).await` y verifica que el buffer tenga `ESC[200~x\nyESC[201~` seguido de `\r`, y que el `\r` ya esté escrito cuando el future resuelve.
   - Ese es justo el comportamiento que esta ronda cambió (resolve después del CR, error propagado). Que solo lo cubra un walk manual es un hueco de cobertura.

4. **low: el `canMerge` que recibe el helper sale de la closure vieja.** Afecta a Plan › `prViewerKeys.ts`.
   - El handler arma `ctx` dentro del effect, y `state` y `merging` no están en los deps. Así, el gate que prueba vitest no es el gate real en ejecución.
   - `requestMergeConfirm` vuelve a chequear y salva el resultado, pero hoy la seguridad depende de esa segunda capa.
   - Fix: armar `ctx.canMerge` con `stateRef.current === "OPEN" && !mergingRef.current`.

5. **low: la firma del helper no incluye `repeat`.** Implementation.md tipa `e: { code; ctrlKey; metaKey; altKey; shiftKey }`, pero la regla usa `!e.repeat` y el test 2.1 pasa `repeat: true`. Falta sumarlo al tipo.

6. **low: el registro de decisiones quedó desalineado con el plan.**
   - `interview_answers.apply_submit` dice "via paste_and_submit_settled", y D3 rechaza explícitamente ese helper. El fondo de la decisión (submit inmediato) se respeta, y el cambio de mecanismo está justificado por el issue 9 de la ronda 1. Igual conviene actualizar el registro o avisar al user para que no aparezca como desviación en la revisión final.
   - `spec_target` no incluye `ship-flow`, que ahora tiene delta.

7. **low: "Ask Agent" mueve el foco a la terminal antes de la entrega, que ahora espera unos 250 ms.**
   - Con el orden de `ConflictsPanel.tsx:419-421`, el foco pasa a la terminal y recién después se hace el invoke.
   - Lo que el usuario teclee en esos 250 ms cae entre el paste y el CR, y se envía junto con el prompt.
   - Fix: mover `setFocusZone("terminal")` y `focusActive()` después de que `deliver_prompt_to_session` resuelva. Si no, agregarlo al riesgo aceptado de D3.

### Verificado sin observaciones

- `paste_to_session` y `submit_to_session` reciben `&PtyManager` y sueltan todos los locks antes de retornar. `deliver_prompt` no mantiene ningún guard durante el `sleep`, así que el future del comando async es `Send`.
- tokio tiene `macros` y `time` (`Cargo.toml:15`), así que `#[tokio::test]` está disponible.
- `leaderPendingAtom` es `LeaderPending | null` (`leader.ts:20`), así que el chequeo truthy es correcto.
- Ningún otro listener de keydown en capture que conviva con PrViewer hace `preventDefault` sobre j, k, Space, a ni Ctrl+←/→. Los listeners de los chips son bubble, y los de ShipDialog son modales. El nuevo early-return por `defaultPrevented` no rompe la navegación actual.
- `terminal_deliver` solo aparece en el master spec `terminal-wezterm` (líneas 91 y 114), y el delta reemplaza las dos menciones.

VERDICT: REVISE
