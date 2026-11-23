# REVIEW: plan-decision-delivery (FRI-604)

## Adversarial

Scope: `git diff main -- src src-tauri`, the five new files, and the change artifacts. I traced the hook side in `src-tauri/src/hooks/cli.rs:206-452` and the path builder in `src-tauri/src/platform/mod.rs:69,329`.

Checked and sound:
- The FIFO helper handles open flags, the ENXIO retry, EINTR, EAGAIN with poll, partial writes and POLLERR correctly. `OwnedFd` closes the fd on every path.
- The claim and the hook build the path from the same `ipc_dir()` string. No canonicalization gap exists on Linux.
- The panel sends the Nergal session id. The registry uses the same key.
- Both `plan:ready` emitters resolve content from the same `tool_input`. A late preview matches `original` and keeps the review.

### Bugs

**Important. Approve and Revise show on any plan tab, and a failure there ends the live review.**
- `PlanPanel.tsx:43` derives `canAnnotate` from the session status only.
- The buttons at `PlanPanel.tsx:280-366` render for any document the panel shows.
- `PlanListView.tsx:66-80` opens an older plan and keeps that document's old `decisionPath`, or leaves it empty.
- During a live review, Approve on that tab fails the guard or the claim.
- `failDecision` then sets the session to `idle` (`PlanPanel.tsx:110`).
- That hides the buttons on the real review's tab, while the hook still waits.
- This breaks the design goal "The panel shows Approve and Revise only for a review that has a decision path" (design.md, Goals).
- Fix: gate the buttons and keys on the document's `decisionPath` matching the session plan's `decisionPath`. Only reset the status when the failed path is the session's current one.

**Nit. A held Ctrl+Shift+R skips the repeat guard.**
- `shortcuts.ts:672` dispatches `nergal:revise-plan` without `detail.repeat`.
- Only the in-flight ref protects that path.
- A repeat can land after `finally` clears the ref but before React removes the listener.
- That second submit fails the claim. It shows a false error toast and turns `submitted` into `idle`.

**Nit. The in-flight guard lives in one panel instance.**
- `decisionInFlight` is a per-mount `useRef` (`PlanPanel.tsx:65`).
- A tab switch during the write remounts the panel and resets the ref. The write can take up to 5 s.
- A second submit then fails the claim and shows a false toast. Meanwhile, the first decision lands.

**Nit. The warning misnames an `ipc_dir()` error.**
- `server.rs:1247` treats an `ipc_dir()` error the same as a foreign parent directory.
- The log then says "outside the IPC dir" (`server.rs:1252`), which misleads debugging.

### Security

**Nit. Registrations outlive their hook.**
- Nothing removes a registration when the hook times out, is killed, or gets its answer in the terminal (`adapter.rs:74-76`).
- FIFO names hold only the pid (`platform/mod.rs:333`).
- After pid reuse, a stale review in session S1 can claim `plan-<pid>.fifo`. Session S2's live hook now owns that path.
- That approves S2's plan without its review. The exposure existed before this change, and the claim does not close it.
- Fix: add a nonce to the FIFO name, or unregister when the hook exits.

### Compliance

**Important. The change contradicts an existing cc-adapter requirement.**
- `openspec/specs/cc-adapter/spec.md:84` still says plan FIFOs live at `/tmp/nergal-plan-<pid>.fifo`.
- The new ADDED requirement registers a FIFO only when its parent is the per-user IPC dir.
- After archive, the main spec holds both rules. A FIFO placed per the old rule gets rejected.
- The change should MODIFY that requirement, not only ADD one.

**Important. The no-reader scenario does not match production timing.**
- Scenario "A FIFO with no reader is an error" requires an error within 1 second.
- The helper retries ENXIO until the deadline (`plan_fifo.rs:21`).
- The command passes 5 s (`commands/plans.rs:158`).
- The test passes only because it uses a 1 s deadline (`plan_fifo_tests.rs:85`).
- In production, a FIFO left on disk without a reader gives a 5 s wait. The panel shows no progress during it. Walk 3.5 will hit this wait.

**Important. Walk 3.7 and the "edited" premise describe an affordance that does not exist.**
- No UI writes a plan's `content`. `setPlanContentAtom` (`plan.ts:65`) and `setPlanDocContentAtom` (`plan.ts:88`) have no callers.
- The scenario "A preview that arrives after its review keeps the decision path" assumes the user edited the plan.
- tasks.md 3.7 says to "edit a plan in the panel". The walk cannot be run as written.
- implementation.md cites `setPlanContentAtom` as the edit path, which repeats the same drift.

**Important. Spec conformance: one requirement clause has no test.**
- The clause: "A `PlanReview` event SHALL register its FIFO only when the FIFO's parent directory is the per-user IPC directory."
- It is implemented at `server.rs:1245-1260`.
- No test covers rejection, the empty `decision_path`, or the non-registration.
- Every other requirement and scenario has an implementing path and a unit test or a walk (3.3-3.7).

