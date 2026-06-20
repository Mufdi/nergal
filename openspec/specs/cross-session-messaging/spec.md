# cross-session-messaging Specification

## Purpose
TBD - created by archiving change cross-session-messaging. Update Purpose after archive.
## Requirements
### Requirement: send_to_session tool

The system SHALL expose a `send_to_session(to_session_id, message, thread_id?)` MCP tool that records a message for the target and triggers delivery. The target MUST be an active session (live agent). The tool SHALL return the thread id (created or joined) and a delivery status (`delivered` or `queued`).

#### Scenario: Send to an active session

- **WHEN** an agent calls `send_to_session` targeting an active session
- **THEN** the message SHALL be recorded in the store and delivery SHALL be triggered, and the tool SHALL return the thread id and delivery status

#### Scenario: Send to an inactive session is refused

- **WHEN** an agent calls `send_to_session` targeting an inactive (closed) session
- **THEN** the tool SHALL return a structured error indicating the session is inactive and pointing to the worktree-spawn path to revive/create it
- **AND** no process SHALL be spawned as a side effect

### Requirement: read_messages tool

The system SHALL expose a `read_messages(thread_id?)` MCP tool returning the messages addressed to the caller it has not yet consumed, setting `agent_consumed_at` on return (take-on-read) for exactly the messages returned. `agent_consumed_at` is distinct from `human_seen_at` (UI). With a `thread_id`, consumption is scoped to that thread (so an agent can drain one conversation without bulk-marking others); without it, all undelivered messages across the caller's threads are returned and consumed.

#### Scenario: Read consumes messages

- **WHEN** an agent calls `read_messages` and has unconsumed messages
- **THEN** the tool SHALL return them and set `agent_consumed_at` so a subsequent call does not return the same messages

#### Scenario: Minimal in-context payload

- **WHEN** `read_messages` returns messages
- **THEN** it SHALL return only the messages needed to act on (not the full thread history), keeping the agent's context minimal

### Requirement: Threads with per-message hop cap, dedup, and count/time budget

A thread SHALL be `{ id, originator_session, participants, status, max_hops, msg_count, msg_budget, deadline_at }`; each message SHALL carry its own `depth` representing **reach**. `depth = sender_message_depth + (target is a new thread participant ? 1 : 0)` — pulling in a new participant increments reach; a reply between existing participants does not. The router SHALL bound **reach** by `max_hops` and **conversation length** separately by `msg_budget` (a message-count cap) plus a wall-clock `deadline_at` (NOT a token budget — cluihud cannot measure tokens spent inside agent turns). It SHALL deduplicate via `hash(from, to, normalize(body))` (`normalize` = trim + collapse whitespace). The originator SHALL NOT block; replies arrive asynchronously tagged with the thread id.

#### Scenario: Two-party dialogue is not capped by reach

- **WHEN** A and B exchange many messages back and forth in thread T (no new participants)
- **THEN** reach SHALL stay constant and the exchange SHALL NOT hit `hop_limit_reached`; only `msg_budget`/`deadline_at` bound its length

#### Scenario: Transitive reach within the cap

- **WHEN** A messages B, then B messages a new participant C in thread T at a reach within `max_hops`
- **THEN** the relay SHALL proceed and C SHALL receive the message tagged with thread T

#### Scenario: Reach cap exceeded

- **WHEN** a `send_to_session` to a new participant would make reach exceed `max_hops`
- **THEN** the tool SHALL return a structured "hop limit reached" error and SHALL NOT deliver

#### Scenario: Fan-out branches are independent

- **WHEN** B sends to two new participants C and D in the same thread
- **THEN** each branch's reach SHALL derive from B's message depth, not from a shared thread counter

#### Scenario: Duplicate is suppressed with a distinct status

- **WHEN** a `send_to_session` repeats an already-seen `dedup_key` within the thread
- **THEN** the tool SHALL return the distinct status `duplicate_suppressed` (NOT a delivered/queued shape) so the caller does not await a phantom reply

#### Scenario: Budget exhausted

- **WHEN** a thread's message-count cap is reached
- **THEN** the thread SHALL be closed, its participants SHALL be notified through `SessionDelivery` (respecting the kill-switch and idle/working guards), and further `send_to_session` on it SHALL be refused

#### Scenario: Deadline closes an idle thread via active sweep

- **WHEN** a thread reaches its `deadline_at` while no further `send_to_session` occurs
- **THEN** an active daemon sweep SHALL close the thread and notify the user (the deadline SHALL NOT be evaluated only lazily on the next send)

### Requirement: Hybrid state-aware delivery

