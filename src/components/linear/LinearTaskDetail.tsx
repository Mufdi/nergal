import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import { FloatingPanel } from "@/components/floating/FloatingPanel";
import { TooltipProvider } from "@/components/ui/tooltip";
import * as terminalService from "@/components/terminal/terminalService";
import { focusZoneAtom } from "@/stores/shortcuts";
import { activeSessionIdAtom } from "@/stores/workspace";
import { clampGeometryToViewport, type FloatingGeometry } from "@/stores/scratchpad";
import { linearDetailIssueIdAtom } from "@/stores/linear";
import {
  LinearIssueBody,
  LinearIssueTitleContent,
  useLinearIssueController,
} from "@/components/linear/LinearTaskView";

const DETAIL_PANEL_ID = "linear-issue-detail";
const DEFAULT_GEOMETRY: FloatingGeometry = { x: 200, y: 90, width: 780, height: 660 };
const MIN_WIDTH = 680;
const MIN_HEIGHT = 580;

export function LinearTaskDetail() {
  const [issueId, setIssueId] = useAtom(linearDetailIssueIdAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const [geometry, setGeometry] = useState<FloatingGeometry>(DEFAULT_GEOMETRY);
  const wasOpenRef = useRef(false);

  const c = useLinearIssueController({ issueId, setIssueId });

  // Restore focus when the modal closes (mirrors ClickUpTaskDetail)
  useEffect(() => {
    if (issueId !== null) {
      wasOpenRef.current = true;
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
            <LinearIssueTitleContent c={c} />
          </TooltipProvider>
        }
      >
        <LinearIssueBody c={c} layout="modal" />
      </FloatingPanel>
    </div>
  );
}
