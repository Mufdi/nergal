import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import {
  CheckSquare,
  CircleCheck,
  ExternalLink,
  GitBranchPlus,
  Link2,
  Loader2,
  Paperclip,
  Pencil,
  Pin,
  PinOff,
  RefreshCw,
  Send,
  Square,
  UserMinus,
  Unlink,
} from "lucide-react";
import { invoke } from "@/lib/tauri";
import { FloatingPanel } from "@/components/floating/FloatingPanel";
import { MarkdownView } from "@/components/plan/MarkdownView";
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
  clickupOverlayAtom,
  clickupTasksAtom,
  clearOverlayEntry,
  reinjectTaskAction,
  requestBindTaskAction,
  requestSendTaskAction,
  setOverlayEntry,
  spawnWorktreeWithTaskAction,
  togglePinTaskAction,
  CLICKUP_ACTION_LABELS as ACTION_LABELS,
  type ClickUpAttachment,
  type ClickUpChecklistItem,
  type ClickUpCustomValue,
  type ClickUpListStatus,
  type ClickUpTaskDetailData,
} from "@/stores/clickup";
import { toastsAtom } from "@/stores/toast";

const DETAIL_PANEL_ID = "clickup-task-detail";
const DEFAULT_GEOMETRY: FloatingGeometry = { x: 240, y: 120, width: 560, height: 520 };

/// Gate for the lazy thumbnail: a non-image `thumbnail_url` must never
/// auto-load, so the decision keys on the attachment's mimetype/extension —
/// not on the thumbnail field's presence.
export function isImageAttachment(att: ClickUpAttachment): boolean {
  if (att.mimetype?.startsWith("image/")) return true;
  const ref = (att.title ?? att.url ?? "").split("?")[0];
  return /\.(png|jpe?g|gif|webp|avif|bmp|svg)$/i.test(ref);
}

/// ClickUp content is multi-writer: an attachment/task URL is untrusted input
/// and `shell:allow-open` is unscoped, so only web URLs may reach xdg-open.
function openExternalUrl(url: string | null | undefined): void {
  if (!url || !/^https?:\/\//i.test(url)) return;
  void openShell(url);
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(0)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDateTime(ms: number): string {
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/// Type-aware read-only rendering of a custom value, falling back to
/// scalar/label heuristics and finally raw JSON for unknown shapes.
export function formatCustomValue(cv: ClickUpCustomValue): string | null {
  if (cv.value_json == null) return null;
  let value: unknown;
  try {
    value = JSON.parse(cv.value_json);
  } catch {
    return cv.value_json;
  }
  const typed = renderByType(cv, value);
  if (typed !== undefined) return typed;
  return renderValue(value);
}

/// `undefined` = type not handled here, defer to the generic renderer.
function renderByType(cv: ClickUpCustomValue, value: unknown): string | null | undefined {
  switch (cv.field_type) {
    case "automatic_progress":
    case "progress": {
      const obj = (typeof value === "object" && value !== null ? value : {}) as Record<string, unknown>;
      const pct = obj["percent_complete"] ?? obj["current"] ?? (typeof value === "number" ? value : null);
      return typeof pct === "number" ? `${Math.round(pct)}%` : null;
    }
    case "drop_down": {
      // Value is the selected option's orderindex or id; names live in the
      // field definition's type_config.
      const options = typeConfigOptions(cv);
      const match = options.find(
        (o) => o.orderindex === value || o.id === value,
      );
      return match ? optionLabel(match) : undefined;
    }
    case "labels": {
      if (!Array.isArray(value)) return undefined;
      const options = typeConfigOptions(cv);
      const names = value
        .map((id) => {
          const match = options.find((o) => o.id === id);
          return match ? optionLabel(match) : renderValue(id);
        })
        .filter((v): v is string => v !== null);
      return names.length > 0 ? names.join(", ") : null;
    }
    case "date": {
      const ms = typeof value === "string" ? Number(value) : value;
      return typeof ms === "number" && Number.isFinite(ms) ? formatDateTime(ms) : undefined;
    }
    default:
      return undefined;
  }
}

interface TypeConfigOption {
  id?: unknown;
  orderindex?: unknown;
  name?: unknown;
  label?: unknown;
}

function typeConfigOptions(cv: ClickUpCustomValue): TypeConfigOption[] {
  if (!cv.type_config_json) return [];
  try {
    const config = JSON.parse(cv.type_config_json) as { options?: TypeConfigOption[] };
    return Array.isArray(config.options) ? config.options : [];
  } catch {
    return [];
  }
}

function optionLabel(option: TypeConfigOption): string | null {
  return renderValue(option.name ?? option.label);
}

function renderValue(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "string") return value.length > 0 ? value : null;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    const parts = value.map(renderValue).filter((v): v is string => v !== null);
    return parts.length > 0 ? parts.join(", ") : null;
  }
  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;
    for (const key of ["name", "label", "username", "value"]) {
      const inner = renderValue(obj[key]);
      if (inner !== null) return inner;
    }
    return JSON.stringify(value);
  }
  return null;
}

