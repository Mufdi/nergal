## Context

The ask-user dialog flow was superseded in v0.1.2 by notifier-only UX: `PreToolUse[AskUserQuestion]` → socket message → tab blink/tint (`stores/hooks.ts:98-133` via `pendingAsksAtom`/`pendingAttentionAtom`); CC's TUI renders the actual prompt. The dialog-era artifacts were parked rather than deleted, in three layers (component, store atom, backend command + adapter trait method + FIFO map), plus a module doc frozen mid-migration.

## Goals / Non-Goals

**Goals:**
- Remove every dead artifact of the dialog flow in one reviewed sweep; correct the stale module doc.
- Preserve the live attention flow untouched.

**Non-Goals:**
- Changing ask-user UX; touching plan-review FIFOs (same file, alive); removing the `ASK_USER_BLOCKING` capability flag.

## Decisions

### D1: delete rather than keep parking (pending user confirmation)

The revival option has a permanent carrying cost: a mounted-null component, a stub atom whose comment must explain itself, a registered Tauri command reachable from the webview that writes to arbitrary FIFO paths, and a trait method implying blocking ask-user is a supported adapter capability. Git history is the archive (same policy as the Widget Buddy removal, 2026-05-26). **Alternative — keep parking**: zero risk of losing the reference implementation, but the reference is already degrading (its store/backend contract no longer exists end-to-end). Rejected, subject to the user's explicit go.

### D2: keep `ASK_USER_BLOCKING`, drop the trait method

The bitflag feeds capability-gated rendering (which sessions get ask-attention UX) and adapter capability declarations (CC, Codex specs reference it). The trait *method* is the dead part — no production caller populates `pending_ask_fifos`, and the frontend command that would consume answers is itself dead. If a future agent needs true blocking ask-user, the method is re-addable with a live contract. **Alternative — remove flag too**: breaks codex/cc adapter specs and the attention gating for no cleanup gain. Rejected.

### D3: deletion order (compiler-guided)

1. Frontend leaf-first: `App.tsx` unmount → delete component file → slim `askUser.ts` (`npx tsc --noEmit` green).
2. Backend: `lib.rs:416` handler entry → `commands.rs:378` fn → CC override (`adapter.rs:300`) + `pending_ask_fifos` map/registration → trait method + default (`agents/mod.rs:643`) (`cargo check` green at each step).
3. Doc rewrite last (`adapter.rs:5-20`), describing the end state.
Before step 2's FIFO deletion, grep-verify `pending_ask_fifos` and any `register_ask_fifo`-style entry point have zero non-test callers (the hook dispatcher must not still register ask FIFOs for some path).

## Risks / Trade-offs

- [A hidden caller of `submit_ask_answer` via dynamic invoke string] → grep `"submit_ask_answer"` across `src/` (string form) — verified zero today; re-verify at implementation.
- [Deleting the CC override changes `capabilities()` behavior] → it must not: capabilities are declared flags, not derived from method presence; assert CC's declared set is unchanged in a test.
- [User later wants the dialog back] → git history + this change doc record exactly what to resurrect.

## Open Questions

- **User confirmation to delete the parked legacy modal** (memory: "kept in case we revive") — blocking gate before implementation, recorded in proposal.
