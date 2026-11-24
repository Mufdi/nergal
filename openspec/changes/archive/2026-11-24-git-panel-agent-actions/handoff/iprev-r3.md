**Revisión ronda 3: `git-panel-agent-actions` (FRI-609)**

Revisé contra el código en `05c263b`. Los 7 hallazgos de la ronda 2 están resueltos, y ninguno de los cambios abre un riesgo medio o alto. Quedan cinco puntos de nivel bajo que se pueden resolver durante el build.

### Ronda 2: cómo quedó cada punto

1. **Guard de Apply:** resuelto. El listener de `Ctrl+Shift+R` (`PrViewer.tsx:719-725`) tiene deps desactualizados, y `applyingRef`, que se lee y se setea de forma sincrónica, lo cubre. El walk 6.6 incluye "`Ctrl+Shift+R` dos veces".
2. **Identidad del PR:** resuelto. Confirmé que `PrViewer` no se monta con key (`PrsChip.tsx:234`, `ZenMode.tsx:176`), y que `performMerge` cierra sobre `workspaceId` y `prNumber` (`:519-583`). El plan captura `{ workspaceId, prNumber }` y vuelve a compararlos después del `await`, y tiene su propio escenario.
3. **Test del happy path:** resuelto. `write_bracketed` (`pty.rs:2041-2049`) escribe `ESC[200~` + body + `ESC[201~` sin tocar los `\n`. `submit_to_session` (`:2097-2113`) escribe un `\r` aparte. El buffer esperado del test 1.1 es correcto. `PtyInstance` se puede armar como en `pty_pixel_tests.rs:37-62`, con un `CapturedWriter` como `writer`.
4. **`canMerge`:** resuelto. Ahora se arma desde las refs.
5. **`repeat` en el tipo:** resuelto.
6. **Registro de decisiones:** resuelto. El refinamiento del mecanismo quedó registrado y `spec_target` incluye `ship-flow`.
7. **Foco de Ask Agent:** resuelto. El foco pasa a la terminal recién cuando la entrega resuelve.

### Issues

1. **low: `prIdentityRef` se usa pero no se declara.**
   - En Plan › `PrViewer.tsx`, la lista de refs es "`stateRef`, `mergingRef`, `performMergeRef` updated every render, and `mergeConfirmOpenRef`". Después, `requestMergeConfirm` lee `prIdentityRef`, que no está en esa lista.
   - Fix: agregar `prIdentityRef` (`{ workspaceId, prNumber }`, actualizada en cada render) a la lista de refs y a la tarea 2.3. Si no se actualiza en cada render, la comparación después del confirm siempre da igual y el guard no hace nada.

2. **low: no dice cuándo se libera `applyingRef`.**
   - El plan dice "checked and set synchronously on entry", pero no dice cuándo se limpia.
   - Si se setea antes de los early returns (`!isOwningSessionActive`, sin annotations, sin `sid`; `PrViewer.tsx:479-481`), o si no se limpia en `finally`, un error de entrega deja Apply bloqueado hasta remontar el componente.
   - Fix: setearla después de los gates y limpiarla en `.finally()`, junto con el state `applying`.

3. **low: `Ask Agent` no tiene el mismo guard que Apply.**
   - D3 dice que "a repeated `Ctrl+Shift+R` within the 250 ms window no longer sends paste, paste, CR, CR" para los dos callers. Pero `askAgent` sigue protegido solo por el state `sending` (`ConflictsPanel.tsx:358`, `:408`).
   - El listener del hook se re-registra cuando cambia `onAskClaude` (`useConflictResolution.ts:272`). Eso solo pasa después del render que sigue a `setSending(true)`, que corre desde un listener nativo de `document`, fuera de un evento React. Un segundo evento antes de ese commit todavía lee `sending = false`.
   - Además, ahora la ventana de `sending` es unos 250 ms más larga.
   - Fix: usar un `sendingRef` sincrónico, igual que en Apply. La otra opción es acotar el texto de D3 a Apply.

4. **low: el listener de `Ctrl+Shift+R` manda el texto de anotación desactualizado (ya existía).**
   - Los deps de `:725` miran `annotations.length`, no el contenido. Si el usuario edita una anotación sin cambiar el conteo, `Ctrl+Shift+R` envía el texto viejo, junto con `hunks` y `title` de la captura vieja. El click del botón manda el texto actual.
   - La tarea 1.3 ya toca este effect. Lo más simple es `applyRef.current = applyWithAgent` en cada render y `onApply() { applyRef.current(); }`, y así también deja de hacer falta el `eslint-disable`.

5. **low: el test 1.1 deja un proceso hijo vivo.**
   - La construcción de `pty_pixel_tests.rs:42-47` lanza `sh -c "sleep 30"`. Esos tests lo matan con `reap()` (`:64-70`), y la tarea 1.1 no lo menciona.
   - Fix: llamar a `reap` (o equivalente) al final del happy path.

### Verificado sin observaciones

- `deliver_prompt` recibe `&PtyManager` y no mantiene ningún guard de mutex durante el `sleep`, así que el command async devuelve `Result` y su future es `Send`.
- Si el viewer se desmonta con el confirm abierto, la identidad capturada sigue igual a la de la ref. El merge resultante es el del PR que nombra el dialog, así que es correcto.
- Mover `setFocusZone` y `focusActive` después del `await` no cambia nada cuando la entrega falla: el foco queda en el panel y sale el toast de error.

VERDICT: APPROVED