**Nit. Doc and task drift.**
- tasks.md 1.1 and 2.1 stay `[ ]`, but `handoff/.red-tests` shows the Red tests exist.
- `docs/hooks.md:32` lists the state machine as `idle → pending_review → submitted`.
- It omits the new transitions to `idle`: a delivery failure, and a preview with new content.
- implementation.md cites "tasks group 4" for the walk. The walk tasks are group 3.

Verdict: the FIFO delivery core is correct. The panel affordance, the cc-adapter spec conflict and walk 3.7 need fixes before archive.

## Frontend

Scope: `PlanPanel.tsx`, `hooks.ts` and `planReady.ts` diffs against main. No static analysis run beyond reading; walk items 3.3-3.7 stay with the user.

### Important

- `src/components/plan/PlanPanel.tsx:104-111` and `:346-357`. No visible pending state while a decision is in flight. The backend may take up to 5 s (D2 deadline). The `useRef` guard is invisible, so a second click or `A` press is ignored silently. Expectation: disable both header buttons or show a "Sending..." label (`aria-busy`) while the call runs. This needs a `useState` mirror of the ref.
- `src/components/plan/PlanPanel.tsx:104-111` and `:299-301`. After a failure the panel goes `idle`: Approve, Revise, annotate toggle and count all vanish. Nothing persistent says why; the toast is transient. Screen-reader users get no lasting cue. Expectation: show an inline muted note in the header, like the `Waiting for revision...` label, for example "Not waiting in Nergal. Answer in the terminal." The review is `idle` and coherent with the hidden buttons, but silent.
- `src/components/plan/PlanPanel.tsx:106`. Toast copy breaks the codebase convention. Existing error toasts use a short title plus a `description` with the cause (`"Push failed"` in `src/stores/shortcuts.ts:826`, `"Approve worktree request failed"` in `src/stores/worktreeGate.ts:103`). Here the 13-word sentence is the title and the raw backend error is the description. The old titles ("Approve Failed", "Revise Failed") also named the action; the new one does not. Expectation: message "Plan decision not delivered", description "Claude Code is not waiting for this plan in Nergal. Answer in the terminal." plus the error text when present.
- `src/components/plan/PlanPanel.tsx:104-111` with `:126`. A failed Revise keeps annotations (spec), but `idle` hides the annotation count and toggle. D3 says the user can copy them to the terminal; confirm in the walk that the annotations stay visible in the plan body. If they do not, the retained annotations are unreachable. Also confirm they do not leak into the next review cycle.
- `src/components/plan/PlanPanel.tsx:206`. The listener effect deps are `canAnnotate`, `annotations.length`, `backendSessionId`, `decisionPath`. `handleApprove` and `handleRevise` also close over `annotations`, `plan.path`, `path` and `sessionId`. Editing an annotation's text without changing the count sends stale feedback. Switching session with an equal length leaves `failDecision` on an old `sessionId`. This predates the diff, but `failDecision` now relies on it. Fix: depend on `annotations` and `sessionId`, or read them through a ref.

### Nit

- `src/components/plan/PlanPanel.tsx:126`. `if (!feedback) return;` exits silently, after the guard says "send". Unreachable today (button disabled, `onRevise` checks length), but it is the one path left with no feedback.
- `src/components/plan/PlanPanel.tsx:62`. The in-flight `useRef` lives per `PlanPanel` instance. A remount mid-flight (tab switch) resets it to false, so a second submit can fire. The backend claim catches it, but the toast would then show a misleading failure.
- `src/stores/hooks.ts:1`. The new import sits above the `jotai` import. Match the file's existing order (external first, then `@/` imports).
- `src/components/plan/PlanPanel.tsx:24-26`. `isRepeat` casts `Event` to `CustomEvent` without checking `instanceof`. A bare `new CustomEvent("nergal:approve-plan")` from other callers returns `undefined` for `detail`, which the optional chaining handles. Safe, but a one-line WHY comment would help future readers; the file has none.

### Checked, no issue

- `finally` reset of the ref is correct for both handlers, and the fail path returns before the ref is set, so no stuck guard.
- Hooks order: the diff adds `useRef` above the pre-existing early return at `:73`, so it does not make that problem worse.
- `e.repeat` travels through `CustomEvent.detail`. A held `A` sends one decision. If the first press fails, `canAnnotate` drops and the keydown listener detaches.
- `planReady.ts` is pure; `hooks.ts` applies it to both atoms consistently. Late previews keep the decision path and edits, and new content resets to `idle`, matching the spec scenarios.
- `docs/patterns.md` and `docs/design.md`: no rule broken. Bare-letter verbs keep the panel-focus gate and `event.code`. `design.md` §7.3 says toasts are not codified, so only the in-code convention above applies.
- Toast accessibility depends on the shared `sileo` Toaster; this diff adds nothing new there.

