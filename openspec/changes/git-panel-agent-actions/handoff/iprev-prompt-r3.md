You are a senior engineer performing an independent review of a proposed implementation plan (an OpenSpec change in the repo at /home/felipe/Projects/cluihud, a Tauri 2 + React 19 desktop wrapper for Claude Code). You may read any repo file to verify claims (read-only; do not edit anything).

Evaluate it for:
1. Correctness — does the approach actually solve the stated problem?
2. Completeness — missing steps, edge cases, failure modes
3. Concurrency / state integrity — races, stale reads
4. Test coverage — is there a clear way to verify each change?
5. Security — input validation, injection surfaces
6. Rollback — is the change reversible?

The decisions in handoff/.work-modules.json interview_answers are settled by the user; do not re-litigate them, review their execution.
Verify the path:line facts in implementation.md against the code.

List issues as a numbered markdown list with severity (critical/high/medium/low). Be specific — cite sections of the plan. Do not soften findings.

End your review with exactly one of these two lines (no quotes, no trailing content):
VERDICT: APPROVED
VERDICT: REVISE



Previous review feedback (rounds 1 and 2):
---
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


---

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

---

Changes since round 2:
1. applyingRef checked/set synchronously on entry (state only drives disabled); walk 6.6 adds Ctrl+Shift+R twice.
2. requestMergeConfirm captures {workspaceId, prNumber} and aborts after confirm if they changed; new scenario "Displayed PR changes while the confirmation is open".
3. Task 1.1 adds the happy-path tokio test with a capturing SharedWriter on a real PtyInstance (paste bytes then trailing \r, written before the future resolves).
4. ctx.canMerge built from the refs.
5. Helper type includes repeat.
6. interview_answers.apply_submit records the mechanism refinement; spec_target includes ship-flow.
7. Ask Agent moves focus to the terminal after the delivery resolves.


Revised plan (after edits):
---

===== proposal.md =====
Linear: FRI-609

## Why

The checkpoint-1 walk (2026-11-24) found four defects in the git panel and in the actions that send a prompt to the agent:

- `Ctrl+Enter` in the PR viewer merges the PR. It is also the global fullscreen-terminal shortcut, and both listeners run on the same keypress.
- The git panel hint bars use 9px text, differ from view to view, and the PR viewer has none. Nothing tells the user that `a` annotates a hunk.
- Actions that hand a prompt to the agent are labelled "Claude". Nergal is agent-agnostic, so the labels name "Agent".
- "Apply with Claude" pastes the prompt but does not submit it. "Ask Claude" in the conflicts view types the prompt with a raw `\r`, so a multi-line prompt arrives as several lines.

## What Changes

- **Merge key**: in the PR viewer, `M` opens a confirmation dialog, and confirming merges the PR. A leader chord ending in `m`, a key repeat, or a PR that is no longer open never starts a merge. The viewer no longer handles `Ctrl+Enter`, so `Ctrl+Enter` only toggles the fullscreen terminal. The Merge button shows `M` as its chip.
- **Hint bar**: one shared `HintBar` component renders every git panel hint bar: Files, History, Stashes, PRs list, PR viewer, Conflicts, Branches and the PR viewer and Conflicts file pickers. Inline hints beside one control move to 10px. It is a sticky flex row at the bottom, with a background, `Kbd` chips at 10px and 10px labels. The PR viewer gets a hint bar with `j/k`, `a`, `Ctrl+←/→` and `M`.
- **Kbd size**: `Kbd` gains a `size` option. The hint bar uses the 10px size; buttons and tooltips keep 9px.
- **Labels**: agent-facing actions say "Agent" instead of "Claude": "Apply with Agent", "Ask Agent", "Intent for the agent (optional)", "Sent to agent", "Send to Agent", "The agent will revise the plan" and "Waiting for the agent to revise...". Product names ("Claude Code" in setup, MCP and settings) and the "Claude" status provider stay.
- **Submit**: a new Tauri command `deliver_prompt_to_session` pastes a prompt into the session as one bracketed paste, submits it after a short settle, and resolves only once the submit is written. "Apply with Agent" and "Ask Agent" use it. `terminal_deliver` is removed: the PR viewer was its only caller.
- **Docs**: `docs/patterns.md` §4 and `docs/design.md` §3.4 describe the hint bar standard and the `M` verb.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `git-panel-v2`: the PR viewer merges with `M` after a confirmation; the git panel views share one hint bar; "Apply with Agent" submits its prompt at once.
- `terminal-wezterm`: the agent delivery used by the git panel is `deliver_prompt_to_session`, which also submits; `terminal_deliver` is gone.
- `conflict-resolution`: "Ask Agent" submits the conflict prompt as one paste through the same command; the contextual shortcut names "Ask Agent".
- `ship-flow`: the Apply button label reads `Apply with Agent (N)`.
- `spec-annotations`: the SpecPanel action is labelled "Send to Agent".

## Impact

- Frontend: `src/components/git/PrViewer.tsx`, `src/components/git/ConflictsPanel.tsx`, `src/hooks/useConflictResolution.ts`, a new `src/lib/prViewerKeys.ts`, `src/components/git/chips/{FilesChip,HistoryChip,BranchesChip,StashesChip,PrsChip}.tsx`, `src/components/ui/kbd.tsx`, a new `src/components/ui/hint-bar.tsx`, `src/components/spec/SpecPanel.tsx`, `src/components/plan/{PlanPanel,PlanAnnotationToolbar}.tsx`.
- Backend: `src-tauri/src/pty.rs` (new command, `terminal_deliver` removed), `src-tauri/src/lib.rs` (command registration).
- Docs: `docs/patterns.md`, `docs/design.md`.
- No config, schema or hook change. No new dependency.


===== design.md =====
## Context

The checkpoint-1 walk of FRI-604/506/507/473 found four git panel defects (FRI-609). The user settled each fix on 2026-11-24. This design records the decisions and the alternatives that were rejected. No flow or state machine changes, so there is no diagram.

## Goals / Non-Goals

**Goals:**
- One key, one action: `Ctrl+Enter` toggles the fullscreen terminal everywhere, including the PR viewer.
- One hint bar standard across the git panel, with the PR viewer keys visible.
- Agent-agnostic labels on actions that hand a prompt to the agent.
- Prompts from the git panel arrive as one turn and are submitted.

