import { useAtomValue, useAtom, useStore } from "jotai";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { ArrowLeft, MessagesSquare } from "lucide-react";
import {
  crossSessionThreadsAtom,
  crossSessionActiveThreadAtom,
  crossSessionMessagesAtom,
  crossSessionUnreadMapAtom,
  openCrossSessionThread,
  markCrossSessionSeen,
  loadCrossSessionThreads,
  type CrossSessionThread,
} from "@/stores/crossSession";
import { workspacesAtom } from "@/stores/workspace";
import { focusIfPanelZone } from "@/lib/panelFocus";
import { focusZoneAtom } from "@/stores/shortcuts";

function relativeTime(epochSecs: number): string {
  const delta = Math.max(0, Math.floor(Date.now() / 1000) - epochSecs);
  if (delta < 60) return `${delta}s`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m`;
  if (delta < 86_400) return `${Math.floor(delta / 3600)}h`;
  return `${Math.floor(delta / 86_400)}d`;
}

export function CrossSessionPanel() {
  const threads = useAtomValue(crossSessionThreadsAtom);
  const [activeThreadId, setActiveThreadId] = useAtom(crossSessionActiveThreadAtom);
  const messages = useAtomValue(crossSessionMessagesAtom);
  const unreadMap = useAtomValue(crossSessionUnreadMapAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const store = useStore();
  const rootRef = useRef<HTMLDivElement>(null);
  const messagesRef = useRef<HTMLDivElement>(null);

  // session id → display name (fallback to a short id when unknown).
  const nameOf = useMemo(() => {
    const map = new Map<string, string>();
    for (const ws of workspaces) {
      for (const s of ws.sessions) map.set(s.id, s.name);
    }
    return (id: string) => map.get(id) ?? id.slice(0, 8);
  }, [workspaces]);

  // Refresh roster on mount (covers a panel opened before any event fired).
  useEffect(() => {
    void loadCrossSessionThreads(store);
  }, [store]);

  const open = useCallback(
    (threadId: string) => {
      void openCrossSessionThread(store, threadId);
    },
    [store],
  );

  const back = useCallback(() => {
    setActiveThreadId(null);
  }, [setActiveThreadId]);

  // Looking at the panel clears the open thread's unread badge — the auto-open
  // selects the thread WITHOUT marking it seen, so the indicator persists until
  // the user actually engages.
  const markActiveSeen = useCallback(() => {
    if (activeThreadId) void markCrossSessionSeen(store, activeThreadId);
  }, [store, activeThreadId]);

  // A message arriving while the user is already on the panel (panel zone
  // focused) is being read live — re-mark the active thread seen so the sidebar
  // unread badge doesn't reappear. onMouseDown/onFocus only fire on the initial
  // engagement, not on subsequent inbound messages.
  useEffect(() => {
    if (focusZone === "panel" && activeThreadId) markActiveSeen();
  }, [messages, focusZone, activeThreadId, markActiveSeen]);

  // Mount-time focus + initial list cursor (canonical right-panel pattern,
  // patterns.md §5.2). focusIfPanelZone only grabs focus when the panel zone is
  // the intended target (avoids stealing the terminal prompt on a restore).
  useEffect(() => {
    const timer = setTimeout(() => {
      const root = rootRef.current;
      if (!root) return;
      focusIfPanelZone(root);
      if (root.querySelector("[data-nav-selected='true']")) return;
      root
        .querySelector<HTMLElement>("[data-nav-item]")
        ?.setAttribute("data-nav-selected", "true");
    }, 50);
    return () => clearTimeout(timer);
  }, [activeThreadId]);

  // Window-level keyboard nav (patterns.md §5.2 + §1.4). Window-level because the
  // panel div rarely holds DOM focus (RightPanel's zone container does). Uses
  // `e.key` not `e.code` — native WebKitGTK keydown does not reliably populate
  // `code` (matches the Spec/PR panels). Backspace returns from detail→list,
  // mirroring the Spec panel + PR viewer convention.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.getAttribute("contenteditable") === "true";
      if (e.altKey || e.ctrlKey || e.metaKey || inField) return;
      if (
        target?.closest("[data-focus-zone='sidebar']") ||
        target?.closest("[role='dialog']") ||
        target?.closest("[role='listbox']")
      )
        return;
      // Only act while the panel zone owns the interaction.
      if (!target?.closest("[data-focus-zone='panel']")) return;
      const root = rootRef.current;
      if (!root) return;

      // Detail view: Backspace/← back to the list; ↑/↓ scroll the messages.
      if (store.get(crossSessionActiveThreadAtom)) {
        if (e.key === "Backspace" || e.key === "ArrowLeft") {
          e.preventDefault();
          back();
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          messagesRef.current?.scrollBy({ top: 48 });
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          messagesRef.current?.scrollBy({ top: -48 });
        }
        return;
      }

      // List view: ↑/↓ move the cursor; Enter opens the selected thread.
      const items = Array.from(root.querySelectorAll<HTMLElement>("[data-nav-item]"));
      if (items.length === 0) return;
      const selected = root.querySelector<HTMLElement>("[data-nav-selected='true']");
      const idx = selected ? items.indexOf(selected) : -1;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const next =
          e.key === "ArrowDown"
            ? Math.min(idx + 1, items.length - 1)
            : Math.max(idx - 1, 0);
        for (const item of items) item.removeAttribute("data-nav-selected");
        items[next].setAttribute("data-nav-selected", "true");
        items[next].scrollIntoView({ block: "nearest" });
      } else if (e.key === "Enter") {
        const tid = selected?.dataset.threadId;
        if (tid) {
          e.preventDefault();
          open(tid);
        }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store, back, open]);

  if (activeThreadId) {
    const thread = threads.find((t) => t.id === activeThreadId);
    return (
      <div
        ref={rootRef}
        tabIndex={-1}
        className="flex h-full flex-col bg-card text-foreground outline-none"
        onMouseDown={markActiveSeen}
        onFocus={markActiveSeen}
      >
        <div className="flex items-center gap-2 border-b border-border/40 px-2.5 py-1.5">
          <button
            type="button"
            onClick={back}
            className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent"
          >
            <ArrowLeft size={13} /> Threads
            <kbd className="ml-0.5 rounded bg-muted/60 px-1 text-[9px]">Backspace</kbd>
          </button>
          {thread && (
            <span className="ml-auto text-[10px] text-muted-foreground">
              {thread.status} · {thread.msg_count} msg
            </span>
          )}
        </div>
        <div ref={messagesRef} className="min-h-0 flex-1 overflow-y-auto px-2.5 py-2">
          {messages.length === 0 ? (
            <p className="py-6 text-center text-[11px] text-muted-foreground">No messages.</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {messages.map((m) => (
                <li
                  key={m.id}
                  className="rounded-lg border border-border/40 bg-secondary/20 px-2.5 py-1.5"
                >
                  <div className="mb-0.5 flex items-center gap-1.5 text-[10px] text-muted-foreground">
                    <span className="font-medium text-foreground/80">{nameOf(m.from_session)}</span>
                    <span>→</span>
                    <span>{nameOf(m.to_session)}</span>
                    <span className="ml-auto">hop {m.depth}</span>
                    <span>{relativeTime(m.created_at)}</span>
                  </div>
                  <p className="whitespace-pre-wrap break-words text-[12px] leading-snug">{m.body}</p>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    );
  }

  return (
    <div ref={rootRef} tabIndex={-1} className="flex h-full flex-col bg-card text-foreground outline-none">
      {threads.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
          <MessagesSquare size={20} className="opacity-50" />
          <p className="text-[11px]">No cross-session conversations yet.</p>
        </div>
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto py-1">
          {threads.map((t) => (
            <ThreadRow
              key={t.id}
              thread={t}
              unread={threadUnread(t, unreadMap)}
              nameOf={nameOf}
              onOpen={() => open(t.id)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function threadUnread(t: CrossSessionThread, unreadMap: Record<string, number>): number {
  return t.participants.reduce((acc, p) => acc + (unreadMap[p] ?? 0), 0) > 0 ? 1 : 0;
}

function ThreadRow({
  thread,
  unread,
  nameOf,
  onOpen,
}: {
  thread: CrossSessionThread;
  unread: number;
  nameOf: (id: string) => string;
  onOpen: () => void;
}) {
  const others = thread.participants.map(nameOf).join(", ");
  return (
    <li
      data-nav-item
      data-thread-id={thread.id}
      onClick={onOpen}
      className="mx-1 cursor-pointer rounded-md px-2 py-1.5 hover:bg-accent/60"
    >
      <div className="flex items-center gap-1.5">
        <span className="truncate text-[12px] font-medium">{others}</span>
        {unread > 0 && <span className="size-1.5 shrink-0 rounded-full bg-primary" />}
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">
          {relativeTime(thread.created_at)}
        </span>
      </div>
      <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
        <span
          className={
            thread.status === "active"
              ? "text-emerald-500/80"
              : thread.status === "closed"
                ? "text-muted-foreground"
                : ""
          }
        >
          {thread.status}
        </span>
        <span>· {thread.msg_count} msg</span>
      </div>
    </li>
  );
}
