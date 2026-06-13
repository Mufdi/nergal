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
import { DatePopover } from "@/components/clickup/DatePopover";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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

/// True when two due-date timestamps fall on the same local calendar day
/// (both null counts as equal). Used to confirm a due-date write past
/// ClickUp's date normalization.
function sameDueDay(a: number | null, b: number | null): boolean {
  if (a === null || b === null) return a === b;
  const da = new Date(a);
  const db = new Date(b);
  return (
    da.getFullYear() === db.getFullYear() &&
    da.getMonth() === db.getMonth() &&
    da.getDate() === db.getDate()
  );
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

/// Toolbar button with an instant tooltip (delay-0, TopBar pattern) instead of
/// the OS-delayed native `title`.
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
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={label}
            onClick={onClick}
            className={`flex size-5 items-center justify-center rounded transition-colors ${
              active
                ? "text-primary hover:bg-secondary/60"
                : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
            }`}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
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
  const [statusesLoading, setStatusesLoading] = useState(false);
  const [editingDesc, setEditingDesc] = useState(false);
  const [descDraft, setDescDraft] = useState<string>("");
  const [statusPickerOpen, setStatusPickerOpen] = useState(false);
  const [dueOpen, setDueOpen] = useState(false);
  const [commentDraft, setCommentDraft] = useState<string>("");
  const [postingComment, setPostingComment] = useState(false);
  const [uncertainComment, setUncertainComment] = useState<{ text: string; sentAtMs: number } | null>(null);
  // Single-flight lock: the field name of the in-flight write, or null. Blocks
  // every write control until the previous one resolves (the user reported
  // changes "appearing to do nothing then landing" when fired back-to-back).
  const [busy, setBusy] = useState<string | null>(null);
  const commentRef = useRef<HTMLTextAreaElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  // Index cursor key over the body's actionable elements (AgentPickerModal
  // pattern). Highlight is a bg/ring on the selected element — no DOM focus.
  const [navKey, setNavKey] = useState<string | null>(null);

  // Contextual task verbs: bare letters scoped to ClickUp surfaces (same
  // convention as ConflictsPanel O/T, PrViewer A). Inside the floating
  // detail the open task wins; inside the right panel the data-nav-selected
  // cursor row wins (panel rows are never DOM-focused — patterns.md §5.2).
  // The handler lives here — the detail is the always-mounted ClickUp
  // surface, so the keys keep working when the chip opens the detail
  // without the right panel.
  useEffect(() => {
    const VERB_KEYS = ["KeyS", "KeyW", "KeyP", "KeyB", "KeyR", "KeyC", "KeyO"];
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
      // S/W/P/B always apply; R/C/O are conditional (reinject needs a live
      // injection target, close-out needs the bound task + a session, open
      // needs a url) — only swallow the key when we actually act.
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
        const url = tasks.find((t) => t.id === id)?.url ?? (detail?.task?.id === id ? detail.task?.url : null);
        if (url) { e.preventDefault(); openExternalUrl(url); }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [taskId, requestSend, spawnWorktree, togglePin, requestBind, reinject, boundTaskId, pinnedTaskIds, activeSessionId, setClosureOffer, tasks, detail]);

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

  // Reset the cursor and grab focus on the nav container when a task opens
  // (or when drilling into a subtask) — the container holds the single focus,
  // arrows move the highlight (no per-element DOM focus).
  useEffect(() => {
    setNavKey(null);
    if (taskId === null) return;
    const t = setTimeout(() => contentRef.current?.focus({ preventScroll: true }), 60);
    return () => clearTimeout(t);
  }, [taskId]);

  // When a popup (status / date) closes, hand focus back to the nav container
  // so arrow-nav resumes from the cursor.
  useEffect(() => {
    if (taskId === null || statusPickerOpen || dueOpen) return;
    contentRef.current?.focus({ preventScroll: true });
  }, [statusPickerOpen, dueOpen, taskId]);

  // The cursor is state-driven (no DOM focus), so the scroll container won't
  // follow it on its own — bring the highlighted element into view.
  useEffect(() => {
    if (!navKey) return;
    contentRef.current
      ?.querySelector<HTMLElement>(`[data-nav-key="${CSS.escape(navKey)}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [navKey]);

  useEffect(() => {
    if (!taskId) {
      setDetail(null);
      setError(null);
      setStatuses([]);
      setEditingDesc(false);
      setDueOpen(false);
      setStatusPickerOpen(false);
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
    setStatusesLoading(true);
    invoke<ClickUpListStatus[]>("clickup_read_list_statuses", { listId })
      .then((s) => { if (!cancelled) setStatuses(s); })
      .catch(() => { if (!cancelled) setStatuses([]); })
      .finally(() => { if (!cancelled) setStatusesLoading(false); });
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

  // After a write lands, ClickUp has read-after-write lag: an immediate GET can
  // still return the pre-write value. Re-fetch a few times until the server
  // reflects our optimistic value, then drop the overlay. Clearing the overlay
  // on the first (stale) read was the source of the toggle/value flicker; the
  // overlay stays applied across the loop so the display never bounces back.
  async function confirmAfterWrite(
    id: string,
    field: string,
    matches: (d: ClickUpTaskDetailData) => boolean,
  ) {
    for (let attempt = 0; attempt < 5; attempt++) {
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId: id });
      setDetail(updated);
      if (matches(updated)) break;
      if (attempt < 4) await new Promise((r) => setTimeout(r, 800));
    }
    clearOverlayEntry(setOverlay, id, field);
  }

  async function handleStatusChange(statusName: string) {
    if (!taskId || busy) return;
    const field = "status";
    const prevStatus = task?.status_name ?? null;
    setBusy(field);
    setOverlayEntry(setOverlay, taskId, field, statusName);
    const id = taskId;
    try {
      await invoke("clickup_set_task_status", { taskId: id, statusName });
      await confirmAfterWrite(id, field, (d) => d.task?.status_name === statusName);
    } catch (err) {
      clearOverlayEntry(setOverlay, id, field);
      addToast({ message: "Status change failed", description: String(err), type: "error" });
      if (prevStatus) {
        setOverlayEntry(setOverlay, id, field, prevStatus);
        setTimeout(() => clearOverlayEntry(setOverlay, id, field), 50);
      }
    } finally {
      setBusy(null);
    }
  }

  async function handleChecklistToggle(checklistId: string, item: ClickUpChecklistItem) {
    if (!taskId || busy) return;
    const field = `checklist:${checklistId}:${item.id}`;
    const newResolved = !item.resolved;
    const id = taskId;
    setBusy(field);
    setOverlayEntry(setOverlay, id, field, newResolved ? "true" : "false");
    try {
      await invoke("clickup_set_checklist_item", { checklistId, itemId: item.id, resolved: newResolved });
      await confirmAfterWrite(id, field, (d) =>
        d.checklists.find((cl) => cl.id === checklistId)?.items.find((it) => it.id === item.id)?.resolved === newResolved,
      );
    } catch (err) {
      clearOverlayEntry(setOverlay, id, field);
      addToast({ message: "Checklist update failed", description: String(err), type: "error" });
    } finally {
      setBusy(null);
    }
  }

  async function handleDescSave() {
    if (!taskId || busy) return;
    const field = "description";
    const draft = descDraft.trim();
    const id = taskId;
    setEditingDesc(false);
    setBusy(field);
    setOverlayEntry(setOverlay, id, field, draft);
    try {
      await invoke("clickup_update_task", { taskId: id, description: draft });
      await confirmAfterWrite(id, field, (d) => (d.description ?? "") === draft);
    } catch (err) {
      clearOverlayEntry(setOverlay, id, field);
      addToast({ message: "Description update failed", description: String(err), type: "error" });
    } finally {
      setBusy(null);
    }
  }

  /// `ms` is local-noon for the picked calendar date (or null to clear) —
  /// see DatePopover. The match is by local calendar day, since ClickUp
  /// normalizes a date-only due date to its own canonical time.
  async function handleDueDateSave(ms: number | null) {
    if (!taskId || busy) return;
    const field = "dueDate";
    const id = taskId;
    setDueOpen(false);
    setBusy(field);
    setOverlayEntry(setOverlay, id, field, ms !== null ? String(ms) : null);
    try {
      await invoke("clickup_update_task", { taskId: id, dueDate: ms });
      await confirmAfterWrite(id, field, (d) => sameDueDay(d.task?.due_date ?? null, ms));
    } catch (err) {
      clearOverlayEntry(setOverlay, id, field);
      addToast({ message: "Due date update failed", description: String(err), type: "error" });
    } finally {
      setBusy(null);
    }
  }

  async function handleRemoveAssignee(assigneeId: number) {
    if (!taskId || busy) return;
    const id = taskId;
    setBusy("assignee");
    try {
      await invoke("clickup_update_task", { taskId: id, assigneesRem: [assigneeId] });
      const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId: id });
      setDetail(updated);
    } catch (err) {
      addToast({ message: "Remove assignee failed", description: String(err), type: "error" });
    } finally {
      setBusy(null);
    }
  }

  async function handlePostComment() {
    if (!taskId || !commentDraft.trim() || busy || postingComment) return;
    const id = taskId;
    setPostingComment(true);
    setBusy("comment");
    const text = commentDraft.trim();
    try {
      const token = await invoke<string>("clickup_request_closure_token", { taskId: id, status: null, comment: text });
      const raw = await invoke<Record<string, unknown>>("clickup_execute_closure", { token });
      const commentStatus = (raw["comment"] as Record<string, unknown>)?.["status"];
      if (commentStatus === "posted") {
        // Hold the spinner until the comment actually shows (ClickUp lag), so
        // it never appears to "save empty" then pop in a moment later.
        for (let attempt = 0; attempt < 5; attempt++) {
          const updated = await invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId: id });
          setDetail(updated);
          if (updated.comments.some((c) => (c.text ?? "").includes(text))) break;
          if (attempt < 4) await new Promise((r) => setTimeout(r, 800));
        }
        setCommentDraft("");
        addToast({ message: "Comment posted", type: "success" });
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
      setBusy(null);
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

  // Optimistic due-date overlay (mirrors the status overlay) so the display
  // holds the picked value while the confirm loop reconciles ClickUp's lag.
  const overlayDue = taskId
    ? (overlay as Record<string, { value: string | null } | undefined>)[`${taskId}:dueDate`]
    : undefined;
  const displayDueMs =
    overlayDue !== undefined
      ? overlayDue.value !== null
        ? Number(overlayDue.value)
        : null
      : task?.due_date ?? null;

  // Index cursor over the body's actionable elements — the new-session modal
  // pattern (AgentPickerModal): one focus on the content container, ↑/↓ move a
  // `data-nav-selected` highlight (bg via the global focus-zone rule, no
  // per-element ring), Enter/Space activate. The DOM order of `[data-nav-key]`
  // nodes is the source of truth, filtered to visible ones. An open popup
  // (status / date) owns the arrows.
  function activateNav(key: string) {
    if (busy) return;
    if (key === "status") setStatusPickerOpen(true);
    else if (key === "due") setDueOpen(true);
    else if (key.startsWith("sub:")) setTaskId(key.slice(4));
    else if (key.startsWith("check:")) {
      const [, clId, itemId] = key.split(":");
      const cl = detail?.checklists.find((c) => c.id === clId);
      const item = cl?.items.find((it) => it.id === itemId);
      if (cl && item) void handleChecklistToggle(cl.id, item);
    } else if (key === "comment") commentRef.current?.focus();
  }

  function navKeysInOrder(): string[] {
    const content = contentRef.current;
    if (!content) return [];
    return Array.from(content.querySelectorAll<HTMLElement>("[data-nav-key]"))
      .filter((el) => el.offsetParent !== null)
      .map((el) => el.dataset.navKey ?? "")
      .filter(Boolean);
  }

  function handleNavKeyDown(e: React.KeyboardEvent) {
    if (statusPickerOpen || dueOpen) return;
    const target = e.target as HTMLElement | null;
    if (target?.tagName === "TEXTAREA" || target?.tagName === "INPUT") {
      // Let text fields own their keys; only Escape backs out to the cursor.
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const keys = navKeysInOrder();
      if (keys.length === 0) return;
      e.preventDefault();
      e.stopPropagation();
      const idx = navKey ? keys.indexOf(navKey) : -1;
      const next =
        e.key === "ArrowDown" ? Math.min(idx + 1, keys.length - 1) : Math.max(idx - 1, 0);
      setNavKey(keys[idx === -1 ? 0 : next] ?? keys[0]);
    } else if ((e.key === "Enter" || e.key === " ") && navKey) {
      e.preventDefault();
      e.stopPropagation();
      activateNav(navKey);
    }
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
        <TooltipProvider delay={0}>
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
                  label="Re-inject current task content into the live session (R) — explicit refresh, never automatic"
                  onClick={() => void reinject(taskId)}
                >
                  <RefreshCw size={12} />
                </ToolbarAction>
              )}
              {/* "Close out task" — manual closure verb (Revision 1, task 5.2b).
                  Shown when this task is bound to the active session. */}
              {taskId === boundTaskId && activeSessionId && (
                <ToolbarAction
                  label="Close out task (C) — move status and/or post a comment to mark this task done"
                  onClick={() => setClosureOffer({ taskId, sessionId: activeSessionId })}
                >
                  <CircleCheck size={12} />
                </ToolbarAction>
              )}
            </>
          )}
          {task?.url && (
            <ToolbarAction label="Open in ClickUp (O)" onClick={() => openExternalUrl(task.url)}>
              <ExternalLink size={12} />
            </ToolbarAction>
          )}
        </TooltipProvider>
      }
    >
      <div
        ref={contentRef}
        tabIndex={0}
        onKeyDown={handleNavKeyDown}
        onClick={(e) => {
          // Clicking an actionable element moves the cursor there; refocus the
          // container (WebKitGTK doesn't focus buttons on click) so arrow-nav
          // resumes — except when clicking into the comment field.
          const target = e.target as HTMLElement | null;
          const el = target?.closest<HTMLElement>("[data-nav-key]");
          if (el?.dataset.navKey) setNavKey(el.dataset.navKey);
          if (target && target.tagName !== "TEXTAREA" && target.tagName !== "INPUT") {
            contentRef.current?.focus({ preventScroll: true });
          }
        }}
        className="h-full overflow-y-auto outline-none"
      >
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
                  loading={statusesLoading}
                  onSelect={(name) => void handleStatusChange(name)}
                  pending={!!overlayStatusName || busy === "status"}
                  open={statusPickerOpen}
                  onOpenChange={setStatusPickerOpen}
                  navSelected={navKey === "status"}
                />
              )}
              {task?.priority && <span>priority: {task.priority}</span>}
              {/* Due date — local-TZ calendar popover (date-only). */}
              <span className="flex items-center gap-1">
                <DatePopover
                  valueMs={displayDueMs}
                  onSelect={(ms) => void handleDueDateSave(ms)}
                  open={dueOpen}
                  onOpenChange={setDueOpen}
                  disabled={busy !== null}
                  navSelected={navKey === "due"}
                />
                {busy === "dueDate" && <Loader2 size={9} className="animate-spin text-muted-foreground" />}
              </span>
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
                  className="flex size-3.5 items-center justify-center rounded text-muted-foreground outline-none hover:text-foreground transition-colors"
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
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && e.ctrlKey) {
                      e.preventDefault();
                      void handleDescSave();
                    }
                  }}
                  className="min-h-[80px] rounded border border-input bg-secondary/30 p-2 text-[11px] leading-relaxed text-foreground/90 outline-none focus-visible:ring-1 focus-visible:ring-ring/50 resize-y"
                  autoFocus
                />
                <div className="flex gap-1.5">
                  <button
                    type="button"
                    onClick={() => void handleDescSave()}
                    className="rounded bg-secondary/60 px-2 py-0.5 text-[10px] text-green-400 hover:text-green-300 transition-colors"
                  >
                    Save (Ctrl+Enter)
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
                      data-nav-key={`sub:${sub.id}`}
                      data-nav-selected={navKey === `sub:${sub.id}` || undefined}
                      className="flex items-center gap-1.5 rounded px-1 py-0.5 text-left text-[11px] text-foreground/80 outline-none transition-colors hover:bg-secondary/40"
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
                          disabled={busy !== null && !pending}
                          data-nav-key={`check:${cl.id}:${item.id}`}
                          data-nav-selected={navKey === `check:${cl.id}:${item.id}` || undefined}
                          className={`flex items-center gap-1.5 rounded pl-1 py-0.5 text-[11px] text-left outline-none transition-colors hover:bg-secondary/40 disabled:opacity-50 ${pending ? "opacity-70" : ""}`}
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
                ref={commentRef}
                value={commentDraft}
                onChange={(e) => setCommentDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && e.ctrlKey) {
                    e.preventDefault();
                    void handlePostComment();
                  } else if (e.key === "Escape") {
                    // Back out to the cursor so arrow-nav resumes.
                    e.preventDefault();
                    e.stopPropagation();
                    contentRef.current?.focus();
                  }
                }}
                data-nav-key="comment"
                placeholder="Add a comment… (Ctrl+Enter to post)"
                rows={2}
                className={`rounded border bg-secondary/30 p-1.5 text-[11px] leading-relaxed text-foreground/90 outline-none focus-visible:ring-1 focus-visible:ring-ring/50 resize-none ${navKey === "comment" ? "border-foreground/40" : "border-input"}`}
                disabled={postingComment || (busy !== null && busy !== "comment")}
              />
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handlePostComment()}
                  disabled={!commentDraft.trim() || postingComment || (busy !== null && busy !== "comment")}
                  className="flex items-center gap-1 rounded bg-secondary px-2 py-0.5 text-[10px] text-foreground/80 hover:bg-secondary/80 disabled:opacity-40 transition-colors"
                >
                  {postingComment ? (
                    <><Loader2 size={9} className="animate-spin" /> Posting…</>
                  ) : (
                    <><Send size={9} /> Comment (Ctrl+Enter)</>
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

/// Inline status pill that expands to a mini-picker. Controlled open state so
/// the detail's Enter can open it; the open dropdown owns ↑/↓ (cycle options,
/// wraparound) + Enter (select) + Esc (close) — the annotation quick-actions
/// pattern. Selecting refocuses the trigger so arrow-nav resumes.
function StatusPicker({
  currentStatus,
  currentColor,
  statuses,
  loading,
  onSelect,
  pending,
  open,
  onOpenChange,
  navSelected,
}: {
  currentStatus: string;
  currentColor: string | null;
  statuses: ClickUpListStatus[];
  loading: boolean;
  onSelect: (name: string) => void;
  pending: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  navSelected: boolean;
}) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  function choose(name: string) {
    onOpenChange(false);
    onSelect(name);
  }

  // Seed the highlight on the current status each time the dropdown opens.
  useEffect(() => {
    if (!open) return;
    const cur = statuses.findIndex((s) => s.name === currentStatus);
    setActiveIdx(cur >= 0 ? cur : 0);
  }, [open, statuses, currentStatus]);

  useEffect(() => {
    if (!open) return;
    function onOutside(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) onOpenChange(false);
    }
    function onKey(e: KeyboardEvent) {
      if (statuses.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p + 1) % statuses.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setActiveIdx((p) => (p - 1 + statuses.length) % statuses.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        const s = statuses[activeIdx];
        if (s) choose(s.name);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        onOpenChange(false);
      }
    }
    document.addEventListener("mousedown", onOutside);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onOutside);
      window.removeEventListener("keydown", onKey, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, statuses, activeIdx]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        data-nav-key="status"
        onClick={() => onOpenChange(!open)}
        className={`flex items-center gap-1 rounded-full px-2 leading-4 outline-none transition-colors hover:opacity-80 ${navSelected ? "ring-1 ring-foreground/50" : ""} ${pending ? "opacity-60" : ""}`}
        style={{
          background: currentColor ? `${currentColor}26` : "var(--color-secondary)",
          color: currentColor ?? "var(--color-secondary-foreground)",
        }}
      >
        {currentStatus}
        {pending && <Loader2 size={8} className="animate-spin" />}
      </button>
      {open && (
        <div
          data-floating-popup
          className="absolute left-0 top-full z-50 mt-1 min-w-[140px] rounded border border-border bg-card py-1 shadow-lg"
        >
          {loading ? (
            <div className="flex items-center gap-1.5 px-2.5 py-1 text-[10px] text-muted-foreground">
              <Loader2 size={10} className="animate-spin" /> Loading statuses…
            </div>
          ) : statuses.length === 0 ? (
            <div className="px-2.5 py-1 text-[10px] text-muted-foreground">No statuses available.</div>
          ) : (
            statuses.map((s, i) => (
            <button
              key={s.name}
              type="button"
              data-nav-selected={i === activeIdx || undefined}
              onMouseEnter={() => setActiveIdx(i)}
              onClick={() => choose(s.name)}
              className={`flex w-full items-center gap-1.5 px-2.5 py-1 text-left text-[10px] transition-colors data-[nav-selected=true]:bg-secondary/60 ${
                s.name === currentStatus ? "text-foreground" : "text-muted-foreground"
              }`}
            >
              {s.color && (
                <span className="size-1.5 shrink-0 rounded-full" style={{ background: s.color }} />
              )}
              {s.name}
            </button>
            ))
          )}
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