**Non-Goals:**
- The Ship dialog hint lines (`ShipDialog.tsx:728`, `:814`, `:997`). The Ship dialog is a modal, not a git panel view.
- Opening a worktree session on a PR branch (FRI-610).
- Product names: "Claude Code" in setup, MCP and settings, and the "Claude" status provider.
- The FilesChip commit `Ctrl+Enter` (`FilesChip.tsx:358`) and the StashesChip `Ctrl+Enter` (`StashesChip.tsx:417`). Both bind inside text fields, where the global dispatcher already skips `fullscreen-terminal` (`useKeyboardShortcuts.ts:341`).

## Decisions

### D1. Merge moves to `M` with a confirmation

`M` (no Ctrl, Alt or Meta; Shift is allowed, as with `a`) in the PR viewer opens `confirm()` (`src/lib/confirm.ts`). Confirming merges the PR. The dialog states the target branch and the number of unresolved annotations. The viewer stops handling `Ctrl+Enter`.

This follows the bare-letter verb pattern of the plan panel. `X` there is confirm-gated because a bare letter is easy to hit by accident (`PlanPanel.tsx:226-275`). A merge is not reversible from Nergal, so it gets the same gate.

Guards, all in the viewer handler or the merge request:
- The handler returns when `e.defaultPrevented` or a leader chord is pending (`leaderPendingAtom`, `src/stores/leader.ts:20`). Both the global dispatcher and the viewer listen on `window` in capture phase, and their registration order changes when either effect re-runs. Checking both signals covers either order, so `leader m` (Complete Merge) never also opens the merge confirmation. The same guard fixes `leader a` opening an annotation today.
- `e.repeat` is ignored, and a `mergeConfirmOpenRef` blocks a second confirmation while one is pending. `confirm()` queues calls (`confirm.ts:63-64`), so without the ref a fast double press would queue two dialogs.
- The request reads `state`, `merging` and `performMerge` through refs that always hold the latest render's values, and checks `state === "OPEN" && !merging` before opening the dialog and again after the user confirms. It also captures `{ workspaceId, prNumber }` when the dialog opens and aborts if the viewer shows another PR when the user confirms: the viewer is not remounted per PR (`PrsChip.tsx:234`), so the latest `performMerge` would target the new PR. The keydown effect does not list these values in its deps, so a closure would see stale ones.
- The pure helper `prViewerKeyAction` receives `canMerge` (PR open and no merge running) so the gate is unit-tested.

The Merge button keeps its click flow. A click without annotations merges at once. A click with annotations shows the inline "merge anyway" strip. A click is deliberate, and the button text names the target branch.

Alternatives:
- Keep `Ctrl+Enter` and call `stopImmediatePropagation` in the viewer. Rejected: `Ctrl+Enter` would mean two things depending on the panel, and the viewer cannot rely on running before the global listener.
- A leader chord (`leader m`). Rejected: `leader m` already means "Complete Merge" (`shortcuts.ts:786`, shown in `GitPanel.tsx:462`).
- `M` without a confirmation. Rejected by the user.
- Reject `Shift+M`. Rejected: the chip renders an upper-case `M`, and `a` already accepts Shift.

### D2. One shared `HintBar` with 10px `Kbd` chips

Scope: the key-hint row at the bottom of each git panel view and of each file picker (the PR viewer picker and the Conflicts picker). Inline hints beside one control stay inline but move to 10px: the Conflicts pane headers (`ConflictsPanel.tsx:1456`) and the intent field's `Ctrl+Shift+R sends` (`ConflictsPanel.tsx:1057`).

A new `src/components/ui/hint-bar.tsx` exports `HintBar`. It renders a sticky bottom row: `sticky bottom-0 z-10 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 border-t border-border/40 bg-card/95 px-3 py-1 text-[10px] text-muted-foreground`. Its children are `Kbd` chips and labels. `Kbd` gains `size?: "sm" | "md"`: `sm` keeps today's 9px, and `md` renders 10px text. The hint bar uses `md`.

The chips stay because the user chose them: chips read faster than plain glyph text. `docs/patterns.md` §4 changes to bless "Kbd chips plus 10px labels in a hint bar". The rule against chips in input placeholders stays.

Alternatives:
- Plain glyph text, as §4 says today. Rejected by the user.
- Override the chip size with `className="text-[10px]"`. Rejected: `Kbd` already sets `text-[9px]`, and two font-size utilities on one element resolve by stylesheet order, not by class order.
- Fix each bar in place without a component. Rejected: the bars drifted apart because each view copied the classes.

### D3. One awaited submit command for git panel prompts

A new async Tauri command `deliver_prompt_to_session(session_id, text)` pastes the prompt as one bracketed paste (`paste_to_session(.., false)`, `pty.rs:2071`), waits 250 ms, then writes a lone CR (`submit_to_session`, `pty.rs:2097`). It resolves only after the CR is written and returns any paste or submit error. Linear and ClickUp "send to session" use the same two steps through `paste_and_submit_settled` (`pty.rs:2122`, called from `linear/mod.rs:452` and `clickup/mod.rs:828`), which avoids the race where an Enter in the same burst as the end-of-paste marker submits too early. That helper fires the CR on a detached task and drops its error (`pty.rs:2137`), so the git panel cannot know when the turn was sent.

Because the command resolves after the CR, the callers' in-flight guards cover the whole delivery: "Ask Agent" keeps its `sending` state, and "Apply with Agent" gains an `applyingRef` (a ref, because the `Ctrl+Shift+R` listener closes over stale state). "Ask Agent" moves focus to the terminal only after the delivery resolves, so keys typed during the settle cannot land between the paste and the CR. A double click or a repeated `Ctrl+Shift+R` within the 250 ms window no longer sends paste, paste, CR, CR.

"Apply with Agent" (`PrViewer.tsx:502`) and "Ask Agent" (`ConflictsPanel.tsx:421`) call it. `terminal_deliver` (`pty.rs:2008`) is removed: the PR viewer is its only caller.

Alternatives:
- Reuse `paste_and_submit_settled` as is. Rejected: it resolves before the CR and hides the submit error, so the success toast could lie and a double click doubles the prompt.
- Add a `submit` flag to `terminal_deliver`. Rejected: no caller wants a delivery without a submit, so the flag would be dead.
- Type a CR from the frontend after `terminal_deliver` resolves. Rejected: it splits one delivery across two IPC calls, and a session switch in between sends the CR elsewhere.
- Keep "Ask Agent" on `writeToSession(prompt + "\r")`. Rejected: an unbracketed multi-line prompt submits its first line as a turn.