Verdict: no blockers; ship after addressing the in-flight feedback and persistent idle cue, or accept them as walk follow-ups.

## Orchestrator ruling

- The user chose to fix every finding (Important and Nit) before checkpoint 1. They became tasks group 3 (Revision 1 in design.md).
- Adversarial Important 2 resolved by a MODIFIED `cc-adapter` transport requirement; its stale ask-user scenario is copied verbatim (recorded drift).
- Adversarial Important 4: walk 3.7 replaced by the old-plan-tab walk; the edited-content scenario stays as a unit-level guarantee.

## Adversarial re-review (Revision 1)

Scope: `git diff main -- src src-tauri docs/hooks.md`, the six untracked files, and design Revision 1, tasks group 3 and the delta spec. Runs: `cargo test --lib plan_fifo` (16 pass), `vitest run src/stores/planReady.test.ts` (24 pass), `tsc --noEmit` (clean), `openspec validate` (valid, two length warnings).

### First-review findings

Adversarial:
- Important, controls on any plan tab: fixed. `isLiveReview` (`src/stores/planReady.ts:70-80`) now drives `canAnnotate` (`src/components/plan/PlanPanel.tsx:46-50`). The listeners and keys attach only under it (`PlanPanel.tsx:198`, `:229`). So `failDecision` fires only from the live review.
- Nit, held Ctrl+Shift+R: not fixed. See Nit 1.
- Nit, per-mount in-flight guard: fixed. `planDecisionInFlightAtom` (`src/stores/plan.ts:37`) is session-scoped and pruned on permanent close (`src/stores/sessionScope.ts:30-38`). The guard reads it synchronously through `appStore` (`PlanPanel.tsx:131`).
- Nit, misnamed `ipc_dir()` error: fixed (`src-tauri/src/hooks/server.rs:1248-1262`).
- Nit security, pid reuse: fixed by the nonce (`src-tauri/src/platform/mod.rs:333-334`). `FifoGuard` creates and unlinks the same nonce path (`src-tauri/src/hooks/cli.rs:225-230`). Registrations still outlive their hook, but a stale one never matches a live FIFO.
- Important, cc-adapter conflict: fixed by the MODIFIED block (`specs/cc-adapter/spec.md:3-23`). Its header matches `openspec/specs/cc-adapter/spec.md:82`.
- Important, no-reader timing: fixed. `ENXIO_BUDGET` caps retries at 1 s (`src-tauri/src/plan_fifo.rs:11,20`). `plan_fifo_tests.rs:146-158` covers a 5 s deadline.
- Important, walk 3.7: fixed. Walk 4.7 replaces it.
- Important, IPC-dir clause untested: partly fixed. `fifo_in_ipc_dir` has three tests (`plan_fifo_tests.rs:162-182`). The handler wiring still has none. See Nit 5.
- Nit, doc and task drift: tasks 1.1 and 2.1 are checked. The `docs/hooks.md` state machine is updated. New drift sits in Nit 5.

Frontend:
- No pending state: fixed. Both buttons render disabled with `aria-busy` (`PlanPanel.tsx:361-376`).
- No persistent cue: fixed. The header shows `Not delivered. Answer in the terminal.` (`PlanPanel.tsx:315-317`).
- Toast copy: fixed (`PlanPanel.tsx:116-125`).
- Annotations after a failed Revise: not resolved. See Important 1.
- Stale listener closures: fixed. A `latestHandlers` ref feeds the listeners (`PlanPanel.tsx:175-176`, `:204-207`); deps are `[canAnnotate]` (`:217`).
- Nit, silent empty feedback: fixed (`PlanPanel.tsx:141-144`).
- Nit, import order: not fixed (`src/stores/hooks.ts:1`).

### Regressions checked, no issue

- `undelivered` readers: `SessionIndicator.tsx:65` and `Workspace.tsx:136,284` compare only with `pending_review`. `canEnterAnnotationModeAtom` returns false for it. `stop` and `session_end` reset it to `idle` (`src/stores/hooks.ts:134,182`).
- Preview then review: the review overwrites both entries with the decision path. The live tab shows controls.
- Review then late preview: `documentBase` and the `original` match keep the path on the session and the document.
- Revise, `submitted`, new-content preview (`idle`), next review (`pending_review`): the new tab goes live; old tabs stay inert.
- Annotation mode on the live tab: `canEnterAnnotationModeAtom` and `canAnnotate` agree there, so it works.
- In-flight cannot stick: every `invoke` settles within 5 s plus the ENXIO budget, and `finally` clears it.
- ENXIO budget: `deadline.min(now + 1 s)` keeps the outer deadline in charge (`plan_fifo.rs:20`).
- Nonce: no code assumes `plan-{pid}.fifo`. The GUI death detector reads `gui.pid`, not the FIFO name (`cli.rs:278-279`). No sweeper globs FIFO names.
- IPC-dir logs: an `ipc_dir()` error and a foreign parent now log different messages.

