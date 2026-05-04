import { atom } from "jotai";
import { atomFamily } from "jotai/utils";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appStore } from "./jotaiStore";

/// Capability flags emitted by the Rust adapter via custom Serialize on
/// `AgentCapability` (see `src-tauri/src/agents/mod.rs`). Stored as a string
/// union here so TypeScript catches typos at gating sites; the Rust serializer
/// is the source of truth — keep this list in sync.
export type AgentCapability =
  | "PLAN_REVIEW"
  | "ASK_USER_BLOCKING"
  | "TOOL_CALL_EVENTS"
  | "STRUCTURED_TRANSCRIPT"
  | "RAW_COST_PER_MESSAGE"
  | "TASK_LIST"
  | "SESSION_RESUME"
  | "ANNOTATIONS_INJECT";

/// All capabilities CC declares. Used as the default set for legacy sessions
/// that predate the agent_id column being plumbed end-to-end (Tauri commands
/// returning `Session` without `agent_capabilities`).
export const FULL_CAPABILITY_SET: ReadonlySet<AgentCapability> = new Set<AgentCapability>([
  "PLAN_REVIEW",
  "ASK_USER_BLOCKING",
  "TOOL_CALL_EVENTS",
  "STRUCTURED_TRANSCRIPT",
  "RAW_COST_PER_MESSAGE",
  "TASK_LIST",
  "SESSION_RESUME",
  "ANNOTATIONS_INJECT",
]);

/// Detection result for a single registered adapter, emitted by the backend
/// on startup and on `cluihud rescan-agents` via the `agents:detected` event.
export interface AgentDetection {
  id: string;
  installed: boolean;
  binary_path: string | null;
  config_path: string | null;
  version: string | null;
}

/// Compact metadata about a single agent from the adapter's perspective.
export interface AgentMetadata {
  id: string;
  display_name: string;
  capabilities: AgentCapability[];
}

/// All adapters the runtime knows about, keyed by id. Populated synchronously
/// when a session is activated (from `Session.agent_capabilities` on the
/// session row) and refreshed when the backend emits `agents:detected`.
export const availableAgentsAtom = atom<AgentDetection[]>([]);

/// Active session's adapter metadata. `null` when no session is selected.
export const activeAgentMetadataAtom = atom<AgentMetadata | null>(null);

/// Effective capability set for the active session. Sync read for UI gating —
/// no async lookup, no TOCTOU window.
export const activeAgentCapabilitiesAtom = atom<ReadonlySet<AgentCapability>>(
  (get) => {
    const meta = get(activeAgentMetadataAtom);
    if (!meta) return FULL_CAPABILITY_SET;
    return new Set(meta.capabilities);
  },
);

/// `useAtomValue(hasCapabilityAtom("PLAN_REVIEW"))` is the canonical UI gate.
/// atomFamily caches per-capability subscriptions so re-renders are scoped.
export const hasCapabilityAtom = atomFamily((cap: AgentCapability) =>
  atom((get) => get(activeAgentCapabilitiesAtom).has(cap)),
);

/// Activate a session's agent metadata (called from session-activation flows).
/// Pass `null` to clear (e.g. when the last session is deleted).
export const activateAgentMetadataAtom = atom(
  null,
  (_get, set, meta: AgentMetadata | null) => {
    set(activeAgentMetadataAtom, meta);
  },
);

let detachAgentsDetectedListener: UnlistenFn | null = null;

/// Wire up the `agents:detected` listener so the store reflects whatever the
/// backend currently sees. Idempotent — call once at app startup. Detaches if
/// called again (returning the prior unlisten function for tests).
export async function setupAgentListeners(): Promise<UnlistenFn> {
  if (detachAgentsDetectedListener) {
    detachAgentsDetectedListener();
  }
  detachAgentsDetectedListener = await listen<AgentDetection[]>(
    "agents:detected",
    (event) => {
      appStore.set(availableAgentsAtom, event.payload);
    },
  );
  return detachAgentsDetectedListener;
}
