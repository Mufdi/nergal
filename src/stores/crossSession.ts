import { atom, type getDefaultStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invoke, listen } from "@/lib/tauri";
import { activePanelViewAtom, expandRightPanelAtom } from "./rightPanel";
import { configAtom } from "./config";

type Store = ReturnType<typeof getDefaultStore>;

/// Mirrors `db::CrossSessionThread` (snake_case serialize).
export interface CrossSessionThread {
  id: string;
  originator_session: string;
  participants: string[];
  status: string;
  max_hops: number;
  msg_count: number;
  msg_budget: number | null;
  deadline_at: number | null;
  created_at: number;
}

/// Mirrors `db::CrossSessionMessage`.
export interface CrossSessionMessage {
  id: string;
  thread_id: string;
  from_session: string;
  to_session: string;
  body: string;
  depth: number;
  dedup_key: string;
  agent_consumed_at: number | null;
  human_seen_at: number | null;
  created_at: number;
}

/// All threads (the panel roster), newest-first.
export const crossSessionThreadsAtom = atom<CrossSessionThread[]>([]);

/// Per-session human-unread counts (drives the SessionRow badge). Keyed by
/// `to_session`. Independent of agent delivery (`agent_consumed_at`).
export const crossSessionUnreadMapAtom = atom<Record<string, number>>({});

/// Session ids that participate in at least one ACTIVE (non-closed) thread —
/// drives the "this session is in a live conversation" indicator on every
/// participant (sender AND recipient), independent of unread state.
export const crossSessionActiveParticipantsAtom = atom<Set<string>>((get) => {
  const set = new Set<string>();
  for (const t of get(crossSessionThreadsAtom)) {
    if (t.status === "active") for (const p of t.participants) set.add(p);
  }
  return set;
});

/// The thread open in the detail view (null = list view).
export const crossSessionActiveThreadAtom = atom<string | null>(null);

/// Messages of the active thread, oldest-first.
export const crossSessionMessagesAtom = atom<CrossSessionMessage[]>([]);

export async function loadCrossSessionThreads(store: Store): Promise<void> {
  try {
    const threads = await invoke<CrossSessionThread[]>("cross_session_list_threads");
    store.set(crossSessionThreadsAtom, threads);
  } catch (err) {
    console.warn("[crosssession] list threads failed:", err);
  }
}

export async function loadCrossSessionUnread(store: Store): Promise<void> {
  try {
    const counts = await invoke<Record<string, number>>("cross_session_unread_counts");
    store.set(crossSessionUnreadMapAtom, counts);
  } catch (err) {
    console.warn("[crosssession] unread counts failed:", err);
  }
}

/// Select a thread in the detail view WITHOUT marking it seen — for the live
/// auto-open while agents are messaging, so the unread indicator persists until
/// the user actually looks at the panel.
export async function selectCrossSessionThreadLive(store: Store, threadId: string): Promise<void> {
  store.set(crossSessionActiveThreadAtom, threadId);
  try {
    const messages = await invoke<CrossSessionMessage[]>("cross_session_thread_messages", {
      threadId,
    });
    store.set(crossSessionMessagesAtom, messages);
  } catch (err) {
    console.warn("[crosssession] select thread failed:", err);
  }
}

/// Mark a thread human-seen (clears the unread badge). Never touches
/// `agent_consumed_at` — looking at the panel never cancels agent delivery.
export async function markCrossSessionSeen(store: Store, threadId: string): Promise<void> {
  try {
    await invoke("cross_session_mark_seen", { threadId });
    await loadCrossSessionUnread(store);
  } catch (err) {
    console.warn("[crosssession] mark seen failed:", err);
  }
}

/// Open a thread in the detail view from an explicit user gesture: select it AND
/// mark it human-seen (clears the unread badge).
export async function openCrossSessionThread(store: Store, threadId: string): Promise<void> {
  await selectCrossSessionThreadLive(store, threadId);
  await markCrossSessionSeen(store, threadId);
}

export const setupCrossSessionListeners = async (store: Store): Promise<UnlistenFn[]> => {
  const refresh = () => {
    void loadCrossSessionThreads(store);
    void loadCrossSessionUnread(store);
    const active = store.get(crossSessionActiveThreadAtom);
    if (active) {
      void invoke<CrossSessionMessage[]>("cross_session_thread_messages", { threadId: active })
        .then((m) => store.set(crossSessionMessagesAtom, m))
        .catch(() => {});
    }
  };

  // Initial hydrate so the badge + panel reflect history on launch.
  refresh();

  const unlisteners: UnlistenFn[] = [];

  // A new message: refresh, then SOFT-open the panel to the live thread. Soft =
  // expand + select the thread (so it streams in), but only when the right panel
  // is collapsed or already on Cross-session — never override a panel the user
  // deliberately switched to, never steal keyboard focus, never lock the panel.
  // The user stays free to type in the terminal, change the panel, or move focus.
  unlisteners.push(
    await listen<{ thread_id?: string }>("crossmsg:new", (payload) => {
      refresh();
      const threadId = payload?.thread_id;
      if (!threadId) return;
      if (!store.get(configAtom).cross_session?.enabled) return;
      void selectCrossSessionThreadLive(store, threadId);
      const currentView = store.get(activePanelViewAtom);
      if (currentView === null || currentView === "crosssession") {
        store.set(activePanelViewAtom, "crosssession");
        store.set(expandRightPanelAtom, (n) => n + 1);
      }
    }),
  );

  for (const event of [
    "crossmsg:agent-consumed",
    "crossmsg:human-seen",
    "crossmsg:thread-closed",
  ]) {
    unlisteners.push(await listen(event, refresh));
  }
  return unlisteners;
};
