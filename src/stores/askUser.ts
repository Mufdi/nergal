import { sessionScopedMapAtom } from "./sessionScope";

export const pendingAsksAtom = sessionScopedMapAtom<true>();

// Separate from `pendingAsksAtom` because the Notification hook is observability-
// only — the GUI never intercepts the in-terminal prompt, so clearing rules
// follow CC turn-state transitions (Stop / PostToolUse / PermissionDenied) rather
// than a paired resolve event.
export const pendingAttentionAtom = sessionScopedMapAtom<true>();
