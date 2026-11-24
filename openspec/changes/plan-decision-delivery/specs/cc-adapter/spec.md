## MODIFIED Requirements

### Requirement: CC adapter uses Unix socket + FIFO transports for events and decisions

The CC adapter SHALL use the per-user Unix socket `hook.sock` in the IPC directory to receive hook events from the `nergal hook` CLI subcommands. For a blocking plan review, the hook SHALL create a FIFO named `plan-<pid>-<nonce>.fifo` in the per-user IPC directory, where `<nonce>` is a random UUID, so a reused pid never yields the path of an earlier review.

#### Scenario: Plan review FIFO write on user decision

- **WHEN** the user accepts or denies a plan in the UI for a CC session
- **AND** the frontend invokes `submit_plan_decision`
- **THEN** the CC adapter SHALL write the decision JSON to the FIFO referenced by `decision_path`
- **AND** the blocked `nergal hook plan-review` subprocess SHALL receive the decision and pass it back to CC via the hook protocol

#### Scenario: Ask-user FIFO write on user answer

- **WHEN** the user submits an AskUserQuestion answer for a CC session
- **THEN** the CC adapter SHALL write `{"answers": <map>}` to the FIFO referenced by `decision_path`

#### Scenario: Two reviews with the same pid get different FIFOs

- **WHEN** the plan-review FIFO path is built twice for pid 42
- **THEN** both paths SHALL sit in the per-user IPC directory, start with `plan-42-`, and differ

## ADDED Requirements

### Requirement: A plan review opens only with a decision path
A plan review SHALL open in the UI only from a `plan:ready` event that carries a non-empty `decision_path`. The plan panel SHALL show Approve and Revise only on the document of the live review: the review state is `pending_review` and the document's decision path is non-empty and equal to the session's. A `plan:ready` event without one whose content equals the content the session last received (`original`) SHALL keep the stored decision path, session id, review state, and any edit the user made. A `plan:ready` event without one and with different content SHALL replace the plan, clear the stored decision path and set the review state to `idle`.

#### Scenario: A preview that arrives after its review keeps the decision path
- **GIVEN** session S received `plan:ready` with `path: "/p/a.md"`, `content: "# A"` and `decision_path: "/run/user/1000/nergal/plan-42-u.fifo"`, and its review state is `pending_review`
- **AND** the user edited the plan content to `# A edited`
- **WHEN** `plan:ready` arrives for S with `content: "# A"` and no `decision_path`, with `path: "/p/a.md"` or with `path: "/p/b.md"`
- **THEN** the session plan and the document for the preview's path SHALL hold decision path `/run/user/1000/nergal/plan-42-u.fifo` and content `# A edited`
- **AND** the review state SHALL stay `pending_review`

#### Scenario: A preview with new content starts a new cycle
- **GIVEN** session S last received plan content `# A`, holds decision path `/run/user/1000/nergal/plan-42-u.fifo` and review state `pending_review`
- **WHEN** `plan:ready` arrives for S with `content: "# B"` and no `decision_path`
- **THEN** the plan content SHALL be `# B`, the stored decision path SHALL be empty, and the review state SHALL be `idle`

#### Scenario: An old plan's tab shows no review controls
- **GIVEN** session S has review state `pending_review` with decision path `/run/user/1000/nergal/plan-42-u.fifo` on `/p/a.md`
- **WHEN** the user opens the tab of an earlier plan `/p/old.md` whose stored decision path is empty or different
- **THEN** that tab SHALL show no Approve and no Revise

#### Scenario: A preview alone opens no review
- **GIVEN** session S has review state `idle` and no stored plan
- **WHEN** `plan:ready` arrives for S without `decision_path`
- **THEN** the plan content SHALL show and the review state SHALL stay `idle`

### Requirement: A plan decision reaches the waiting hook or fails visibly
On Unix, `submit_plan_decision` SHALL claim the session's registration before writing: it SHALL proceed only when the decision path equals the path registered by the latest `PlanReview` event, and the claim SHALL remove that registration, so at most one decision is written per review. It SHALL write only to an existing FIFO that has a reader, and SHALL never create a file, follow a symlink, or take more than 5 seconds. A FIFO with no reader SHALL fail within 1 second, whatever the write deadline. A `PlanReview` event SHALL register its FIFO only when the FIFO's parent directory is the per-user IPC directory. When the UI cannot deliver a decision, it SHALL show an error toast titled with the action (`Approve failed` or `Revise failed`) whose description tells the user to answer in the terminal, and the review state SHALL become `undelivered`. That state hides Approve and Revise, keeps the annotations visible and clearable, and shows `Not delivered. Answer in the terminal.` in the panel header. It lasts until a `plan:ready` opens a review or brings new content, or the session stops. While a decision for a session is being sent, its Approve and Revise SHALL be disabled. A failed Revise SHALL keep the annotations.

