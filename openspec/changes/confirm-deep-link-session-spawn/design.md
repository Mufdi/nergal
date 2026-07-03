## Context

Custom-scheme handlers turn any clickable link into a local IPC trigger. The OS/browser "open in app?" prompt is the only current boundary, and it is both dismissible-permanently and content-blind. Nergal's deep-link surface includes a route that spawns an agent and feeds it a first instruction — the highest-authority action in the app — so it needs an in-app, content-aware confirmation.

## Goals

- Close the 1-click auto-spawn-and-prompt vector without degrading legitimate deep-link UX (the common case is the user's own tooling firing a link they expect).
- Show the user *what* they are authorizing: which directory, what prompt, git-repo or not.
- Avoid confirmation fatigue — gate only the routes that reach an agent.

## Decision 1: Gate at the router (frontend), not the PTY (backend)

**Chosen**: the confirmation lives in `deepLinkRouter.ts`, before `create_session`.

- **Alternative**: block in `queue_session_prompt`/spawn — rejected: the backend has no UI affordance for a rich confirm, and the same command is used by legitimate in-app flows that should not prompt. The router is the one place that knows the request came from an *external* link.
- **Trade-off**: a future non-router caller of the spawn commands would not inherit the gate — acceptable, because in-app callers are already user-initiated in context.

## Decision 2: Which routes are gated (corrected in iprev round 1)

**Chosen**: gate every route that creates a workspace/session for an unknown path — `handleSessionNew` (spawn + prompt), `handleOpenFile`'s workspace/session-creation branch, and `handleOpenWorkspace`'s workspace-creation branch. Do **not** gate focusing an already-known session.

- **Correction**: the first draft excluded `open-file` on the premise it "neither spawns an agent nor injects a prompt." That premise was **false** — `handleOpenFile` (`deepLinkRouter.ts:230-292`) calls `create_workspace` for an unknown path (`:240`), `create_session` (`:266`), and activates it; its own comment (`:285`) says "Activating the session spawns its PTY (empty session starts the agent)." So `open-file` is a spawn vector and must be gated. The gate attaches to the workspace/session-creation branch, not to opening a tab in an already-known workspace.
- **Rationale**: the threat is an attacker instruction reaching an agent OR an arbitrary directory being registered and an agent spawned there. Both `session/new` and `open-file` do that for unknown paths.
- **Alternative**: gate every deep-link action including known-session focus — rejected: trains users to click through, weakening the gate that matters.

## Decision 3: Non-git target — warn via a PRE-creation probe (corrected)

**Chosen**: a pre-creation probe command (reuse `resolve_repo_root`, or a lightweight `probe_workspace_path`) tells the router whether the path is a git repo *before* `create_workspace` runs; the gate warns when it is not. It does not hard-block non-git directories.

- **Correction**: `create_workspace` already returns `is_git` (`commands.rs:773`), but that is only known *after* the workspace is created — useless for a gate that must warn first and create nothing on decline. The probe must be a separate, side-effect-free call.
- **Rationale**: opening a non-git directory is a legitimate manual action; a hard block would break real use. The warning gives the user the signal that matters for the *deep-link* case (an agent about to run somewhere unexpected like `$HOME`).

## Decision 5: The confirm's default action must not be "proceed" (iprev round 1)

`ConfirmHost` steals focus on open and confirms on Enter in the capture phase (`ConfirmHost.tsx:36-43,65-69`). For an externally-triggered security dialog, a stray Enter from the user typing in the terminal could confirm it — attacker-controllable timing.

**Chosen**: the deep-link confirm variant does not make "proceed" the Enter default — Enter cancels or is inert, and proceeding requires an explicit pointer/keyed action on the proceed control.

- **Alternative**: reuse the default `ConfirmHost` as-is — rejected: its proceed-on-Enter default is unsafe for an externally-triggered dialog. This is a required property, not deferred polish.

## Decision 4: Prompt rendering safety

The prompt text is attacker-controlled and rendered in the confirm dialog, whose body is a `dangerouslySetInnerHTML` sink by contract (`ConfirmHost.tsx:75-80`). It MUST be HTML-escaped via the shared `escapeHtml` helper (the sibling Band-B change extracts this from `linear.ts`/`clickup.ts` into a shared util). If that change has not landed, inline-escape here and refactor later.

## Risks

- **Confirmation fatigue / user clicks through** (MED): mitigated by gating only agent-reaching routes and by showing concrete cwd+prompt so the dialog carries real signal.
- **A legitimate automation that fired links silently now needs a click** (LOW): acceptable and correct — that automation was, by construction, the same capability an attacker would abuse.

## Migration / rollout

Frontend-first; the backend `is_git_repo` signal is additive. No schema, no data migration, no flag — the gate is strictly safer and legitimate confirmed links behave as before.
