import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import { FloatingPanel } from "@/components/floating/FloatingPanel";
import { TooltipProvider } from "@/components/ui/tooltip";
import * as terminalService from "@/components/terminal/terminalService";
import { focusZoneAtom } from "@/stores/shortcuts";
import { activeSessionIdAtom } from "@/stores/workspace";
import { clampGeometryToViewport, type FloatingGeometry } from "@/stores/scratchpad";
import {
  activeSessionLinearIssueAtom,
  activeSessionLinearPinsAtom,
  consumeLinearDetailCloseFocusSuppress,
  linearClosureOfferAtom,
  linearDetailIssueIdAtom,
  reinjectIssueAction,
  requestBindIssueAction,
  requestSendIssueAction,
  spawnWorktreeWithIssueAction,
  togglePinIssueAction,
} from "@/stores/linear";
import {
  LinearIssueBody,
  LinearIssueHistoryNav,
  LinearIssueTitleContent,
  LinearVerbToolbar,
  useLinearIssueController,
} from "@/components/linear/LinearTaskView";

const DETAIL_PANEL_ID = "linear-issue-detail";
const DEFAULT_GEOMETRY: FloatingGeometry = { x: 200, y: 90, width: 780, height: 660 };
const MIN_WIDTH = 680;
const MIN_HEIGHT = 600;

export function LinearTaskDetail() {
  const [issueId, setIssueId] = useAtom(linearDetailIssueIdAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const boundIssueId = useAtomValue(activeSessionLinearIssueAtom);
  const pinnedIssueIds = useAtomValue(activeSessionLinearPinsAtom);
  const requestSend = useSetAtom(requestSendIssueAction);
  const spawnWorktree = useSetAtom(spawnWorktreeWithIssueAction);
  const togglePin = useSetAtom(togglePinIssueAction);
  const requestBind = useSetAtom(requestBindIssueAction);
  const reinject = useSetAtom(reinjectIssueAction);
  const [, setClosureOffer] = useAtom(linearClosureOfferAtom);
  const [geometry, setGeometry] = useState<FloatingGeometry>(DEFAULT_GEOMETRY);
  const wasOpenRef = useRef(false);

  const c = useLinearIssueController({ issueId, setIssueId });

  // Contextual issue verbs: bare letters scoped to the Linear zone. The open
  // detail issue wins; otherwise the panel's data-nav-selected row (rows expose
  // data-issue-id). KeyC = close-out (free in VERB_KEYS, Decision 6).
  useEffect(() => {
    const VERB_KEYS = ["KeyS", "KeyW", "KeyP", "KeyB", "KeyR", "KeyC"];
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (!VERB_KEYS.includes(e.code)) return;
      const target = e.target as HTMLElement | null;
      const inField =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        !!target?.closest(".cm-editor") ||
        target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (!target?.closest("[data-focus-zone='linear']")) return;
      const selectedRow = document.querySelector<HTMLElement>(
        "[data-focus-zone='linear'] [data-nav-selected='true'][data-issue-id]",
      );
      const id = issueId ?? selectedRow?.dataset.issueId ?? null;
      if (!id) return;
      if (e.code === "KeyS") {
        e.preventDefault();
        requestSend(id);
      } else if (e.code === "KeyW") {
        e.preventDefault();
        void spawnWorktree(id);
      } else if (e.code === "KeyP") {
        e.preventDefault();
        void togglePin(id);
      } else if (e.code === "KeyB") {
        e.preventDefault();
        void requestBind(id);
      } else if (e.code === "KeyR") {
        // Re-inject only acts on an issue already bound or pinned to this session.
        if (id === boundIssueId || pinnedIssueIds.includes(id)) {
          e.preventDefault();
          void reinject(id);
        }
      } else if (e.code === "KeyC") {
        // Close-out verb (Decision 6 — manual primary path). Requires an active session.
        if (activeSessionId) {
          e.preventDefault();
          setClosureOffer({ issueId: id, sessionId: activeSessionId });
        }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [issueId, requestSend, spawnWorktree, togglePin, requestBind, reinject, boundIssueId, pinnedIssueIds, activeSessionId, setClosureOffer]);

  // Restore focus when the modal closes (mirrors ClickUpTaskDetail). Skipped
  // when a worktree spawn closed it — that flow owns focus (the new session's
  // terminal), so stealing it back to the panel would be wrong.
  useEffect(() => {
    if (issueId !== null) {
      wasOpenRef.current = true;
      return;
    }
    if (consumeLinearDetailCloseFocusSuppress()) {
      wasOpenRef.current = false;
      return;
    }
    if (wasOpenRef.current) {
      requestAnimationFrame(() => {
        const panel = document.querySelector<HTMLElement>("[data-focus-zone='linear'][tabindex]");
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
  }, [issueId, activeSessionId, setFocusZone]);

  // Geometry persisted in the same scratchpad row family as the ClickUp detail
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

  // Ctrl+←/→ steps drill-in history (mirrors ClickUpTaskDetail; no collision in shortcuts.ts).
  // Editable fields keep their own word-nav.
  useEffect(() => {
    if (issueId === null) return;
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
    // c.stepHistory is stable across renders (ref-backed); issueId is the real dep.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [issueId]);

  function handleGeometryChange(next: FloatingGeometry) {
    setGeometry(next);
    invoke("scratchpad_set_geometry", {
      panelId: DETAIL_PANEL_ID,
      geometryJson: JSON.stringify(next),
      opacity: 1,
    }).catch(() => {});
  }

  return (
    <div data-focus-zone="linear" className="contents">
      <FloatingPanel
        panelId={DETAIL_PANEL_ID}
        open={issueId !== null}
        onClose={() => setIssueId(null)}
        geometry={geometry}
        onGeometryChange={handleGeometryChange}
        opacity={1}
        zIndex={50}
        minWidth={MIN_WIDTH}
        minHeight={MIN_HEIGHT}
        accent
        autoFocus
        title={
          <TooltipProvider delay={0}>
            <LinearIssueHistoryNav c={c} />
            <LinearIssueTitleContent c={c} />
          </TooltipProvider>
        }
        toolbar={
          <TooltipProvider delay={0}>
            {issueId && <LinearVerbToolbar issueId={issueId} />}
          </TooltipProvider>
        }
      >
        <LinearIssueBody c={c} layout="modal" />
      </FloatingPanel>
    </div>
  );
}
