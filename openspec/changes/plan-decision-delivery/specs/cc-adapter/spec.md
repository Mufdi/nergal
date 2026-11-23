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
