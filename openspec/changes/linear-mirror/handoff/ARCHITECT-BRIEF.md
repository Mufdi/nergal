# Architect Brief — linear-mirror

**Mission.** Nergal is a Linux desktop wrapper around the Claude Code CLI; it runs *around* the agent↔human loop and organizes the ecosystem surrounding it. Bringing trackers (ClickUp, now Linear) inside is squarely in scope: it removes a context-switch from the loop without reimplementing the agent.

**This change.** First of three sequential Linear changes mirroring the archived ClickUp staging. Builds the foundation: Personal-API-key auth (OAuth-extensible seam), a GraphQL client, a SQLite mirror of teams/states/labels/issues, a bounded poller, and a read-only keyboard-first panel. Delivers value alone.

**Control metadata** (`.work-modules.json`): tier XL, ceremony deep, risk_tier critical, files ~22, tags [migration, security, feature], visibility private, spec_target `linear-mirror` + `linear-task-panel`.

**Confirmed decisions (AskUserQuestion 2026-06-16):**
- Staging: 3 sequential changes; this is #1.
- Auth: Personal API key now; OAuth deferred, `AuthMode` seam left.
- Approach: spec-first with a checkpoint, then implement in `/loop`.

**Hard divergences from the ClickUp precedent** (why this is adaptation, not copy):
1. GraphQL (single endpoint, cursor pagination, nested-relation queries) vs REST fan-out.
2. Fixed schema — no custom fields, no checklists; first-class labels + native state `type`.
3. Bounded poll scope (updated-window ∪ viewer-assigned) vs ClickUp's poll-all — Linear scale forces it. **The one runtime decision to validate on the walk.**
4. Rate limit = HTTP 400 + GraphQL `RATELIMITED` + `X-RateLimit-*-Reset`, no `Retry-After`.

**Reused codebase assets** (anchored in implementation.md): `keyring` dep, `MarkdownView` sanitizing pipeline, `StatusIcon`/`PriorityIcon` glyphs, dual-shell detail/tab pattern, `notify-send` helper, the `BACKEND_OWNED_CONFIG_KEYS` discipline, the rightPanel `TabType` registry.

**Dependencies / blockers.** None — independent of ClickUp and the context-bridge MCP changes. Runs in parallel.

**Gating.** iterative-plan-review (Claude evaluator) pre-build per A3 (critical + migration + security). Security escalation planned for the build phase (auth + untrusted-content rendering + same-uid boundary).