#### Scenario: An unregistered path is rejected before any open
- **GIVEN** session S has registered `/run/user/1000/nergal/plan-42-u.fifo`
- **WHEN** `submit_plan_decision` targets `/tmp/other.fifo` for S
- **THEN** it SHALL return an error, open no file, and keep the registration

#### Scenario: Two submits for one review write one decision
- **GIVEN** session S has registered a live FIFO
- **WHEN** two `submit_plan_decision` calls for S and that path run at the same time
- **THEN** exactly one SHALL write its decision
- **AND** the other SHALL return an error and write nothing

#### Scenario: A missing FIFO is an error and creates nothing
- **WHEN** the decision writer targets a path that does not exist
- **THEN** it SHALL return an error
- **AND** no file SHALL exist at that path afterwards

#### Scenario: A regular file or a symlink is not a FIFO
- **GIVEN** a regular file with content `x`, or a symlink to a FIFO, at the decision path
- **WHEN** the decision writer targets it
- **THEN** it SHALL return an error and the file content SHALL stay `x`

#### Scenario: A FIFO with no reader is an error
- **GIVEN** a FIFO at the decision path that no process has open for reading
- **WHEN** the decision writer targets it
- **THEN** it SHALL return an error within 1 second, with a 1-second or a 5-second deadline

#### Scenario: A reader that never reads hits the deadline
- **GIVEN** a FIFO with a reader open that never reads
- **WHEN** the decision writer writes 100000 bytes with a 200 ms deadline
- **THEN** it SHALL return an error within 1 second

#### Scenario: A live FIFO receives the whole decision
- **GIVEN** a FIFO at the decision path with a reader already open
- **WHEN** the decision writer writes a JSON value 100000 bytes long
- **THEN** the reader SHALL receive all 100000 bytes and parse them as one JSON value

#### Scenario: A review without a decision path shows no controls
- **GIVEN** session S has review state `pending_review` and the open document's stored decision path is empty
- **WHEN** the plan panel renders that document
- **THEN** it SHALL show no Approve and no Revise

#### Scenario: The FIFO must sit in the IPC directory
- **WHEN** a `PlanReview` event names `/tmp/plan-42-u.fifo` while the IPC directory is `/run/user/1000/nergal`
- **THEN** the FIFO SHALL not be registered and `plan:ready` SHALL carry an empty `decision_path`

#### Scenario: A held key sends one decision
- **GIVEN** the plan panel shows a review with focus in the panel
- **WHEN** the user holds `A` so the key repeats
- **THEN** the panel SHALL call `submit_plan_decision` once
- **AND** a repeat of `A`, `R` or `Ctrl+Shift+R` that arrives after the review left `pending_review` SHALL send nothing

#### Scenario: A failed Revise keeps the annotations
- **GIVEN** a review with two annotations whose delivery fails
- **WHEN** the user presses `R`
- **THEN** a `Revise failed` toast SHALL show, the review state SHALL become `undelivered`, and both annotations SHALL stay highlighted in the plan
- **AND** pressing `X` and confirming SHALL clear them

### Requirement: An approval carries the tool input
The `nergal hook plan-review` subprocess SHALL answer an approval with a `PermissionRequest` decision whose `behavior` is `allow` and whose `updatedInput` equals the `tool_input` object it read from stdin, because Claude Code ignores an `allow` without `updatedInput` for `ExitPlanMode`. A denial SHALL keep `behavior: "deny"` with the message. Without `NERGAL_SESSION_ID` the subprocess SHALL print nothing and exit 0, so Claude Code shows its own dialog. A released review SHALL print nothing and exit 0.

#### Scenario: Approve echoes the plan input
- **GIVEN** the hook read `{"tool_name":"ExitPlanMode","tool_input":{"plan":"# A","planFilePath":"/p/a.md"}}` from stdin
- **WHEN** it receives `{"approved": true}`
- **THEN** stdout SHALL be one JSON object with `hookSpecificOutput.hookEventName` `PermissionRequest`, `decision.behavior` `allow`, and `decision.updatedInput` equal to `{"plan":"# A","planFilePath":"/p/a.md"}`

#### Scenario: Revise keeps the deny message
- **WHEN** the hook receives `{"approved": false, "message": "fix step 2"}`
- **THEN** stdout SHALL hold `decision.behavior` `deny` and `decision.message` `fix step 2`, and no `updatedInput`