function ToolbarAction({
  label,
  onClick,
  active = false,
  children,
}: {
  label: string;
  onClick: () => void;
  active?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex size-5 items-center justify-center rounded transition-colors ${
        active
          ? "text-primary hover:bg-secondary/60"
          : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

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
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setClosureOffer = useSetAtom(clickupClosureOfferAtom);
  const [overlay, setOverlay] = useAtom(clickupOverlayAtom);
  const addToast = useSetAtom(toastsAtom);
  const [detail, setDetail] = useState<ClickUpTaskDetailData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [geometry, setGeometry] = useState<FloatingGeometry>(DEFAULT_GEOMETRY);
  const wasOpenRef = useRef(false);

  // Write-control local state
  const [statuses, setStatuses] = useState<ClickUpListStatus[]>([]);
  const [editingDesc, setEditingDesc] = useState(false);
  const [descDraft, setDescDraft] = useState<string>("");
  const [editingDueDate, setEditingDueDate] = useState(false);
  const [dueDateDraft, setDueDateDraft] = useState<string>("");
  const [commentDraft, setCommentDraft] = useState<string>("");
  const [postingComment, setPostingComment] = useState(false);
  const [uncertainComment, setUncertainComment] = useState<{ text: string; sentAtMs: number } | null>(null);

  // Contextual task verbs: bare letters scoped to ClickUp surfaces (same
  // convention as ConflictsPanel O/T, PrViewer A). Inside the floating
  // detail the open task wins; inside the right panel the data-nav-selected
  // cursor row wins (panel rows are never DOM-focused — patterns.md §5.2).
  // The handler lives here — the detail is the always-mounted ClickUp
  // surface, so the keys keep working when the chip opens the detail
  // without the right panel.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (e.code !== "KeyS" && e.code !== "KeyW" && e.code !== "KeyP" && e.code !== "KeyB") return;
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
      e.preventDefault();
      if (e.code === "KeyS") requestSend(id);
      else if (e.code === "KeyW") void spawnWorktree(id);
      else if (e.code === "KeyP") void togglePin(id);
      else void requestBind(id);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [taskId, requestSend, spawnWorktree, togglePin, requestBind]);

  // Close path (Esc + the X button) hands focus back to the PTY — same
  // pattern as the scratchpad / vault search close. Only when a session is
  // active; rAF lets React finish unmounting first.
  useEffect(() => {
    if (taskId !== null) {
      wasOpenRef.current = true;
      return;
    }
    if (wasOpenRef.current && activeSessionId) {
      requestAnimationFrame(() => {
        setFocusZone("terminal");
        terminalService.focusActive();
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
          setGeometry(clampGeometryToViewport(JSON.parse(row.geometry_json) as FloatingGeometry));
        } catch {
          setGeometry(DEFAULT_GEOMETRY);
        }
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!taskId) {
      setDetail(null);
      setError(null);
      setStatuses([]);
      setEditingDesc(false);
      setEditingDueDate(false);
      setCommentDraft("");
      setUncertainComment(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId })
      .then((data) => {
        if (!cancelled) setDetail(data);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [taskId]);

  // Load statuses for the current task's list when the detail changes.
  // Keyed on detail (not task) to avoid a reference-before-declaration lint
  // error: `task` is derived from `detail` and declared later in the render.
  useEffect(() => {
    const listId = detail?.task?.list_id;
    if (!listId) { setStatuses([]); return; }
    let cancelled = false;
    invoke<ClickUpListStatus[]>("clickup_read_list_statuses", { listId })
      .then((s) => { if (!cancelled) setStatuses(s); })
      .catch(() => { if (!cancelled) setStatuses([]); });
    return () => { cancelled = true; };
  }, [detail?.task?.list_id]);

  function handleGeometryChange(next: FloatingGeometry) {
    setGeometry(next);
    invoke("scratchpad_set_geometry", {
      panelId: DETAIL_PANEL_ID,
      geometryJson: JSON.stringify(next),
      opacity: 1,
    }).catch(() => {});
  }

  async function handleStatusChange(statusName: string) {
    if (!taskId) return;
    const field = "status";
    const prevStatus = task?.status_name ?? null;
    setOverlayEntry(setOverlay, taskId, field, statusName);
    try {
      await invoke("clickup_set_task_status", { taskId, statusName });
      clearOverlayEntry(setOverlay, taskId, field);
      // Re-fetch detail so the status chip reflects the new value from the mirror.
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
      setDetail(updated);
    } catch (err) {
      clearOverlayEntry(setOverlay, taskId, field);
      addToast({ message: "Status change failed", description: String(err), type: "error" });
      // Revert: restore the pre-edit value in overlay briefly so the detail
      // re-renders with the original while the detail refetch is in progress.
      if (prevStatus) {
        setOverlayEntry(setOverlay, taskId, field, prevStatus);
        setTimeout(() => clearOverlayEntry(setOverlay, taskId, field), 50);
      }
    }
  }

  async function handleChecklistToggle(checklistId: string, item: ClickUpChecklistItem) {
    if (!taskId) return;
    const field = `checklist:${checklistId}:${item.id}`;
    const newResolved = !item.resolved;
    setOverlayEntry(setOverlay, taskId, field, newResolved ? "true" : "false");
    try {
      await invoke("clickup_set_checklist_item", {
        checklistId,
        itemId: item.id,
        resolved: newResolved,
      });
      clearOverlayEntry(setOverlay, taskId, field);
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
      setDetail(updated);
    } catch (err) {
      clearOverlayEntry(setOverlay, taskId, field);
      addToast({ message: "Checklist update failed", description: String(err), type: "error" });
    }
  }

  async function handleDescSave() {
    if (!taskId) return;
    const field = "description";
    const draft = descDraft.trim();
    setEditingDesc(false);
    setOverlayEntry(setOverlay, taskId, field, draft);
    try {
      await invoke("clickup_update_task", { taskId, description: draft });
      clearOverlayEntry(setOverlay, taskId, field);
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
      setDetail(updated);
    } catch (err) {
      clearOverlayEntry(setOverlay, taskId, field);
      addToast({ message: "Description update failed", description: String(err), type: "error" });
    }
  }

  async function handleDueDateSave() {
    if (!taskId) return;
    const field = "dueDate";
    setEditingDueDate(false);
    const ms = dueDateDraft ? new Date(dueDateDraft).getTime() : null;
    setOverlayEntry(setOverlay, taskId, field, ms !== null ? String(ms) : null);
    try {
      await invoke("clickup_update_task", {
        taskId,
        dueDate: ms !== null ? ms : null,
      });
      clearOverlayEntry(setOverlay, taskId, field);
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
      setDetail(updated);
    } catch (err) {
      clearOverlayEntry(setOverlay, taskId, field);
      addToast({ message: "Due date update failed", description: String(err), type: "error" });
    }
  }

  async function handleRemoveAssignee(assigneeId: number) {
    if (!taskId) return;
    try {
      await invoke("clickup_update_task", { taskId, assigneesRem: [assigneeId] });
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
      setDetail(updated);
    } catch (err) {
      addToast({ message: "Remove assignee failed", description: String(err), type: "error" });
    }
  }

  async function handlePostComment() {
    if (!taskId || !commentDraft.trim() || postingComment) return;
    setPostingComment(true);
    const text = commentDraft.trim();
    try {
      const token = await invoke<string>("clickup_request_closure_token", {
        taskId,
        status: null,
        comment: text,
      });
      const raw = await invoke<Record<string, unknown>>("clickup_execute_closure", { token });
      const commentStatus = (raw["comment"] as Record<string, unknown>)?.["status"];
      if (commentStatus === "posted") {
        addToast({ message: "Comment posted", type: "success" });
        setCommentDraft("");
        const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
        setDetail(updated);
      } else if (commentStatus === "uncertain") {
        setUncertainComment({ text, sentAtMs: Date.now() });
        addToast({
          message: "Comment status unclear",
          description: "Network timeout — verify before retrying.",
          type: "info",
        });
      } else {
        const errMsg = String((raw["comment"] as Record<string, unknown>)?.["error"] ?? "unknown");
        addToast({ message: "Comment failed", description: errMsg, type: "error" });
      }
    } catch (err) {
      addToast({ message: "Comment failed", description: String(err), type: "error" });
    } finally {
      setPostingComment(false);
    }
  }

  async function handleVerifyComment() {
    if (!taskId || !uncertainComment) return;
    try {
      const landed = await invoke<boolean>("clickup_verify_comment_landed", {
        taskId,
        text: uncertainComment.text,
        postedAtMs: uncertainComment.sentAtMs,
      });
      if (landed) {
        addToast({ message: "Comment confirmed landed", type: "success" });
        setUncertainComment(null);
        setCommentDraft("");
        const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId });
        setDetail(updated);
      } else {
        addToast({ message: "Comment not found — safe to retry", type: "info" });
        setUncertainComment(null);
      }
    } catch (err) {
      addToast({ message: "Verify failed", description: String(err), type: "error" });
    }
  }

  const task = detail?.task ?? null;
  const subtasks = taskId ? tasks.filter((t) => t.parent_id === taskId) : [];

  // Apply optimistic overlay to status_name for the status pill.
  const overlayStatusName = taskId
    ? (overlay as Record<string, { value: string | null } | undefined>)[`${taskId}:status`]?.value ?? null
    : null;
  const displayStatusName = overlayStatusName ?? task?.status_name ?? null;
  const displayStatusColor = (() => {
    if (!overlayStatusName || !taskId) return task?.status_color ?? null;
    return statuses.find((s) => s.name === overlayStatusName)?.color ?? task?.status_color ?? null;
  })();

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
      minWidth={380}
      minHeight={260}
      accent
      autoFocus
      title={
        <>
          <span
            className="size-1.5 shrink-0 rounded-full"
            style={{ background: task?.status_color ?? "var(--color-muted-foreground)" }}
          />
          <span className="truncate text-xs font-medium text-foreground">
            {task?.name ?? "ClickUp task"}
          </span>
        </>
      }
      toolbar={
        <>
          {taskId && (
            <>
              <ToolbarAction label={ACTION_LABELS.send} onClick={() => requestSend(taskId)}>
                <Send size={12} />
              </ToolbarAction>
              <ToolbarAction label={ACTION_LABELS.spawn} onClick={() => void spawnWorktree(taskId)}>
                <GitBranchPlus size={12} />
              </ToolbarAction>
              <ToolbarAction
                label={pinnedTaskIds.includes(taskId) ? ACTION_LABELS.unpin : ACTION_LABELS.pin}
                onClick={() => void togglePin(taskId)}
                active={pinnedTaskIds.includes(taskId)}
              >
                {pinnedTaskIds.includes(taskId) ? <PinOff size={12} /> : <Pin size={12} />}
              </ToolbarAction>
              <ToolbarAction
                label={taskId === boundTaskId ? ACTION_LABELS.unbind : ACTION_LABELS.bind}
                onClick={() => void requestBind(taskId)}
                active={taskId === boundTaskId}
              >
                {taskId === boundTaskId ? <Unlink size={12} /> : <Link2 size={12} />}
              </ToolbarAction>
              {/* Only meaningful for tasks already injected into the active
                  session (bound or pinned) — the explicit-refresh path for
                  stale live context (design Decision 7 + risk table). */}
              {(taskId === boundTaskId || pinnedTaskIds.includes(taskId)) && (
                <ToolbarAction
                  label="Re-inject current task content into the live session (explicit refresh — never automatic)"
                  onClick={() => void reinject(taskId)}
                >
                  <RefreshCw size={12} />
                </ToolbarAction>
              )}
              {/* "Close out task" — manual closure verb (Revision 1, task 5.2b).
                  Shown when this task is bound to the active session. No single-
                  letter key (S/W/P/B are taken). */}
              {taskId === boundTaskId && activeSessionId && (
                <ToolbarAction
                  label="Close out task — move status and/or post a comment to mark this task done"
                  onClick={() =>
                    setClosureOffer({ taskId, sessionId: activeSessionId })
                  }
                >
                  <CircleCheck size={12} />
                </ToolbarAction>
              )}
            </>
          )}
          {task?.url && (
            <button
              type="button"
              aria-label="Open in ClickUp"
              title="Open in ClickUp"
              onClick={() => openExternalUrl(task.url)}
              className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground transition-colors"
            >
              <ExternalLink size={12} />
            </button>
          )}
        </>
      }
    >
      <div className="h-full overflow-y-auto">
        {loading && !detail ? (
          <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
            <Loader2 size={14} className="animate-spin" /> Loading…
          </div>
        ) : error ? (
          <div className="flex h-full items-center justify-center px-6 text-center text-xs text-red-400">
            {error}
          </div>
        ) : detail ? (
          <div className="flex flex-col gap-3 px-3 py-2">
            {/* Meta strip */}
            <div className="flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground">
              {/* Status pill — clicking opens the inline status picker */}
              {displayStatusName && (
                <StatusPicker
                  currentStatus={displayStatusName}
                  currentColor={displayStatusColor}
                  statuses={statuses}
                  onSelect={(name) => void handleStatusChange(name)}
                  pending={!!overlayStatusName}
                />
              )}
              {task?.priority && <span>priority: {task.priority}</span>}
              {/* Due date — inline editable */}
              {editingDueDate ? (
                <span className="flex items-center gap-1">
                  <input
                    type="date"
                    value={dueDateDraft}
                    onChange={(e) => setDueDateDraft(e.target.value)}
                    className="h-5 rounded border border-input bg-transparent px-1 text-[10px] outline-none focus-visible:ring-1 focus-visible:ring-ring/50"
                    autoFocus
                  />
                  <button
                    type="button"
                    onClick={() => void handleDueDateSave()}
                    className="rounded px-1 text-[10px] text-green-400 hover:text-green-300"
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    onClick={() => setEditingDueDate(false)}
                    className="rounded px-1 text-[10px] text-muted-foreground hover:text-foreground"
                  >
                    Cancel
                  </button>
                  {dueDateDraft && (
                    <button
                      type="button"
                      onClick={() => { setDueDateDraft(""); void handleDueDateSave(); }}
                      className="rounded px-1 text-[10px] text-muted-foreground hover:text-red-400"
                    >
                      Clear
                    </button>
                  )}
                </span>
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    const ms = task?.due_date;
                    setDueDateDraft(ms ? new Date(ms).toISOString().split("T")[0] : "");
                    setEditingDueDate(true);
                  }}
                  className="hover:text-foreground transition-colors"
                  title="Edit due date"
                >
                  {task?.due_date != null ? `due ${formatDateTime(task.due_date)}` : "set due date"}
                </button>
              )}
              {task && <span className="truncate">{task.list_name}</span>}
              {/* Assignees: show initials avatars + remove button on hover.
                  Add-assignee is scoped to remove-only: the mirror holds only
                  the task's current assignees and there is no workspace members
                  directory in the mirror — add would require a live API call
                  for the member list. Remove-only is correct for this MVP. */}
              {task?.assignees.map((a) => (
                <span
                  key={a.id ?? a.username ?? "?"}
                  className="group relative flex items-center gap-0.5"
                >
                  <span
                    title={a.username ?? undefined}
                    className="flex size-4 items-center justify-center rounded-full text-[8px] font-medium text-white"
                    style={{ background: a.color ?? "var(--color-secondary)" }}
                  >
                    {a.initials ?? (a.username?.slice(0, 2).toUpperCase() ?? "?")}
                  </span>
                  {a.id !== null && (
                    <button
                      type="button"
                      title={`Remove ${a.username ?? "assignee"}`}
                      onClick={() => void handleRemoveAssignee(a.id!)}
                      className="hidden group-hover:flex size-3 items-center justify-center rounded-full bg-secondary/80 text-muted-foreground hover:text-red-400"
                    >
                      <UserMinus size={8} />
                    </button>
                  )}
                </span>
              ))}
              {task?.tags.map((tag) => (
                <span
                  key={tag.name}
                  className="rounded-full px-1.5 leading-4"
                  style={{
                    background: tag.tag_bg ? `${tag.tag_bg}33` : "var(--color-secondary)",
                    color: tag.tag_fg ?? tag.tag_bg ?? "var(--color-secondary-foreground)",
                  }}
                >
                  {tag.name}
                </span>
              ))}
            </div>

            {/* Description — untrusted multi-writer markdown: rendered through
                the same sanitizing pipeline as vault note bodies (MarkdownView
                / react-markdown skips raw HTML; never raw-HTML passthrough). */}
            <div className="flex items-center gap-2">
              <SectionCaps label="Description" />
              {!editingDesc && (
                <button
                  type="button"
                  title="Edit description"
                  onClick={() => {
                    setDescDraft(detail.description ?? "");
                    setEditingDesc(true);
                  }}
                  className="flex size-3.5 items-center justify-center rounded text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Pencil size={10} />
                </button>
              )}
            </div>
            {editingDesc ? (
              <div className="flex flex-col gap-1.5">
                <textarea
                  value={descDraft}
                  onChange={(e) => setDescDraft(e.target.value)}
                  className="min-h-[80px] rounded border border-input bg-secondary/30 p-2 text-[11px] leading-relaxed text-foreground/90 outline-none focus-visible:ring-1 focus-visible:ring-ring/50 resize-y"
                  autoFocus
                />
                <div className="flex gap-1.5">
                  <button
                    type="button"
                    onClick={() => void handleDescSave()}
                    className="rounded bg-secondary/60 px-2 py-0.5 text-[10px] text-green-400 hover:text-green-300 transition-colors"
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    onClick={() => setEditingDesc(false)}
                    className="rounded px-2 py-0.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            ) : detail.description ? (
              <div className="-mx-3 -my-2 rounded">
                <MarkdownView content={detail.description} />
              </div>
            ) : (
              <p className="text-xs text-muted-foreground">No description</p>
            )}

            {detail.custom_values.length > 0 && (
              <>
                <SectionCaps label="Fields" />
                <div className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
                  {detail.custom_values.map((cv) => {
                    const rendered = formatCustomValue(cv);
                    if (rendered === null) return null;
                    return (
                      <div key={cv.field_id} className="contents">
                        <span className="text-muted-foreground">{cv.name}</span>
                        <span className="min-w-0 truncate text-foreground/80" title={rendered}>
                          {rendered}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </>
            )}

            {subtasks.length > 0 && (
              <>
                <SectionCaps label={`Subtasks · ${subtasks.length}`} />
                <div className="flex flex-col">
                  {subtasks.map((sub) => (
                    <button
                      key={sub.id}
                      type="button"
                      onClick={() => setTaskId(sub.id)}
                      className="flex items-center gap-1.5 rounded px-1 py-0.5 text-left text-[11px] text-foreground/80 transition-colors hover:bg-secondary/40"
                    >
                      <span
                        className="size-1.5 shrink-0 rounded-full"
                        style={{ background: sub.status_color ?? "var(--color-muted-foreground)" }}
                      />
                      <span className="truncate">{sub.name}</span>
                    </button>
                  ))}
                </div>
              </>
            )}

            {detail.checklists.length > 0 && (
              <>
                <SectionCaps label="Checklists" />
                {detail.checklists.map((cl) => (
                  <div key={cl.id} className="flex flex-col gap-0.5">
                    {cl.name && <p className="text-[11px] font-medium text-foreground/80">{cl.name}</p>}
                    {cl.items.map((item) => {
                      // Apply optimistic overlay for this checklist item.
                      const overlayKey = taskId ? `${taskId}:checklist:${cl.id}:${item.id}` : null;
                      const overlayVal = overlayKey ? overlay[overlayKey as `${string}:${string}`]?.value : undefined;
                      const resolved = overlayVal !== undefined
                        ? overlayVal === "true"
                        : item.resolved;
                      const pending = overlayVal !== undefined;
                      return (
                        <button
                          key={item.id}
                          type="button"
                          onClick={() => void handleChecklistToggle(cl.id, item)}
                          className={`flex items-center gap-1.5 rounded pl-1 py-0.5 text-[11px] text-left transition-colors hover:bg-secondary/40 ${pending ? "opacity-70" : ""}`}
                        >
                          {resolved ? (
                            <CheckSquare size={11} className="shrink-0 text-green-500" />
                          ) : (
                            <Square size={11} className="shrink-0 text-muted-foreground" />
                          )}
                          <span className={resolved ? "text-muted-foreground line-through" : "text-foreground/80"}>
                            {item.name}
                          </span>
                          {pending && <Loader2 size={9} className="ml-auto shrink-0 animate-spin text-muted-foreground" />}
                        </button>
                      );
                    })}
                  </div>
                ))}
              </>
            )}

            {detail.attachments.length > 0 && (
              <>
                <SectionCaps label="Attachments" />
                <div className="flex flex-wrap gap-1.5">
                  {detail.attachments.map((att) => (
                    <AttachmentChip key={att.id} attachment={att} />
                  ))}
                </div>
              </>
            )}

            <SectionCaps label={`Comments · ${detail.comments.length}`} />
            {detail.comments.length === 0 ? (
              <p className="text-xs text-muted-foreground">No comments</p>
            ) : (
              <div className="flex flex-col gap-2">
                {detail.comments.map((comment) => (
                  <div key={comment.id} className="rounded border border-border/50 bg-secondary/20">
                    <div className="flex items-center gap-1.5 px-2 pt-1.5 text-[10px] text-muted-foreground">
                      <span
                        className="flex size-4 items-center justify-center rounded-full text-[8px] font-medium text-white"
                        style={{ background: comment.user?.color ?? "var(--color-secondary)" }}
                      >
                        {comment.user?.initials ?? (comment.user?.username?.slice(0, 2).toUpperCase() ?? "?")}
                      </span>
                      <span className="font-medium text-foreground/70">
                        {comment.user?.username ?? "Unknown"}
                      </span>
                      {comment.date != null && <span>{formatDateTime(comment.date)}</span>}
                      {comment.resolved && <span className="text-green-500">resolved</span>}
                    </div>
                    {/* Same sanitizing markdown path as the description. */}
                    <div className="-my-1.5">
                      <MarkdownView content={comment.text ?? ""} />
                    </div>
                  </div>
                ))}
              </div>
            )}

            {/* Comment composer — token-gated via closure token (Decision 5) */}
            <div className="flex flex-col gap-1.5 pt-1 border-t border-border/40">
              {uncertainComment && (
                <div className="rounded border border-yellow-500/30 bg-yellow-500/10 px-2 py-1 text-[10px] text-yellow-300">
                  Status unclear — verify before retrying.{" "}
                  <button
                    type="button"
                    onClick={() => void handleVerifyComment()}
                    className="underline hover:no-underline"
                  >
                    Check if it landed
                  </button>
                </div>
              )}
              <textarea
                value={commentDraft}
                onChange={(e) => setCommentDraft(e.target.value)}
                placeholder="Add a comment…"
                rows={2}
                className="rounded border border-input bg-secondary/30 p-1.5 text-[11px] leading-relaxed text-foreground/90 outline-none focus-visible:ring-1 focus-visible:ring-ring/50 resize-none"
                disabled={postingComment}
              />
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handlePostComment()}
                  disabled={!commentDraft.trim() || postingComment}
                  className="flex items-center gap-1 rounded bg-secondary px-2 py-0.5 text-[10px] text-foreground/80 hover:bg-secondary/80 disabled:opacity-40 transition-colors"
                >
                  {postingComment ? (
                    <><Loader2 size={9} className="animate-spin" /> Posting…</>
                  ) : (
                    <><Send size={9} /> Comment</>
                  )}
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </FloatingPanel>
    </div>
  );
}

function SectionCaps({ label }: { label: string }) {
  return (
    <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
      {label}
    </span>
  );
}

/// Inline status pill that expands to a mini-picker on click.
/// Uses a local open/close state; clicking a status option calls onSelect
/// and immediately closes, so the optimistic overlay takes effect at once.
function StatusPicker({
  currentStatus,
  currentColor,
  statuses,
  onSelect,
  pending,
}: {
  currentStatus: string;
  currentColor: string | null;
  statuses: ClickUpListStatus[];
  onSelect: (name: string) => void;
  pending: boolean;
}) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onOutside);
    return () => document.removeEventListener("mousedown", onOutside);
  }, [open]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-1 rounded-full px-2 leading-4 transition-colors hover:opacity-80 ${pending ? "opacity-60" : ""}`}
        style={{
          background: currentColor ? `${currentColor}26` : "var(--color-secondary)",
          color: currentColor ?? "var(--color-secondary-foreground)",
        }}
      >
        {currentStatus}
        {pending && <Loader2 size={8} className="animate-spin" />}
      </button>
      {open && statuses.length > 0 && (
        <div className="absolute left-0 top-full z-50 mt-1 min-w-[140px] rounded border border-border bg-card py-1 shadow-lg">
          {statuses.map((s) => (
            <button
              key={s.name}
              type="button"
              onClick={() => {
                setOpen(false);
                onSelect(s.name);
              }}
              className={`flex w-full items-center gap-1.5 px-2.5 py-1 text-left text-[10px] transition-colors hover:bg-secondary/60 ${
                s.name === currentStatus ? "text-foreground" : "text-muted-foreground"
              }`}
            >
              {s.color && (
                <span className="size-1.5 shrink-0 rounded-full" style={{ background: s.color }} />
              )}
              {s.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function AttachmentChip({ attachment }: { attachment: ClickUpAttachment }) {
  const image = isImageAttachment(attachment);
  // Click-through is the explicit user action that opens the original in the
  // browser; nothing is fetched or stored locally besides the lazy thumbnail.
  function openOriginal() {
    openExternalUrl(attachment.url);
  }

  return (
    <button
      type="button"
      onClick={openOriginal}
      disabled={!attachment.url}
      title={attachment.title ?? undefined}
      className="flex max-w-44 flex-col gap-1 rounded border border-border/60 bg-secondary/30 p-1.5 text-left transition-colors hover:border-border hover:bg-secondary/60 disabled:opacity-50"
    >
      {image && attachment.thumbnail_url && /^https?:\/\//i.test(attachment.thumbnail_url) && (
        <img
          src={attachment.thumbnail_url}
          alt={attachment.title ?? "attachment"}
          loading="lazy"
          className="max-h-24 w-full rounded object-cover"
        />
      )}
      <span className="flex items-center gap-1 text-[10px] text-foreground/80">
        <Paperclip size={10} className="shrink-0 text-muted-foreground" />
        <span className="truncate">{attachment.title ?? "attachment"}</span>
        {attachment.size != null && (
          <span className="shrink-0 text-muted-foreground">{formatBytes(attachment.size)}</span>
        )}
      </span>
    </button>
  );
}
