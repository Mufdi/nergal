import { useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { TooltipProvider } from "@/components/ui/tooltip";
import { activeSessionIdAtom } from "@/stores/workspace";
import {
  activeSessionClickUpPinsAtom,
  activeSessionClickUpTaskAtom,
  clickupClosedTasksAtom,
  clickupClosureOfferAtom,
  clickupTasksAtom,
  reinjectTaskAction,
  requestBindTaskAction,
  requestSendTaskAction,
  spawnWorktreeWithTaskAction,
  togglePinTaskAction,
} from "@/stores/clickup";
import { open as openShell } from "@tauri-apps/plugin-shell";
import {
  ClickUpTaskBody,
  TaskHistoryNav,
  TaskTitleContent,
  TaskVerbToolbar,
  useClickUpTaskController,
} from "@/components/clickup/ClickUpTaskView";

const VERB_KEYS = ["KeyS", "KeyW", "KeyP", "KeyB", "KeyR", "KeyC", "KeyO"];

/// A ClickUp task rendered as a full document tab (single column). Mirrors the
/// floating detail's keyboard model — the index cursor lives on the body, and
/// the contextual verbs (S/W/P/B/R/C/O) + Ctrl+←/→ drill-in history fire while
/// focus is inside this tab. Drill-in is local state (it never touches
/// clickupDetailTaskIdAtom), so the tab is independent of the modal.
export function ClickUpTaskTab({ taskId: rootTaskId }: { taskId: string }) {
  // Seeded from the tab's data; drill-in mutates this, the tab's `data.taskId`
  // stays at the root.
  const [taskId, setTaskId] = useState(rootTaskId);
  // A tab always has a task; drill-in only ever sets a concrete id (the null
  // branches in the controller are the modal's close path), so swallow null.
  const c = useClickUpTaskController({
    taskId,
    setTaskId: (id) => { if (id !== null) setTaskId(id); },
  });
  const wrapperRef = useRef<HTMLDivElement | null>(null);

  const tasks = useAtomValue(clickupTasksAtom);
  const closedTasks = useAtomValue(clickupClosedTasksAtom);
  const boundTaskId = useAtomValue(activeSessionClickUpTaskAtom);
  const pinnedTaskIds = useAtomValue(activeSessionClickUpPinsAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const requestSend = useSetAtom(requestSendTaskAction);
  const spawnWorktree = useSetAtom(spawnWorktreeWithTaskAction);
  const togglePin = useSetAtom(togglePinTaskAction);
  const requestBind = useSetAtom(requestBindTaskAction);
  const reinject = useSetAtom(reinjectTaskAction);
  const setClosureOffer = useSetAtom(clickupClosureOfferAtom);

  const taskUrl = c.task?.url ?? null;

  // Contextual verbs — same convention as the floating detail, but scoped to
  // this tab's subtree (so multiple open task tabs don't all fire). No "T":
  // converting to a tab is meaningless once you're already in one.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (!VERB_KEYS.includes(e.code)) return;
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (!wrapperRef.current?.contains(target)) return;
      const id = taskId;
      if (e.code === "KeyS") { e.preventDefault(); requestSend(id); }
      else if (e.code === "KeyW") { e.preventDefault(); void spawnWorktree(id); }
      else if (e.code === "KeyP") { e.preventDefault(); void togglePin(id); }
      else if (e.code === "KeyB") { e.preventDefault(); void requestBind(id); }
      else if (e.code === "KeyR") {
        if (id === boundTaskId || pinnedTaskIds.includes(id)) { e.preventDefault(); void reinject(id); }
      } else if (e.code === "KeyC") {
        if (id === boundTaskId && activeSessionId) {
          e.preventDefault();
          setClosureOffer({ taskId: id, sessionId: activeSessionId });
        }
      } else if (e.code === "KeyO") {
        const url = [...tasks, ...closedTasks].find((t) => t.id === id)?.url ?? taskUrl;
        if (url && /^https?:\/\//i.test(url)) { e.preventDefault(); void openShell(url); }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [taskId, requestSend, spawnWorktree, togglePin, requestBind, reinject, setClosureOffer, boundTaskId, pinnedTaskIds, activeSessionId, tasks, closedTasks, taskUrl]);

  // Ctrl+←/→ steps the drill-in history (matches the modal). Scoped to this
  // tab; editable fields keep their own word-nav.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      const t = e.target as HTMLElement | null;
      if (t?.tagName === "TEXTAREA" || t?.tagName === "INPUT") return;
      if (!wrapperRef.current?.contains(t)) return;
      e.preventDefault();
      c.stepHistory(e.code === "ArrowLeft" ? -1 : 1);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div ref={wrapperRef} className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-border/50 px-2 py-1">
        <TaskHistoryNav c={c} />
        <div className="flex min-w-0 flex-1 items-center gap-1.5">
          <TaskTitleContent c={c} />
        </div>
        <TooltipProvider delay={0}>
          {c.taskId && <TaskVerbToolbar taskId={c.taskId} taskUrl={taskUrl} />}
        </TooltipProvider>
      </div>
      <div className="min-h-0 flex-1">
        <ClickUpTaskBody c={c} layout="tab" />
      </div>
    </div>
  );
}