The system SHALL wake a target according to its mode, through a `SessionDelivery` abstraction. An idle target SHALL receive a PTY stdin wake note (via the existing PTY writer, only the owning agent PTY, with every embedded relayed string sanitized) that **embeds the pending message bodies directly** — the wake IS the read, so no separate `read_messages` round-trip is needed; once the wake lands the system SHALL mark those messages `agent_consumed_at` (delivery == consume for the wake path). `read_messages` remains as a catch-up / full-history fallback. A working target's delivery SHALL be queued; on the target's next `Stop`, the `cluihud hook stop` CLI command SHALL query for pending deliveries and emit `hookSpecificOutput.additionalContext` on its stdout (the hook socket is fire-and-forget and cannot return data). Delivery SHALL key off `agent_consumed_at` (set by the wake path or `read_messages`), never `human_seen_at`. If the wake fails to land the messages SHALL be left unconsumed so the next idle flip retries — never stranded.

Every working→idle transition with a non-empty pending queue SHALL trigger a PTY wake for ALL agents, and a send to an already-idle target SHALL wake it immediately — the `additionalContext` path is a best-effort fast layer, never the sole delivery path, so a message sent just after a `Stop` is never stranded.

#### Scenario: Deliver to an idle target

- **WHEN** a message is recorded for a target whose mode is idle
- **THEN** the system SHALL inject a sanitized wake note embedding the message bodies into the target's PTY stdin, then mark them `agent_consumed_at` (no separate `read_messages` round-trip required)

#### Scenario: Deliver to a working target via Stop CLI stdout

- **WHEN** a message is recorded for a working target and that target next emits a `Stop`
- **THEN** the `cluihud hook stop` CLI SHALL emit `hookSpecificOutput.additionalContext` on stdout notifying it of pending messages, without a hook error

#### Scenario: Message sent just after Stop is not stranded

- **WHEN** a message is queued for a target whose mode reads working but whose `Stop` already fired (now effectively idle)
- **THEN** the next working→idle transition (or immediate idle detection) SHALL PTY-wake the target so the message is delivered, regardless of agent `additionalContext` support

#### Scenario: UI viewing does not cancel delivery

- **WHEN** the user opens the thread in the UI (setting `human_seen_at`) before the agent has consumed the message
- **THEN** the pending delivery SHALL remain active (delivery keys off `agent_consumed_at`, which the UI never sets)

### Requirement: Relayed context is non-authoritative (labeling + documented limits only)

Cross-session relayed context SHALL be treated as carrying no user authority, scoped to what cluihud can actually enforce. cluihud CANNOT attribute a downstream autonomous action to a relayed message (the agent's reasoning is unobservable), so a provenance flag on cluihud's gates is NOT used. The enforceable controls are: (1) injected wake/`additionalContext` SHALL be labeled as relayed and advisory, naming the origin session; (2) the system SHALL document that cluihud's own gates (plan-review FIFO, the `agent-spawned-worktrees` human gate) are human-decided by construction (a relayed message cannot auto-satisfy them), and that cluihud cannot override the target agent's own `--permission-mode`.

#### Scenario: Injected context is framed as non-authoritative

- **WHEN** a wake prompt or `additionalContext` is injected for a relayed message
- **THEN** its text SHALL identify the originating session and mark the content as relayed/advisory, not an instruction carrying user authority

#### Scenario: No provenance flag is claimed

- **WHEN** a session acts on a relayed message and later reaches a cluihud gate
- **THEN** the system SHALL rely on the gate's existing human decision (not a provenance flag), since the action cannot be attributed to the relayed message

#### Scenario: Agent permission-mode limitation is documented

- **WHEN** the target agent runs under a bypass/auto `--permission-mode`
- **THEN** the design SHALL document that cluihud cannot override that posture, rather than implying an enforcement it does not have

### Requirement: search_sessions tool (read-only over active and inactive)

The system SHALL expose a `search_sessions(query)` MCP tool that searches across both active and inactive sessions by name, summary, and transcript content using the existing `search/mod.rs` engine with its `transcripts_dir` scope (ripgrep-backed; performance is ripgrep-bounded, not indexed). It SHALL return read-only descriptors, marking inactive sessions as not messageable.

#### Scenario: Find an inactive session read-only

- **WHEN** an agent calls `search_sessions` with a query matching an inactive session's transcript or summary
- **THEN** the tool SHALL return that session marked as inactive/read-only

### Requirement: list_threads tool

The system SHALL expose a `list_threads` MCP tool returning the threads the caller participates in, with their status and participants.

#### Scenario: Enumerate the caller's threads

- **WHEN** an agent calls `list_threads`
- **THEN** the tool SHALL return the threads the caller participates in, with their status and participants