### D4. Labels say "Agent"

Visible strings, the matching function names and the comments that name them change: `applyWithClaude` → `applyWithAgent`, `askClaude` → `askAgent`, `onAskClaude` → `onAskAgent` (`src/hooks/useConflictResolution.ts` and `ConflictsPanel.tsx`), `handleSendToClaude` → `handleSendToAgent`. Master specs `ship-flow` and `conflict-resolution` name the old labels too, so they get deltas. The spec requirement titles keep their names, so the archive matches the existing blocks.

## Risks / Trade-offs

- [A user with `Ctrl+Enter` merge in muscle memory toggles fullscreen instead] → the Merge button chip shows `M`, and the PR viewer hint bar lists `M`.
- [`M` fires while focus sits in the terminal] → the viewer handler already returns when the target is a text field (`PrViewer.tsx:601-608`), and the terminal input is a textarea.
- [The 250 ms settle is a heuristic] → the same value already serves Linear and ClickUp delivery without reported failures.
- [Text the user left half-typed in Claude Code's input is submitted together with the prompt] → accepted: the user chose an immediate submit, the same behavior Linear and ClickUp delivery already have.
- [`deliver_prompt_to_session` is a new IPC entry point that types into a PTY] → it accepts the same inputs `terminal_deliver` accepted, and it refuses auxiliary shells through `paste_to_session`.


===== implementation.md =====
# Implementation Plan: git-panel-agent-actions

> Grounded in the codebase at `05c263b` (main, 2026-11-24). Behaviour verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- PR viewer keyboard: one `window` keydown listener in capture phase (effect `src/components/git/PrViewer.tsx:597-696`), live only while `isActive && listenerActive` (`:597-598`, `listenerActive` at `:250-252`). It returns early in annotation mode, while any `[data-slot="dialog-content"]` exists, and when the target is an input, textarea, `.cm-editor` or contenteditable (`:601-608`). It does not check `e.defaultPrevented` or `leaderPendingAtom`. `a` calls `startAnnotating()` (`:665-671`). `Ctrl+Enter` calls `handleMergeClick()` (`:672-677`). `Ctrl+←/→` change file (`:679-690`). The effect deps list is at `:696`; it omits `state`, `merging`, `baseRefName`, `prNumber`, `title` and `performMerge`.
- Merge: `handleMergeClick()` (`PrViewer.tsx:585-592`) returns unless `state === "OPEN"`, shows the inline "merge anyway" strip when annotations exist, else calls `performMerge()` (`:519-582`). `merging` state disables the button (`:1207`).
- Merge button chip: `<Kbd keys="ctrl+enter" />` at `PrViewer.tsx:1214`. Apply button label at `:1160`, its `ctrl+shift+r` chip at `:1161`, tooltip strings at `:1166-1170`.
- Apply: `applyWithClaude()` (`PrViewer.tsx:478-517`) builds the prompt and calls `invoke("terminal_deliver", { sessionId, text })` (`:502`), toast title "Apply with Claude" (`:505`). `onApply()` at `:721` wraps it for the `ctrl+shift+r` route (`:716` comment).
- PR file picker header hint: `PrViewer.tsx:1083-1090`, a sticky top header with a 9px plain-text hint `j/k move · Enter pick · Esc close`.
- PR viewer footer: `PrViewer.tsx:1128` (`flex shrink-0 items-center gap-2 border-t ... px-3 py-1.5`). The diff body sits above it.
- `terminal_deliver` (`src-tauri/src/pty.rs:2008-2014`) calls `paste_to_session(&state, &session_id, &text, false)`. Registered in `src-tauri/src/lib.rs:533`. Its only frontend caller is `PrViewer.tsx:502` (grep `terminal_deliver` in `src/`).
- `paste_and_submit_settled(app, session_id, text)` (`pty.rs:2122-2141`): `paste_to_session(.., false)` then, on a detached async task, sleeps 250 ms and calls `submit_to_session` (writes `\r`, `pty.rs:2097-2113`), dropping its error (`:2137`). It returns before the CR. `paste_to_session` (`pty.rs:2071`) rejects auxiliary shell ids (test `paste_to_session_rejects_aux_shells`, `pty.rs:2681`).
- Conflicts "Ask": `askClaude` (`src/components/git/ConflictsPanel.tsx:407-429`) invokes `build_conflict_prompt`, focuses the terminal, then `terminalService.writeToSession(sessionId, prompt + "\r")` (`:421`), toast "Sent to Claude" (`:422`). Exposed as `onAskClaude` (`:483`), consumed by `src/hooks/useConflictResolution.ts:81`, `:117`, `:265`, `:272`. The Conflicts file picker (`ConflictsPanel.tsx:1084-1090`) copies the PR viewer picker header hint; inline 9px hints sit at `:1057` (`Ctrl+Shift+R sends`) and `:1456` (pane header). Labels at `:1052` (comment), `:1056` ("Intent for Claude (optional)"), `:1068` ("Ask Claude"). Hint line at `:935-937` (9px plain text, ends with "ask Claude"), inside the toolbar block, not at the view bottom.
- SpecPanel: `handleSendToClaude` (`src/components/spec/SpecPanel.tsx:339`) queues feedback via `set_pending_annotations`; label "Send to Claude" at `:559`; callers `:247`, `:551`.
- Plan labels: `PlanPanel.tsx:157` toast description "Claude will revise the plan"; `PlanAnnotationToolbar.tsx:403` "Waiting for Claude to revise...". `PlanPanel.tsx:121` names "Claude Code" (product name, stays).
- Hint bars today: `FilesChip.tsx:331-333`, `HistoryChip.tsx:194-195`, `BranchesChip.tsx:462-463` (sticky, 9px, `gap-1`, no wrap); `StashesChip.tsx:419-423` (inline under the Save button, only when `stashes.length > 0`); `PrsChip.tsx:315-317` (list footer, not sticky, no flex, 9px) and a duplicate header hint `PrsChip.tsx:258-260`.
- `Kbd` (`src/components/ui/kbd.tsx`): fixed `h-4 px-1 font-mono text-[9px]`, tones `subtle` / `onPrimary`, no size prop. `formatKey` upper-cases single letters, so `keys="m"` renders `M`.
- Leader: `leaderPendingAtom` (`src/stores/leader.ts:20`); the global dispatcher resolves a chord with `preventDefault` + `stopPropagation` (`useKeyboardShortcuts.ts:119-123`); `leader m` is `complete-merge` (`shortcuts.ts:786`). Both it and the PR viewer listen on `window` capture, and their order changes when either effect re-registers (`useKeyboardShortcuts.ts:369`).
- Comments naming the old symbols: `PrViewer.tsx:264` ("Apply-with-Claude"), `:716` ("applyWithClaude").
- `confirm()` (`src/lib/confirm.ts`) returns `Promise<boolean>` and queues a call made while another is active (`:63-64`); ConfirmHost confirms on Enter (`ConfirmHost.tsx:71-75`); `body` is HTML and callers escape user text with `escapeHtml` (`src/lib/escapeHtml.ts`). The dialog renders a `DialogContent` (`src/components/ui/ConfirmHost.tsx:62`), so the PR viewer listener is inert while it is open.
- Global `Ctrl+Enter` is `fullscreen-terminal` (`src/stores/shortcuts.ts:704`), dispatched at window capture by `useKeyboardShortcuts.ts`; it is skipped only inside non-terminal text fields (`useKeyboardShortcuts.ts:341`).
- Pure key helpers live in `src/lib/` with vitest files beside them (`gitChipKeys.ts` + `gitChipKeys.test.ts`). Vitest runs with `pnpm test`.

## Execution order

1. Backend: `deliver_prompt_to_session` in `pty.rs`, register in `lib.rs`, remove `terminal_deliver`.
2. `Kbd` `size` prop, then `HintBar` component.
3. PR viewer: key helper (`src/lib/prViewerKeys.ts`), `M` + confirm, hint bar, picker hint bar, Apply via the new command, labels.
4. Conflicts: Ask via the new command, labels, hint bar at the view bottom.
5. Chips: Files, History, Branches, Stashes, PRs list on `HintBar`.
6. Label sweep: SpecPanel, PlanPanel, PlanAnnotationToolbar.
7. Docs: `docs/patterns.md` §4 (+ the bare-letter precedents line `:406` and `:96` add `M`), `docs/design.md` §3.4 (`size` prop + `HintBar`).

## Plan

- **`src-tauri/src/pty.rs`**: add `pub(crate) async fn deliver_prompt(pty: &PtyManager, session_id: &str, text: &str, settle: Duration) -> Result<(), String>`: `paste_to_session(pty, .., false)?`, `tokio::time::sleep(settle).await`, `submit_to_session(pty, ..)`. The command `#[tauri::command] pub async fn deliver_prompt_to_session(state: State<'_, PtyManager>, session_id: String, text: String) -> Result<(), String>` calls it with the shared settle constant. Tests call `deliver_prompt` on `PtyManager::new()` for the error paths, and on a manager holding a real `PtyInstance` whose `writer` is a capturing `SharedWriter` (`pty.rs:16`; instance construction as in `pty_pixel_tests.rs:37-62`) for the happy path. Keep the 250 ms in one `const` shared with `paste_and_submit_settled`. Delete `terminal_deliver`. `lib.rs:533`: swap the registration.
- **`src/components/ui/kbd.tsx`**: `size?: "sm" | "md"` (default `sm`); `sm` → `text-[9px]`, `md` → `text-[10px]`. Keep `h-4`.
- **`src/components/ui/hint-bar.tsx`** (new): `HintBar({ children, className })` renders a `div` with the D2 classes; `Hint({ keys, children })` renders `<Kbd size="md" keys=…/>` chips (one per key, `keys` may be an array) plus the label. Callers compose `<HintBar><Hint keys={["j","k"]}>hunk</Hint>…</HintBar>`.
- **`src/lib/prViewerKeys.ts`** (new): `prViewerKeyAction(e: { code; ctrlKey; metaKey; altKey; shiftKey; repeat }, ctx: { pickerOpen; hunkCount; fileCount; canMerge }) → "picker-close" | "picker-next" | "picker-prev" | "picker-pick" | "hunk-next" | "hunk-prev" | "toggle-collapse" | "annotate" | "merge" | "file-prev" | "file-next" | null`. The handler builds `ctx.canMerge` from the refs (`stateRef.current === "OPEN" && !mergingRef.current`), never from the effect closure. `KeyM` without Ctrl/Alt/Meta (Shift allowed), `!e.repeat` and `canMerge` → `"merge"`; `Enter` with Ctrl or Meta → `null`; `Space` without modifiers and with hunks → `"toggle-collapse"`. The component returns before the helper when `e.defaultPrevented` or `appStore.get(leaderPendingAtom)` is set. `PrViewer.tsx` handler maps the action to the existing calls. State checks that need component state (`annotating`, dialog open, in-field) stay in the component.
- **`PrViewer.tsx`**: refs `stateRef`, `mergingRef`, `performMergeRef` updated every render, and `mergeConfirmOpenRef`. `"merge"` → `requestMergeConfirm()`: returns unless `mergeConfirmOpenRef` is false and `stateRef.current === "OPEN" && !mergingRef.current`; sets `mergeConfirmOpenRef`; captures `{ workspaceId, prNumber }` from `prIdentityRef`; `await confirm({ title: "Merge PR #N?", body: "Merge into <b>base</b>." + (annotations ? " N annotations are unresolved." : ""), confirmLabel: "Merge", destructive: true })` with `escapeHtml(baseRefName)`; clears `mergeConfirmOpenRef` when it settles; on true re-checks `stateRef`/`mergingRef` and that `prIdentityRef.current` still equals the captured identity (the viewer is not keyed per PR, `PrsChip.tsx:234`, `ZenMode.tsx:176`), then `performMergeRef.current()`. Merge button chip `keys="m"`. Add `HintBar` at the bottom of the diff area (above the footer) with `j/k` hunk, `a` annotate, `Ctrl+←/→` file, `M` merge. Picker: replace the header hint with a `HintBar` at the picker bottom (`j/k` move, `↵` pick, `Esc` close). `applyWithClaude` → `applyWithAgent`, invoke `deliver_prompt_to_session` behind an `applyingRef` read and set synchronously on entry (the `Ctrl+Shift+R` listener at `:719-725` has stale deps), with an `applying` state only for the button's `disabled`, toast title "Apply with Agent", label "Apply with Agent (N)". Update the comments at `:264` and `:716`.
- **`ConflictsPanel.tsx`** + **`src/hooks/useConflictResolution.ts`**: `askAgent` invokes `deliver_prompt_to_session` instead of `writeToSession` (its `sending` state now spans the whole delivery; focus moves to the terminal only after the delivery resolves, so keys typed during the settle do not land between paste and CR); toast "Sent to agent"; `onAskAgent`; the Conflicts file picker hint moves to a `HintBar` at the picker bottom; the inline hints at `:1057` and `:1456` move to 10px; labels "Intent for the agent (optional)" and "Ask Agent"; hint line → `HintBar` at the bottom of the view with chips (`↑↓`/`j k` region, `o`/`t`/`b` region, `Ctrl+Shift+O`/`T` all, `s` sync, `Ctrl+Shift+K` picker, `Ctrl+Shift+↵` save, `Ctrl+Shift+R` ask agent).
- **Chips**: replace each bar's `div` with `HintBar`, keeping its hints. Stashes: move its hints into a `HintBar` at the bottom of the view; it still renders only when `stashes.length > 0`. PRs list: footer → `HintBar`; drop the duplicate header hint (`PrsChip.tsx:258-260`).
- **Labels**: SpecPanel `handleSendToAgent` + "Send to Agent"; PlanPanel "The agent will revise the plan"; PlanAnnotationToolbar "Waiting for the agent to revise...".

## Per-phase risk

- [Removing `terminal_deliver` breaks a caller missed by the grep] → grep `terminal_deliver` across `src/` and `src-tauri/` after the change; `tsc` does not check `invoke` names, so the grep is the gate.
- [The PR viewer effect captures stale `merging`/`state`/`performMerge`] → read them through refs and re-check after the confirm resolves.
- [A sticky `HintBar` inside a non-scrolling flex column does not stick] → place it as the last child of the scroll container, or as a `shrink-0` sibling after it; check each view in the walk.
- [`M` pressed twice quickly queues two confirms (`confirm.ts:63-64`)] → `mergeConfirmOpenRef` and the `e.repeat` check.
- [`leader m` also reaches the viewer listener] → the `defaultPrevented` / `leaderPendingAtom` guard, covered by walk 6.3.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main git-panel-agent-actions`.
- Grep: no `terminal_deliver` in `src/` or `src-tauri/src/`; no "Claude" in the agent-action strings listed in the proposal.
- Manual walk on the installed `.deb` (tasks group Verification).


===== tasks.md =====
## 1. Prompt delivery that submits

- [ ] 1.1 Test (Red): `src-tauri/src/pty_deliver_prompt_tests.rs` (`#[path]` module of `pty.rs`, `#[tokio::test]`): `deliver_prompt` on `PtyManager::new()` returns an error for an auxiliary shell id (`"sess::shell"`) and for a session with no PTY (`"sess-1"`), writing nothing. Happy path: a manager whose session `sess-1` maps to a real `PtyInstance` with a capturing `SharedWriter` (construction as in `pty_pixel_tests.rs:37-62`); `deliver_prompt(.., "x\ny", 10 ms).await` returns `Ok`, and once it resolves the buffer equals `ESC[200~x\nyESC[201~` followed by `\r`, with the `\r` as the last byte.
- [ ] 1.2 Add `deliver_prompt` and the async command `deliver_prompt_to_session(session_id, text)` in `src-tauri/src/pty.rs` (paste, settle, CR, resolve after the CR, return any error); share the 250 ms settle constant with `paste_and_submit_settled`; delete `terminal_deliver`; swap the registration in `src-tauri/src/lib.rs`.
- [ ] 1.3 `src/components/git/PrViewer.tsx`: `applyWithClaude` → `applyWithAgent`, invoking `deliver_prompt_to_session` behind an `applyingRef` checked and set synchronously on entry (covers the stale `Ctrl+Shift+R` listener) plus an `applying` state for the button's `disabled`; toast title "Apply with Agent"; button label "Apply with Agent (N)"; update the comments at `:264` and `:716`.
- [ ] 1.4 `src/components/git/ConflictsPanel.tsx` + `src/hooks/useConflictResolution.ts`: `askClaude` → `askAgent`, `onAskClaude` → `onAskAgent`, invoking `deliver_prompt_to_session` instead of `writeToSession(prompt + "\r")`; focus moves to the terminal after it resolves; toast "Sent to agent".

## 2. Merge with M

- [ ] 2.1 Test (Red): `src/lib/prViewerKeys.test.ts` for `prViewerKeyAction(e, { pickerOpen, hunkCount, fileCount, canMerge })`: `KeyM` and `Shift+M` with `canMerge` → `"merge"`; `KeyM` with `canMerge: false`, with `repeat: true`, with Ctrl, Alt or Meta → `null`; `Ctrl+Enter` and `Meta+Enter` → `null`; `KeyA` → `"annotate"` with hunks and `null` without; `j`/`k`/`↓`/`↑` → hunk next/prev with hunks; `Space` → `"toggle-collapse"` with hunks, `null` with any modifier; `Ctrl+←/→` → file prev/next only with 2+ files; picker open: `Escape` → `"picker-close"`, `j`/`k` → picker next/prev with files, `Enter` → `"picker-pick"`, `KeyM` → `null`.
- [ ] 2.2 Add `src/lib/prViewerKeys.ts` with `prViewerKeyAction` and route the `PrViewer.tsx` keydown handler through it. The component keeps the annotation, dialog and in-field guards and returns first when `e.defaultPrevented` or `leaderPendingAtom` is set.
- [ ] 2.3 `"merge"` → `requestMergeConfirm()` per implementation.md (latest-value refs for `state`, `merging`, `performMerge`; `mergeConfirmOpenRef`; `confirm()` "Merge PR #N?" with the target branch escaped by `escapeHtml` and the unresolved annotation count; confirm label "Merge"; re-check state and that `{ workspaceId, prNumber }` is unchanged after confirm; `ctx.canMerge` built from the refs). The Merge button chip becomes `keys="m"`.

## 3. Shared hint bar

- [ ] 3.1 Screenshot check: each git panel view against `Git panel hint bars share one standard` and `The PR viewer lists its keys` (walk 6.4).
- [ ] 3.2 `src/components/ui/kbd.tsx`: add `size?: "sm" | "md"` (`sm` = 9px default, `md` = 10px).
- [ ] 3.3 Add `src/components/ui/hint-bar.tsx` (`HintBar` + `Hint`) with the classes from design D2.
- [ ] 3.4 PR viewer: a `HintBar` below the diff with `j/k` hunk, `a` annotate, `Ctrl+←/→` file, `M` merge; the file picker hint moves to a `HintBar` at the picker bottom.
- [ ] 3.5 `FilesChip`, `HistoryChip`, `BranchesChip`, `StashesChip` and the `PrsChip` list footer use `HintBar`; drop the duplicate PRs header hint (`PrsChip.tsx:258-260`); Stashes hints move to the bottom of the view.
- [ ] 3.6 `ConflictsPanel.tsx`: the hint line becomes a `HintBar` at the bottom of the view, with chips and "ask agent"; the Conflicts file picker hint (`:1084-1090`) moves to a `HintBar` at the picker bottom; the inline hints at `:1057` and `:1456` move to 10px.

## 4. Agent labels

- [ ] 4.1 `ConflictsPanel.tsx`: "Intent for the agent (optional)", "Ask Agent", and the `{/* Intent + Ask Claude */}` comment.
- [ ] 4.2 `src/components/spec/SpecPanel.tsx`: `handleSendToClaude` → `handleSendToAgent`, label "Send to Agent".
- [ ] 4.3 `src/components/plan/PlanPanel.tsx`: "The agent will revise the plan"; `src/components/plan/PlanAnnotationToolbar.tsx`: "Waiting for the agent to revise...".

## 5. Docs

- [ ] 5.1 `docs/patterns.md` §4: hint bars use `HintBar` (sticky bottom, 10px labels, 10px `Kbd` chips, wraps); placeholders keep plain glyph text. Add `M` (confirm-gated) to the PrViewer bare-letter precedents (`patterns.md:96`, `:406`).
- [ ] 5.2 `docs/design.md` §3.4: document the `Kbd` `size` prop and `HintBar`.

## 6. Verification

- [ ] 6.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [ ] 6.2 `~/.claude/skills/work/scripts/work-gates.sh main git-panel-agent-actions` passes; grep finds no `terminal_deliver`, `applyWithClaude`, `askClaude` or `handleSendToClaude` in `src/` or `src-tauri/src/`.
- [ ] 6.3 Manual walk: in the PR viewer of an open PR, `M` opens "Merge PR #N?"; Cancel merges nothing; holding `M` opens one dialog; `Ctrl+Enter` toggles the fullscreen terminal and opens no merge confirmation; `Ctrl+Space` then `m` opens no merge confirmation.
- [ ] 6.4 Manual walk: every git panel view (Files, History, Branches, Stashes, PRs list, PR viewer, PR file picker, Conflicts, Conflicts file picker) shows the same bottom hint bar with 10px chips; the PR viewer bar lists `j/k`, `a`, `Ctrl+←/→` and `M`; a narrow panel wraps the Branches bar.
- [ ] 6.5 Manual walk: `M` on a merged PR opens nothing; with focus in the terminal, `m` types into the terminal; with 2 annotations, the confirmation body says 2 annotations are unresolved.
- [ ] 6.6 Manual walk: annotate a PR hunk and press "Apply with Agent": the agent receives the multi-line prompt as one turn and starts without an Enter; a double click, or `Ctrl+Shift+R` twice in a row, sends it once. In a conflict, "Ask Agent" with an intent does the same.
- [ ] 6.7 Manual walk: the SpecPanel button reads "Send to Agent"; a plan Revise shows "The agent will revise the plan".


===== handoff/.work-modules.json =====
{
  "tier": "S",
  "ceremony": "quick-fix",
  "risk_tier": "low",
  "spec_target": "git-panel-v2, terminal-wezterm, conflict-resolution, spec-annotations, ship-flow",
  "tracker": {
    "system": "linear",
    "id": "FRI-609",
    "pending": []
  },
  "validation": "pending",
  "planning_only": false,
  "iprev": {
    "rounds": 0,
    "verdict": null
  },
  "interview_answers": {
    "merge_key": "M with a confirmation dialog in the PR viewer; Ctrl+Enter only toggles fullscreen (user, 2026-11-24)",
    "hint_bars": "Kbd chips at 10px in a sticky bottom bar with background; update docs/patterns.md section 4 to bless it (user, 2026-11-24)",
    "labels": "Claude -> Agent on agent-facing actions; product names (Claude Code) and the Claude status provider stay (user, 2026-11-24)",
    "apply_submit": "Apply with Agent submits immediately, like Linear/ClickUp send-to-session (user, 2026-11-24). Mechanism refined in iprev r1: an awaited deliver_prompt_to_session (paste, 250 ms settle, CR, resolve after CR) instead of the fire-and-forget paste_and_submit_settled, so in-flight guards and error toasts are truthful.",
    "worktree_for_pr": "out of scope, FRI-610"
  },
  "events": []
}

===== specs/conflict-resolution/spec.md =====
## MODIFIED Requirements

### Requirement: Conflict tab toolbar actions
The conflict tab SHALL expose four toolbar actions: Accept Ours, Accept Theirs, Ask Agent, Save Resolution. The intent field above Ask Agent is labelled "Intent for the agent (optional)".

#### Scenario: Accept ours copies ours to merged
- **WHEN** user clicks "Accept Ours"
- **THEN** the merged panel contents are replaced with the ours panel contents

#### Scenario: Accept theirs copies theirs to merged
- **WHEN** user clicks "Accept Theirs"
- **THEN** the merged panel contents are replaced with the theirs panel contents

#### Scenario: Ask Claude to resolve injects context and switches focus
- **WHEN** user clicks "Ask Agent" (or presses `Ctrl+Shift+R`) with an optional intent
- **THEN** the backend builds the conflict prompt from `{ path, ours, theirs, merged, intent }` (`build_conflict_prompt`)
- **AND** `deliver_prompt_to_session` pastes it into the session as one bracketed paste and submits it, so a multi-line prompt arrives as one turn
- **AND** focus switches to the terminal and a toast reads "Sent to agent"

#### Scenario: Save resolution writes, stages, closes, re-checks
- **WHEN** user clicks "Save Resolution"
- **THEN** the merged contents are written to the file on disk, the file is staged, the conflict tab is closed, and the git panel's conflict list is refreshed; if no conflicts remain, the conflict list section disappears

### Requirement: Contextual resolve-conflict shortcut dispatch
The shortcut currently bound to revise-plan SHALL be converted to a contextual dispatcher that emits one of two events depending on the active context, without introducing a new key binding.

#### Scenario: Active tab is conflict type
- **WHEN** user presses the contextual shortcut while a `conflict` tab is active
- **THEN** the system emits `nergal:resolve-conflict-active-tab` (triggers "Ask Agent" on the active tab)

#### Scenario: Git panel with conflicts but no conflict tab active
- **WHEN** user presses the contextual shortcut while the git panel is the active tab and the session has one or more conflicted files
- **THEN** the system opens/focuses the conflict tab for the first conflicted file

#### Scenario: Plan panel with pending review falls back to revise-plan
- **WHEN** user presses the contextual shortcut while a plan panel is active and a plan review is pending
- **THEN** the system emits `nergal:revise-plan` (current behavior)

#### Scenario: Neither context active is a no-op
- **WHEN** none of the above conditions apply
- **THEN** the system emits no event and shows no toast (silent no-op)


===== specs/git-panel-v2/spec.md =====
## ADDED Requirements

### Requirement: PR viewer merges with M after a confirmation
In the PR viewer, `M` (with or without Shift, and without Ctrl, Alt or Meta) SHALL open a confirmation dialog for merging the displayed PR, and SHALL merge only when the user confirms. The PR viewer SHALL NOT handle `Ctrl+Enter`; `Ctrl+Enter` keeps its global meaning (toggle the fullscreen terminal). The Merge button SHALL show `M` as its shortcut chip.

#### Scenario: M asks before merging
- **GIVEN** the PR viewer shows open PR #7 into `main` with no annotations
- **WHEN** the user presses `M`
- **THEN** a confirmation dialog titled "Merge PR #7?" names `main` as the target
- **AND** confirming runs the merge; cancelling merges nothing

#### Scenario: M with unresolved annotations
- **GIVEN** the PR viewer shows open PR #7 with 3 annotations
- **WHEN** the user presses `M`
- **THEN** the confirmation body states that 3 annotations are unresolved
- **AND** confirming merges the PR

#### Scenario: Ctrl+Enter no longer merges
- **GIVEN** the PR viewer is active and focus is not in a text field
- **WHEN** the user presses `Ctrl+Enter`
- **THEN** no merge starts and no merge confirmation opens
- **AND** the fullscreen terminal toggles

### Requirement: The M merge key is guarded
`M` SHALL do nothing while the PR is not open, while a merge runs, while a dialog or a merge confirmation is open, in annotation mode, while focus is in a text field, on a key repeat, or when the keypress completes a leader chord. The PR state SHALL be checked again after the user confirms.

#### Scenario: Leader m does not open the merge confirmation
- **GIVEN** the PR viewer is active
- **WHEN** the user presses `Ctrl+Space` then `m` (Complete Merge)
- **THEN** no merge confirmation opens

#### Scenario: A held M opens one confirmation
- **GIVEN** the PR viewer shows open PR #7
- **WHEN** the user holds `M` so the key repeats
- **THEN** exactly one confirmation opens

#### Scenario: PR merged while the confirmation is open
- **GIVEN** the merge confirmation for PR #7 is open
- **WHEN** a refresh marks PR #7 as merged and the user then confirms
- **THEN** no merge request is sent

#### Scenario: Displayed PR changes while the confirmation is open
- **GIVEN** the merge confirmation for PR #7 is open
- **WHEN** the viewer switches to PR #9 and the user then confirms
- **THEN** no merge request is sent for #7 or #9

#### Scenario: M is inert when the PR is closed
- **GIVEN** the PR viewer shows a merged or closed PR
- **WHEN** the user presses `M`
- **THEN** no confirmation opens

### Requirement: Git panel hint bars share one standard
Every keyboard hint bar in the git panel SHALL use one shared hint bar: a row pinned to the bottom of its view (sticky, with an opaque card background and a top border) that wraps when narrow, with 10px labels and 10px `Kbd` chips. This SHALL apply to the bottom hint row of the Files, History, Branches, Stashes, PRs list, Conflicts and PR viewer views and of the PR viewer and Conflicts file pickers.

#### Scenario: PRs list footer matches the other views
- **WHEN** the PRs list renders its footer
- **THEN** the footer is the shared hint bar, with 10px text and chips, pinned to the bottom of the list

#### Scenario: Narrow panel wraps the hint bar
- **GIVEN** the Branches view hint bar holds 7 hints
- **WHEN** the panel is narrower than the hints
- **THEN** the hints wrap onto a second row instead of being clipped

### Requirement: The PR viewer lists its keys
The PR viewer hint bar SHALL list `j/k` (hunks), `a` (annotate the hunk), `Ctrl+←/→` (files) and `M` (merge). A hint beside a single control in the git panel (a pane header or a field label) MAY stay inline and SHALL use 10px text.

#### Scenario: PR viewer shows its keys
- **GIVEN** the PR viewer shows an open PR with at least one hunk
- **THEN** its hint bar shows chips for `j`/`k`, `a`, `Ctrl+←`/`Ctrl+→` and `M` with their labels

## MODIFIED Requirements

### Requirement: PRs chip is a picker → embedded viewer
The `PRs` chip SHALL render a file-picker-style PR list (j/k/Enter to select). Selecting a PR replaces the chip body with the `PrViewer` embedded in place plus a "All PRs" back-strip. `Backspace` returns to the picker.

#### Scenario: Backspace returns to picker
- **WHEN** the user is viewing a PR (`selected !== null`)
- **AND** presses `Backspace` outside an input/textarea
- **THEN** the chip returns to picker mode

#### Scenario: Apply with Claude tooltip on disabled
- **WHEN** the Apply-with-Agent button is disabled (no owning session, or owning session not active)
- **AND** the user hovers it
- **THEN** a Tooltip surfaces the specific gating reason (no matching branch / wrong active session)
- **AND** the button label includes the annotation count: `Apply with Agent (N)`

#### Scenario: Apply with Agent submits the prompt
- **GIVEN** the PR viewer has 2 annotations and the owning session is active
- **WHEN** the user clicks "Apply with Agent (2)" (or presses `Ctrl+Shift+R`)
- **THEN** `deliver_prompt_to_session` receives the structured prompt for that session
- **AND** the agent receives it as one turn, with no Enter needed from the user
- **AND** a second click or `Ctrl+Shift+R` before the delivery resolves sends nothing
- **AND** a toast titled "Apply with Agent" reports 2 annotations sent


===== specs/ship-flow/spec.md =====
## MODIFIED Requirements

### Requirement: Apply-with-Claude tooltip explains the disabled state
The Apply-with-Agent button SHALL surface its disabled-state reason via a Tooltip component (not a native browser title). The button label SHALL include the annotation count inline (`Apply with Agent (N)`).

#### Scenario: Tooltip on no matching session
- **WHEN** no local session has a worktree branch matching the PR's `head_ref_name`
- **THEN** the button is disabled and the tooltip reads "No local session matches this PR's branch (`<headRefName>`). Open a session whose worktree is on this branch to apply."

#### Scenario: Tooltip on wrong active session
- **WHEN** an owning session exists but is not the active session
- **THEN** the tooltip reads "Switch to the session on branch `<headRefName>` — its terminal will receive the prompt."

#### Scenario: Tooltip on enabled state
- **WHEN** the owning session is active
- **THEN** the tooltip reads "Send N annotation(s) as a structured prompt to the session terminal."


===== specs/spec-annotations/spec.md =====
## MODIFIED Requirements

### Requirement: Send annotations to Claude via chat injection

The `SpecPanel` SHALL provide a "Send to Agent" action in the annotation toolbar that serializes all annotations for the currently viewed spec (or other artifact) and queues them into `HookState.pending_annotations` via the `set_pending_annotations` Tauri command. The serialized payload SHALL include the artifact's repo-relative path so Claude can read the file directly. Delivery SHALL happen via the existing `inject-edits` (`UserPromptSubmit`) hook — NOT via the plan-review FIFO.

#### Scenario: Send to Claude queues feedback

- **WHEN** user has one or more annotations on a spec and clicks "Send to Agent" (or presses `Ctrl+Shift+R`)
- **THEN** the system SHALL call `serializeSpecAnnotations(annotations, { changeName, artifactPath, isMaster })`
- **AND** the serialized result SHALL be passed to `set_pending_annotations`
- **AND** the local annotations for that scope SHALL be cleared (in-memory atom + SQLite for that `spec_key`)
- **AND** annotation mode SHALL exit
- **AND** a toast SHALL confirm "Feedback queued — Will be sent in your next prompt"

#### Scenario: Master spec vs change-scoped spec context

- **WHEN** the spec being annotated is under `openspec/specs/<capability>/spec.md` (master spec)
- **THEN** the serialized payload header SHALL read `Review the OpenSpec capability spec and address my annotations.`
- **AND** the `Change:` field SHALL read `(master specs)`
- **AND** the `File:` field SHALL contain the master-spec path
- **WHEN** the spec being annotated is under `openspec/changes/<change>/...`
- **THEN** the header SHALL read `Review the OpenSpec change artifact and address my annotations.`
- **AND** the `Change:` field SHALL contain the change slug
- **AND** the `File:` field SHALL contain the change-scoped path

#### Scenario: Next user prompt injects feedback

- **WHEN** the user sends their next message in the active session after queuing spec feedback
- **THEN** the `inject-edits` hook SHALL read `pending_annotations` and append the serialized feedback to the prompt submitted to Claude
- **AND** `pending_annotations` SHALL be cleared after injection


===== specs/terminal-wezterm/spec.md =====
## MODIFIED Requirements

### Requirement: Selection and clipboard
The terminal SHALL support mouse-drag selection, keyboard copy, and paste. A user paste SHALL be wrapped in bracketed-paste markers only while the running app has enabled bracketed paste (DECSET 2004). Otherwise the text SHALL be written without markers: every newline (`\r\n`, `\n` or `\r`) written as `\r`, then embedded bracketed-paste markers removed. A delivery to an agent session (`paste_to_session`, and the `deliver_prompt_to_session` command the git panel's "Apply with Agent" and "Ask Agent" use) SHALL stay wrapped whatever the mode, so a multi-line body arrives as one paste. `deliver_prompt_to_session` SHALL then write a single CR as a separate write after a settle delay, so the agent submits the whole paste as one turn, and SHALL resolve only after that CR is written, returning any paste or submit error.

#### Scenario: Drag to select
- **WHEN** the user mouse-downs on a cell and drags to another cell
- **THEN** the range SHALL be visually highlighted
- **AND** on mouseup the selection SHALL remain highlighted until a new selection is started or input clears it

#### Scenario: Copy selection to clipboard
- **WHEN** the user presses Ctrl+Shift+C (or the configured copy shortcut) with an active selection
- **THEN** the selected cells SHALL be serialized to text and written to the system clipboard

#### Scenario: Bracketed paste
- **GIVEN** the app wrote `ESC[?2004h`
- **WHEN** the user presses Ctrl+Shift+V with clipboard text `a` + LF + `b`
- **THEN** the PTY SHALL receive `ESC[200~a` + LF + `bESC[201~`

#### Scenario: Plain paste without DECSET 2004
- **GIVEN** the app has not enabled bracketed paste, or wrote `ESC[?2004l`
- **WHEN** the user pastes `a` + CR + LF + `b` + LF + `c` + `ESC[201~` + `d`
- **THEN** the PTY SHALL receive `a` + CR + `b` + CR + `cd`

#### Scenario: Agent deliveries stay bracketed
- **GIVEN** the agent session's app has not enabled bracketed paste
- **WHEN** `paste_to_session` or `deliver_prompt_to_session` delivers `x` + LF + `y`
- **THEN** the PTY SHALL receive `ESC[200~x` + LF + `yESC[201~`

#### Scenario: Prompt delivery submits after the paste
- **GIVEN** an agent session with a live PTY
- **WHEN** `deliver_prompt_to_session` delivers `x` + LF + `y`
- **THEN** the PTY SHALL first receive `ESC[200~x` + LF + `yESC[201~`
- **AND** about 250 ms later it SHALL receive one separate write holding only CR
- **AND** the command SHALL resolve after that CR write, not before

#### Scenario: Prompt delivery to a shell tab is refused
- **WHEN** `deliver_prompt_to_session` targets an auxiliary shell id (one containing `::`)
- **THEN** it SHALL return an error and write nothing to any PTY

---
Re-evaluate. Focus on whether the round 2 issues were addressed, and on new defects the edits introduced.