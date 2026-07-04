## Why

Content pulled from external systems is concatenated into the agent session's initial context with no marker separating it from the user's own instructions. `assemble_injected_context` / `concat_context_blocks` (`pty.rs:754-791`) join the vault, ClickUp, and Linear blocks into one string that is folded directly into the launch command (`pty.rs:634-656`). The block builders render titles, descriptions, and comments verbatim (length-budgeted, not content-framed):

- `clickup/integration.rs:52-88`
- `linear/integration.rs:85-121`
- `obsidian/pinned_notes.rs:41-58` (only a `# Pinned vault context` heading, no data/instruction framing)

Any Linear issue or ClickUp task a session is bound/pinned to can carry text authored outside the user's control (wherever intake includes external sources). That text lands in a fully tool-capable coding agent's context indistinguishable from the user's own instructions. The project already takes this class seriously elsewhere — `hooks/server.rs:678-685` rejects non-`.jsonl` transcript paths specifically to stop a crafted payload from steering the summarizer — but the context-injection path has no equivalent framing.

## What Changes

- **Wrap each external block with an explicit untrusted-data boundary.** In `concat_context_blocks`, prefix each vault/ClickUp/Linear block with a clear delimiter and instruction that the enclosed text is **reference material to treat as data, not instructions** (e.g. a fenced `<external-reference source="linear">…</external-reference>` framing plus a one-line preamble the agent reads as a guardrail). The user's own prompt stays outside these boundaries.
- **Neutralize delimiter collision (required, not optional).** External content that contains the literal closing marker would end the fence early and dump the remainder next to the user prompt — the exact bypass this defends against. The closing marker MUST be un-spoofable by content: either escape/rewrite any occurrence of the marker inside each block before fencing, or derive a per-block unpredictable fence tag (e.g. content-hash-suffixed) the content cannot match. A bare static marker with no neutralization is not acceptable (see design Decision 2).
- **Apply uniformly across all three sources** so no block is folded in bare; the obsidian pinned block gets the same treatment as the tracker blocks.
- **Keep the length budgeting unchanged** — this is additive framing (plus mechanical marker-escaping), not a content filter; truncation markers stay.

## Capabilities

### Modified Capabilities

- `obsidian-context-injection`: the assembled context requirement gains an explicit, collision-neutralized untrusted-data boundary around each externally-sourced block, so injected reference material is delimited and framed as data rather than concatenated indistinguishably from user instructions. The boundary is applied centrally in `concat_context_blocks` (the single assembly choke point), so this capability owns the requirement; `clickup-agent-integration`/`linear-agent-integration` (which govern block *content*, not assembly) are unchanged — resolved, single delta (design Decision 4).

## Impact

- **`src-tauri/src/pty.rs`**: `concat_context_blocks` (`:780`) wraps each block with the boundary framing.
- **`src-tauri/src/clickup/integration.rs`, `linear/integration.rs`, `obsidian/pinned_notes.rs`**: block builders emit their content inside the shared boundary (or the boundary is applied centrally in `concat_context_blocks` — prefer central so all sources are covered by construction).
- **Tests**: a test asserting each source's block is enclosed by the boundary markers in the assembled string; a test that an empty source contributes no bare text.
- **Risk**: LOW — additive text framing; does not change what content is injected, only how it is delimited. The mitigation is defense-in-depth (it reduces, not eliminates, indirect-injection risk — the agent's own handling still matters).
- **Out of scope**: filtering/sanitizing external content (a heavier, lossy approach); changing which sources are injected; the deep-link and fs-hardening changes (separate).
