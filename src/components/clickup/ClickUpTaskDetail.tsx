import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { invoke } from "@/lib/tauri";
import { FloatingPanel } from "@/components/floating/FloatingPanel";
import { TooltipProvider } from "@/components/ui/tooltip";
import * as terminalService from "@/components/terminal/terminalService";
import { focusZoneAtom } from "@/stores/shortcuts";
import { activeSessionIdAtom } from "@/stores/workspace";
import {
  clampGeometryToViewport,
  type FloatingGeometry,
} from "@/stores/scratchpad";
import {
  activeSessionClickUpPinsAtom,
  activeSessionClickUpTaskAtom,
  clickupClosureOfferAtom,
  clickupDetailTaskIdAtom,
  clickupTasksAtom,
  openClickUpTaskTabAction,
  reinjectTaskAction,
  requestBindTaskAction,
  requestSendTaskAction,
  spawnWorktreeWithTaskAction,
  togglePinTaskAction,
} from "@/stores/clickup";
import {
  ClickUpTaskBody,
  TaskHistoryNav,
  TaskTitleContent,
  TaskVerbToolbar,
  useClickUpTaskController,
} from "@/components/clickup/ClickUpTaskView";

const DETAIL_PANEL_ID = "clickup-task-detail";
const DEFAULT_GEOMETRY: FloatingGeometry = { x: 200, y: 90, width: 780, height: 660 };
// A two-column issue detail has a real minimum usable footprint — enforce it
// so a geometry persisted from an earlier, narrower layout grows on load
// instead of staying cramped.
const MIN_WIDTH = 680;
const MIN_HEIGHT = 600;