### Important

**1. After a failed Revise, the plan body drops the highlights and X stops working.**
- `failDecision` sets `undelivered`, so `canAnnotate` flips to false (`PlanPanel.tsx:46-50,124`).
- `AnnotatableMarkdownView` recreates its highlighter on that flip (`AnnotatableMarkdownView.tsx:245-291`).
- web-highlighter 0.7.4 `dispose()` calls `removeAll()`, which strips every mark.
- The restore effect depends only on `[annotations, content]` (`:562`). It never repaints the marks.
- The `X` key and the clear listener return early without `canAnnotate` (`PlanPanel.tsx:198,229`).
- Design Revision 1 says the annotations "stay until the user clears them (`X`)". Only the drawer lists and clears them (`RightPanel.tsx:188-246`).
- Walk 4.6 ("the annotations stay visible") passes only if the drawer counts.
- Uncleared annotations carry into the next review's Revise.
- Fix: add `annotationsEnabled` to the restore effect deps. Let `X` work in `undelivered`.

**2. The spec keeps `undelivered` longer than the code does.**
- `specs/cc-adapter/spec.md:52` keeps it "until the next `plan:ready` with a decision path".
- `applyPlanReady` turns it into `idle` on a new-content preview (`planReady.ts:37-46`). The first ADDED requirement demands that.
- `stop` and `session_end` also reset it (`hooks.ts:134,182`).
- After archive, the main spec holds two rules that disagree.
- Fix: end it at "a `plan:ready` with a decision path or new content, or the end of the turn".

### Nits

1. Held Ctrl+Shift+R still skips the repeat guard. `src/stores/shortcuts.ts:672` sends no `detail.repeat`. A repeat after `finally` but before the listener detaches resends. The claim fails, and a successful Revise shows `undelivered`. It needs a write slower than the key-repeat delay.
2. Stale FIFO names. `docs/hooks.md:13` still says `plan-{pid}.fifo`. Spec examples at `spec.md:30,33,37,55` use `plan-42.fifo`, against the MODIFIED name rule.
3. "Within 1 second" is not strictly met. The budget is 1 s plus one 20 ms sleep (`plan_fifo.rs:8,29`). The tests allow 1.5 s (`plan_fifo_tests.rs:93,154`). Say "about 1 second", or set a 900 ms budget.
4. `canEnterAnnotationModeAtom` ignores `isLiveReview` (`src/stores/annotations.ts:30-32`). On an old plan's tab, Ctrl+Shift+H flips annotation mode with no visible toggle. The live tab then opens in annotate mode.
5. Drift. Task 3.1 stays `[ ]`, but its tests exist and pass. `docs/hooks.md:33` names "path outside the IPC dir" as an `undelivered` cause; that path shows no controls. Scenario `spec.md:90-93` is unreachable through the gated UI; only `decisionGuard` tests it. The handler wiring (`server.rs:1263-1268`, `:1305-1309`) has no test.

### Spec conformance

- Transport, plan decision write: `commands/plans.rs:144-163`, `adapter.rs:304-317`; adapter tests; walks 4.3, 4.4.
- Transport, ask-user write: no path. Recorded drift (design Revision 1, last bullet).
- Same pid, different FIFOs: `platform/mod.rs:333-334`; `platform/plan_fifo_path_tests.rs`.
- Late preview keeps the path: `planReady.ts:30-35,51-57`; `planReady.test.ts:29-52,110-121`.
- New-content preview: `planReady.ts:37-46`; `planReady.test.ts:54-63`.
- Old plan tab: `PlanPanel.tsx:46-50`; `isLiveReview` tests; walk 4.7.
- Preview alone: `planReady.test.ts:65-74`.
- Unregistered path, two submits: `adapter.rs:78-84`, `plans.rs:147-152`; `adapter_plan_fifo_tests.rs`.
- Missing, regular, symlink, no reader, stalled, whole payload: `plan_fifo.rs`; `plan_fifo_tests.rs:53-158`.
- Approve without a path: `planReady.ts:59-68`, `PlanPanel.tsx:127-135`; unit only (Nit 5).
- FIFO in the IPC dir: `server.rs:1246-1268,1305-1309`; predicate tests only (Nit 5).
- Held key: `PlanPanel.tsx:246`, `planReady.ts:65`; walk 4.6.
- Failed Revise keeps annotations: state kept (`PlanPanel.tsx:157`); walk 4.6, at risk from Important 1.

Verdict: the Rust fixes hold; fix the failed-Revise annotation surface and the `undelivered` spec wording before the walks.
