## Context

Nergal seeds an agent session with context assembled from the user's vault pins, bound ClickUp tasks, and bound Linear issues. Today these are concatenated bare into the launch command. A coding agent cannot distinguish "reference material the user attached" from "an instruction the user typed" when both arrive as adjacent plain text. Where tracker/vault content can carry text authored outside the user's control, that is an indirect-injection seam into a fully tool-capable agent.

## Goals

- Delimit and frame every externally-sourced block as data, uniformly, by construction — so no source can be added later that bypasses the boundary.
- Keep it additive: no content filtering, no change to which sources inject, no loss of the existing length budget.
- Defense-in-depth, honestly scoped: this reduces the risk (the agent gets a clear guardrail), it does not eliminate it (the agent's own instruction-following still matters).

## Decision 1: Central boundary in `concat_context_blocks`, not per-builder

**Chosen**: apply the boundary centrally where blocks are joined (`pty.rs:780`), so every source is wrapped by construction.

- **Alternative**: each builder (`clickup/integration.rs`, `linear/integration.rs`, `obsidian/pinned_notes.rs`) wraps its own output — rejected as the primary mechanism: a future fourth source could forget the wrap. Central application is fail-safe. The builders may still label their source; the enclosing boundary is guaranteed centrally.

## Decision 2: Framing format AND delimiter-collision neutralization (hardened in iprev round 1)

**Chosen**: a labeled, fenced boundary per block — an opening marker naming the source, a one-line preamble stating the enclosed text is external reference material to treat as data (not instructions), the content, then a closing marker.

**The closing marker MUST be un-spoofable by the content**, or the whole mitigation is trivially escaped: external content containing the literal closing marker (e.g. a Linear comment that includes `</external-reference>`) would end the fence early, dumping everything after it *outside* the boundary and adjacent to the user prompt — precisely the attacker bypass this change exists to prevent. The design therefore requires ONE of:
- **(a) Neutralize the marker in content** — scan each block's content for occurrences of the closing marker (and the opening marker) and escape/rewrite them before fencing, so no content byte can terminate the fence. Simple and deterministic.
- **(b) Per-block unpredictable fence tag** — derive the boundary token per block from a value the content cannot predict (e.g. a hash of the content, appended to the marker: `<external-reference:ab12cd>…</external-reference:ab12cd>`), so content cannot contain a matching closer. Fallback if (a)'s escaping is awkward for the chosen format.

Implementation picks (a) unless a concrete reason favors (b); either is acceptable, but a bare static marker with no neutralization is NOT — it is escapable by construction.

- **Alternative**: a single preamble before all blocks — rejected: weaker separation, and shares the same collision problem.

## Decision 3: Additive framing, not content sanitization

**Chosen**: do not strip or filter the *meaning* of external content (lossy, unreliable). The ONLY rewriting permitted is the marker-neutralization of Decision 2 (escaping the fence token itself), which is mechanical and non-lossy. A clear, un-spoofable data boundary is the standard mitigation.

## Decision 4: Capability ownership (resolved)

The boundary is applied **centrally** in `concat_context_blocks` (`pty.rs:779-791`), which owns the assembly of all three blocks — verified as the single choke point, with the user prompt (`initial_prompt`) traveling in a separate `SpawnContext` field. Therefore the assembled-context requirement is owned by **`obsidian-context-injection`** (the capability governing the injected-context assembly), and this change carries a single MODIFIED delta there. The `clickup-agent-integration` / `linear-agent-integration` specs govern how each block's *content* is built, not the assembly boundary, so they are unchanged — no ambiguous multi-delta.

## Risks

- **Framing is not a hard guarantee** (LOW, accepted): a sufficiently capable injection could still influence the agent. This is defense-in-depth, consistent with how the transcript-path guard (`hooks/server.rs:678-685`) reduces rather than eliminates its risk class. Documented honestly in the proposal.
- **Prompt bloat** (LOW): boundary markers add a few tokens per block; negligible against the existing 64KB budget.

## Migration / rollout

Pure backend string-assembly change; no schema, no data migration, no flag. Legitimate context is injected exactly as before, now delimited.
