# cross-session-messaging

Consumption is recorded only after the wake's deferred submit is confirmed.

## MODIFIED Requirements

### Requirement: Hybrid state-aware delivery

The system SHALL wake a target according to its mode, through a `SessionDelivery` abstraction. An idle target SHALL receive a PTY stdin wake note (via the existing PTY writer, only the owning agent PTY, with every embedded relayed string sanitized) that **embeds the pending message bodies directly** — the wake IS the read, so no separate `read_messages` round-trip is needed; once the wake lands the system SHALL mark those messages `agent_consumed_at` (delivery == consume for the wake path). A wake SHALL count as landed only when its deferred submit `\r` has been delivered to the target PTY — paste success alone SHALL NOT trigger consumption; the mark-consumed write and the `crossmsg:agent-consumed` emission SHALL run in the wake's completion path after a confirmed submit. While a wake's submit is pending, the target session SHALL be marked in-flight and further drains for it SHALL be skipped (no double paste of still-unconsumed bodies). `read_messages` remains as a catch-up / full-history fallback. A working target's delivery SHALL be queued; on the target's next `Stop`, the `nergal hook stop` CLI command SHALL query for pending deliveries and emit `hookSpecificOutput.additionalContext` on its stdout (the hook socket is fire-and-forget and cannot return data). Delivery SHALL key off `agent_consumed_at` (set by the wake path or `read_messages`), never `human_seen_at`. If the wake fails to land — paste failure **or** submit failure — the messages SHALL be left unconsumed so the next idle flip retries — never stranded.

Every working→idle transition with a non-empty pending queue SHALL trigger a PTY wake, and a send to an **already-settled** idle target SHALL wake it immediately — the `additionalContext` path is a best-effort fast layer, never the sole delivery path, so a message sent just after a `Stop` is never stranded.

**Wake submit timing (walk hardening).** A wake note SHALL be pasted to the PTY WITHOUT the `\r` submit, and the submit SHALL be sent as a separate lone `\r` after a short settle, so the Enter does not travel in the same write burst as the bracketed-paste end marker and race the TUI's exit from paste mode (which would leave the note as an unsubmitted draft). At most ONE PTY paste SHALL occur per idle transition: when a cross-session wake pastes on a `Stop`, any other queued PTY delivery for that session (e.g. an agent-spawned-worktree outcome) SHALL defer to the next idle rather than paste back-to-back (the second paste would race the first's submit).

**Just-idled debounce.** A target that idled only moments ago (within a short settle window) MAY still be returning its TUI to the prompt; the send-path immediate wake SHALL queue such a target rather than paste into it, relying on its imminent next `Stop` to drain it. A *settled* idle target — or one whose runtime mode is unknown (the side-map is volatile and empty after a restart, and a quiet idle session never emits the `Stop` the drain needs) — SHALL still be woken immediately so it is never stranded.

#### Scenario: Deliver to a settled idle target

- **WHEN** a message is recorded for a target whose mode is idle and that has been idle past the settle window (or whose mode is unknown)
- **THEN** the system SHALL inject a sanitized wake note embedding the message bodies into the target's PTY stdin (pasted without the Enter, then submitted with a settled `\r`), and mark them `agent_consumed_at` only after the submit is confirmed

#### Scenario: Submit failure leaves messages pending

- **GIVEN** a wake note was pasted and the target PTY closes before the deferred `\r` submit lands
- **WHEN** the submit fails
- **THEN** `agent_consumed_at` SHALL remain unset and the next working→idle transition SHALL retry with a fresh wake note

#### Scenario: In-flight wake is not double-pasted

- **GIVEN** a wake's submit is pending for a session
- **WHEN** another drain (send-path immediate wake or a concurrent `Stop`) targets the same session in that window
- **THEN** it SHALL skip the session (no second paste of the same still-unconsumed bodies)

#### Scenario: Just-idled target is queued, not pasted

- **WHEN** a message is recorded for a target that idled within the settle window
- **THEN** the delivery SHALL be queued (status `queued`) and the message SHALL remain pending until the target's next `Stop` drains it, never pasted into a not-yet-ready prompt

#### Scenario: At most one paste per idle transition

- **WHEN** a session reaches `Stop` with both a pending cross-session message AND a pending agent-spawned-worktree outcome
- **THEN** the cross-session wake SHALL paste and the worktree-outcome delivery SHALL defer to the next idle, so two bracketed pastes never race each other's submit

#### Scenario: Deliver to a working target via Stop CLI stdout

- **WHEN** a message is recorded for a working target and that target next emits a `Stop`
- **THEN** the `nergal hook stop` CLI SHALL emit `hookSpecificOutput.additionalContext` on stdout notifying it of pending messages, without a hook error

#### Scenario: Message sent just after Stop is not stranded

- **WHEN** a message is queued for a target whose mode reads working but whose `Stop` already fired (now effectively idle)
- **THEN** the next working→idle transition (or immediate idle detection) SHALL PTY-wake the target so the message is delivered, regardless of agent `additionalContext` support

#### Scenario: UI viewing does not cancel delivery

- **WHEN** the user opens the thread in the UI (setting `human_seen_at`) before the agent has consumed the message
- **THEN** the pending delivery SHALL remain active (delivery keys off `agent_consumed_at`, which the UI never sets)
