# Design notes — plan-panel-multi-agent

Created post-hoc to capture mid-implementation revisions per
`openspec/config.yaml::rules.apply` ("Mid-implementation revision: …
Agregar sección '## Revision N: <título>' en design.md …").

## Revision 1: OpenCode plan_capability flipped from FileBased to NotApplicable

**Date**: 2026-05-24

**What changed**
- Proposal / spec deltas / tasks previously declared the OpenCode adapter
  as `FileBased { dir: cwd.join(".opencode/plans"), label: "OpenCode" }`.
- Revised to `NotApplicable` across `proposal.md`, the spec delta, and
  `tasks.md`. The adapter source comment now references the upstream
  source code and the issue thread that motivated the flip.

**Why**
- The original `FileBased` claim was an unverified assumption inherited
  from the spec. User testing surfaced that OpenCode in plan mode does
  not auto-create or auto-save `.opencode/plans/*.md`.
- Validation against the official OpenCode source code
  (`anomalyco/opencode` branch `dev`,
  `packages/opencode/src/agent/agent.ts`) confirms plan mode is read-only
  with only two narrow `edit` permission exceptions:
  - `<cwd>/.opencode/plans/*.md`
  - `<Global.Path.data>/plans/*.md`
  These are permission gates, not auto-save behaviour. Whether the agent
  actually writes to either path depends on the model deciding to use a
  bash heredoc, and upstream issue
  [anomalyco/opencode#11078](https://github.com/anomalyco/opencode/issues/11078)
  documents the model routinely refusing even with permission granted.
- The OpenCode TUI docs (`opencode.ai/docs/tui/`) and modes docs make no
  mention of `.opencode/plans/` nor of automatic persistence.

**Net effect on user experience**
- The Plans entry in the right-panel chrome is hidden for OpenCode
  sessions (same handling as Codex / Pi).
- If upstream OpenCode ships reliable automatic plan persistence in the
  future, the adapter can be flipped back to `FileBased` and the empty
  state can be refined; until then `NotApplicable` is the honest
  contract.

**Evidence trail**
- Source: <https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/agent/agent.ts>
- Issue: <https://github.com/anomalyco/opencode/issues/11078>
- Docs (silent on the topic): <https://opencode.ai/docs/tui/>, <https://opencode.ai/docs/agents/>