#### Scenario: No Nergal session prints no decision
- **GIVEN** `NERGAL_SESSION_ID` is unset or empty
- **WHEN** the hook runs
- **THEN** stdout SHALL be empty and the exit code SHALL be 0

#### Scenario: A released review prints no decision
- **WHEN** the hook receives `{"released": true}`
- **THEN** stdout SHALL be empty and the exit code SHALL be 0

### Requirement: A review resolved outside the panel releases its hook
Nergal SHALL register its `PostToolUse` hook for `ExitPlanMode` in addition to the tools it already matches. It SHALL release a session's plan review on a `PostToolUse` for `ExitPlanMode` from the main agent, on a `PlanReview` for a session that still has a registered FIFO (before registering the new one), on `Stop`, and on `SessionEnd`. Releasing SHALL remove the registration and, when one existed, write `{"released": true}` to its FIFO with a deadline of at most 1 second, off the hook server's event loop; a failed write SHALL only be logged. On a `PostToolUse` for `ExitPlanMode` from the main agent, Nergal SHALL emit `plan:review-closed` with the session id. A deny or Esc answered in Claude Code's own dialog emits no event Nergal registers, so such a review stays registered until one of these events.

#### Scenario: A terminal approval releases the hook and closes the review
- **GIVEN** session S has registered FIFO F with a reader
- **WHEN** a `PostToolUse` event for `ExitPlanMode` without `agent_id` arrives for S
- **THEN** the registration for S SHALL be gone, the reader of F SHALL receive `{"released": true}`, and `plan:review-closed` SHALL be emitted for S

#### Scenario: A new review releases the old hook
- **GIVEN** session S has registered FIFO F1 with a reader
- **WHEN** a `PlanReview` event for S arrives with FIFO F2 in the IPC directory
- **THEN** the reader of F1 SHALL receive `{"released": true}` and S SHALL have F2 registered

#### Scenario: Stop releases a waiting hook
- **GIVEN** session S has registered FIFO F with a reader
- **WHEN** a `Stop` event for S arrives
- **THEN** the reader of F SHALL receive `{"released": true}` and S SHALL have no registration

#### Scenario: A subagent's PostToolUse releases nothing
- **GIVEN** session S has registered FIFO F
- **WHEN** a `PostToolUse` event for `ExitPlanMode` with an `agent_id` arrives for S
- **THEN** F SHALL stay registered and no `plan:review-closed` SHALL be emitted

#### Scenario: Setup adds ExitPlanMode to the PostToolUse matcher
- **GIVEN** settings hold the Nergal `PostToolUse` entry with matcher `Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create|AskUserQuestion`
- **WHEN** setup runs
- **THEN** that entry's matcher SHALL also include `ExitPlanMode` and no second Nergal `PostToolUse` entry SHALL exist

### Requirement: The panel shows a review answered in the terminal
On `plan:review-closed`, the panel SHALL move a review in `pending_review` or `undelivered` to state `answered`, and a review in `submitted` to `answered` with an error toast titled `Revise not delivered`. `answered` hides Approve and Revise, keeps the annotations visible and clearable, and shows `Answered in the terminal.` in the header, plus `Annotations were not sent.` when annotations exist. It SHALL last until a `plan:ready` opens a review or brings new content, or the session ends; `Stop` SHALL keep it.

#### Scenario: The panel shows the terminal answer and keeps annotations
- **GIVEN** session S has review state `pending_review` and two annotations
- **WHEN** `plan:review-closed` arrives for S
- **THEN** the review state SHALL be `answered`, both annotations SHALL stay, Approve and Revise SHALL be hidden, and the header SHALL read `Answered in the terminal. Annotations were not sent.`

#### Scenario: A decision after a terminal answer fails visibly
- **GIVEN** session S was released by a `PostToolUse` for `ExitPlanMode`
- **WHEN** the panel calls `submit_plan_decision` for S with the old decision path
- **THEN** it SHALL return an error and the panel SHALL show `Approve failed` or `Revise failed`

#### Scenario: A revise that lost the race reports it
- **GIVEN** session S has review state `submitted` after a Revise
- **WHEN** `plan:review-closed` arrives for S
- **THEN** a `Revise not delivered` error toast SHALL show and the review state SHALL be `answered`

#### Scenario: Stop keeps the answered state
- **GIVEN** session S has review state `answered`
- **WHEN** a `Stop` event for S arrives
- **THEN** the review state SHALL stay `answered`