export function ClickUpTaskDetail() {
  const [taskId, setTaskId] = useAtom(clickupDetailTaskIdAtom);
  const tasks = useAtomValue(clickupTasksAtom);
  const boundTaskId = useAtomValue(activeSessionClickUpTaskAtom);
  const pinnedTaskIds = useAtomValue(activeSessionClickUpPinsAtom);
  const requestSend = useSetAtom(requestSendTaskAction);
  const spawnWorktree = useSetAtom(spawnWorktreeWithTaskAction);
  const togglePin = useSetAtom(togglePinTaskAction);
  const requestBind = useSetAtom(requestBindTaskAction);
  const reinject = useSetAtom(reinjectTaskAction);
  const openTaskTab = useSetAtom(openClickUpTaskTabAction);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setClosureOffer = useSetAtom(clickupClosureOfferAtom);
  const [geometry, setGeometry] = useState<FloatingGeometry>(DEFAULT_GEOMETRY);
  const wasOpenRef = useRef(false);
  // Set when the close is triggered by "open as tab" — the close-focus effect
  // must NOT yank focus back to the panel/terminal, the new tab owns it.
  const suppressCloseFocusRef = useRef(false);

  const c = useClickUpTaskController({ taskId, setTaskId });

  function convertToTab(id: string, fromModal: boolean) {
    openTaskTab(id);
    if (fromModal) {
      suppressCloseFocusRef.current = true;
      setTaskId(null);
    }
  }

  // Contextual task verbs: bare letters scoped to ClickUp surfaces. Inside the
  // floating detail the open task wins; inside the right panel the
  // data-nav-selected cursor row wins (panel rows are never DOM-focused). The
  // handler lives here — the detail is the always-mounted ClickUp surface, so
  // the keys keep working when a chip opens the detail without the right panel.
  useEffect(() => {
    const VERB_KEYS = ["KeyS", "KeyW", "KeyP", "KeyB", "KeyR", "KeyC", "KeyO", "KeyT"];
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (!VERB_KEYS.includes(e.code)) return;
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      const inClickupZone = !!target?.closest("[data-focus-zone='clickup']");
      const inPanelZone = !!target?.closest("[data-focus-zone='panel']");
      if (!inClickupZone && !inPanelZone) return;
      const selectedRow = document.querySelector<HTMLElement>(
        "[data-focus-zone='clickup'] [data-nav-selected='true'][data-task-id]",
      );
      const id = inClickupZone && taskId ? taskId : selectedRow?.dataset.taskId ?? taskId;
      if (!id) return;
      // S/W/P/B/T always apply; R/C/O are conditional — only swallow the key
      // when we actually act.
      if (e.code === "KeyS") { e.preventDefault(); requestSend(id); }
      else if (e.code === "KeyW") { e.preventDefault(); void spawnWorktree(id); }
      else if (e.code === "KeyP") { e.preventDefault(); void togglePin(id); }
      else if (e.code === "KeyB") { e.preventDefault(); void requestBind(id); }
      // The floating detail is a clickup zone OUTSIDE the right panel; the
      // panel rows are a clickup zone INSIDE it. Only the former closes on
      // convert (the panel has nothing to close).
      else if (e.code === "KeyT") { e.preventDefault(); convertToTab(id, inClickupZone && !inPanelZone); }
      else if (e.code === "KeyR") {
        if (id === boundTaskId || pinnedTaskIds.includes(id)) { e.preventDefault(); void reinject(id); }
      } else if (e.code === "KeyC") {
        if (id === boundTaskId && activeSessionId) {
          e.preventDefault();
          setClosureOffer({ taskId: id, sessionId: activeSessionId });
        }
      } else if (e.code === "KeyO") {
        const url = tasks.find((t) => t.id === id)?.url ?? (c.detail?.task?.id === id ? c.detail.task?.url : null);
        if (url && /^https?:\/\//i.test(url)) { e.preventDefault(); void openShell(url); }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, requestSend, spawnWorktree, togglePin, requestBind, reinject, openTaskTab, boundTaskId, pinnedTaskIds, activeSessionId, setClosureOffer, tasks, c.detail]);

  // Close path (Esc + the X button) hands focus back to the PTY — same pattern
  // as the scratchpad / vault search close. Skipped when the close is an
  // "open as tab" (the new tab takes focus). rAF lets React finish unmounting.
  useEffect(() => {
    if (taskId !== null) {
      wasOpenRef.current = true;
      return;
    }
    if (suppressCloseFocusRef.current) {
      suppressCloseFocusRef.current = false;
      wasOpenRef.current = false;
      return;
    }
    if (wasOpenRef.current) {
      requestAnimationFrame(() => {
        const panel = document.querySelector<HTMLElement>("[data-focus-zone='clickup'][tabindex]");
        if (panel) {
          setFocusZone("panel");
          panel.focus({ preventScroll: true });
        } else if (activeSessionId) {
          setFocusZone("terminal");
          terminalService.focusActive();
        }
      });
    }
    wasOpenRef.current = false;
  }, [taskId, activeSessionId, setFocusZone]);

  // Geometry persists in the same SQLite panel-geometry row family as the
  // scratchpad — FloatingPanel was built for exactly this reuse.
  useEffect(() => {
    invoke<{ geometry_json: string; opacity: number } | null>("scratchpad_get_geometry", {
      panelId: DETAIL_PANEL_ID,
    })
      .then((row) => {
        if (!row) return;
        try {
          const saved = clampGeometryToViewport(JSON.parse(row.geometry_json) as FloatingGeometry);
          setGeometry({
            ...saved,
            width: Math.max(saved.width, MIN_WIDTH),
            height: Math.max(saved.height, MIN_HEIGHT),
          });
        } catch {
          setGeometry(DEFAULT_GEOMETRY);
        }
      })
      .catch(() => {});
  }, []);

  // Ctrl+←/→ steps the drill-in history (no collision — shortcuts.ts has no
  // Ctrl+Arrow binding). Editable fields keep their own word-nav.
  useEffect(() => {
    if (taskId === null) return;
    function onKey(e: KeyboardEvent) {
      if (!e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      const t = e.target as HTMLElement | null;
      if (t?.tagName === "TEXTAREA" || t?.tagName === "INPUT") return;
      e.preventDefault();
      c.stepHistory(e.code === "ArrowLeft" ? -1 : 1);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId]);

  function handleGeometryChange(next: FloatingGeometry) {
    setGeometry(next);
    invoke("scratchpad_set_geometry", {
      panelId: DETAIL_PANEL_ID,
      geometryJson: JSON.stringify(next),
      opacity: 1,
    }).catch(() => {});
  }

  return (
    // display:contents wrapper only marks the zone for the contextual keys —
    // the detail mounts at Workspace level, outside the panel's zone subtree.
    <div data-focus-zone="clickup" className="contents">
      <FloatingPanel
        panelId={DETAIL_PANEL_ID}
        open={taskId !== null}
        onClose={() => setTaskId(null)}
        geometry={geometry}
        onGeometryChange={handleGeometryChange}
        opacity={1}
        zIndex={50}
        minWidth={MIN_WIDTH}
        minHeight={MIN_HEIGHT}
        accent
        autoFocus
        title={
          <>
            <TaskHistoryNav c={c} />
            <TaskTitleContent c={c} />
          </>
        }
        toolbar={
          <TooltipProvider delay={0}>
            {taskId && (
              <TaskVerbToolbar
                taskId={taskId}
                taskUrl={c.task?.url ?? null}
                onConvertToTab={() => convertToTab(taskId, true)}
              />
            )}
          </TooltipProvider>
        }
      >
        <ClickUpTaskBody c={c} layout="modal" />
      </FloatingPanel>
    </div>
  );
}
