import { useEffect, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import {
  CheckSquare,
  ChevronLeft,
  ChevronRight,
  CircleCheck,
  Copy,
  ExternalLink,
  GitBranchPlus,
  Link2,
  List,
  PanelRight,
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
import { MarkdownView } from "@/components/plan/MarkdownView";
import { DatePopover } from "@/components/clickup/DatePopover";
import { StatusIcon } from "@/components/clickup/StatusIcon";
import { StatusPicker } from "@/components/clickup/StatusPicker";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { PulseDots } from "@/components/ui/PulseDots";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { activeSessionIdAtom } from "@/stores/workspace";
import {
  activeSessionClickUpPinsAtom,
  activeSessionClickUpTaskAtom,
  clickupClosureOfferAtom,
  clickupListStatusesAtom,
  clickupOverlayAtom,
  clickupTasksAtom,
  clearOverlayEntry,
  copyTaskIdAction,
  reinjectTaskAction,
  requestBindTaskAction,
  requestSendTaskAction,
  setOverlayEntry,
  spawnWorktreeWithTaskAction,
  statusFraction,
  togglePinTaskAction,
  CLICKUP_ACTION_LABELS as ACTION_LABELS,
  type ClickUpAttachment,
  type ClickUpChecklistItem,
  type ClickUpCustomValue,
  type ClickUpListStatus,
  type ClickUpTask,
  type ClickUpTaskDetailData,
} from "@/stores/clickup";
import { toastsAtom } from "@/stores/toast";

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
/// the OS-delayed native `title`. Caller wraps with a single TooltipProvider.
export function ToolbarAction({
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

/// Stale-while-revalidate cache for fetched task details, keyed by task id.
/// Module-level so it survives a tab/modal unmount — revisiting a task renders
/// instantly from cache (no spinner, no reload flash) while a silent background
/// fetch keeps it fresh. Plain Map (no reactivity needed); kept fresh by the
/// controller's detail→cache sync effect, so optimistic writes persist too.
const detailCache = new Map<string, ClickUpTaskDetailData>();

export type TaskController = ReturnType<typeof useClickUpTaskController>;

/// All task-detail state, fetching, optimistic writes, drill-in history and the
/// index-cursor keyboard nav — shared by the floating modal (atom-driven
/// taskId) and the document tab (local-state taskId). Surface-specific pieces
/// (FloatingPanel geometry + close-focus for the modal, focus-scoped verb /
/// Ctrl-arrow window listeners) stay with each consumer.
export function useClickUpTaskController({
  taskId,
  setTaskId,
}: {
  taskId: string | null;
  setTaskId: (id: string | null) => void;
}) {
  const tasks = useAtomValue(clickupTasksAtom);
  const listStatuses = useAtomValue(clickupListStatusesAtom);
  const copyTaskId = useSetAtom(copyTaskIdAction);
  const [overlay, setOverlay] = useAtom(clickupOverlayAtom);
  const addToast = useSetAtom(toastsAtom);
  const [detail, setDetail] = useState<ClickUpTaskDetailData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Write-control local state
  const [statuses, setStatuses] = useState<ClickUpListStatus[]>([]);
  const [statusesLoading, setStatusesLoading] = useState(false);
  const [editingDesc, setEditingDesc] = useState(false);
  const [descDraft, setDescDraft] = useState<string>("");
  const [statusPickerOpen, setStatusPickerOpen] = useState(false);
  const [dueOpen, setDueOpen] = useState(false);
  const [commentDraft, setCommentDraft] = useState<string>("");
  const [commentFocused, setCommentFocused] = useState(false);
  const [postingComment, setPostingComment] = useState(false);
  const [uncertainComment, setUncertainComment] = useState<{ text: string; sentAtMs: number } | null>(null);
  // Single-flight lock: the field name of the in-flight write, or null. Blocks
  // every write control until the previous one resolves.
  const [busy, setBusy] = useState<string | null>(null);
  const commentRef = useRef<HTMLTextAreaElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const mainRef = useRef<HTMLDivElement | null>(null);
  // Drill-in history: clicking a subtask navigates into it; ‹ › step the stack.
  const detailHistory = useRef<{ stack: string[]; index: number }>({ stack: [], index: -1 });
  const navInternal = useRef(false);
  const [histNav, setHistNav] = useState({ canBack: false, canFwd: false });
  // Index cursor key over the body's actionable elements (AgentPickerModal
  // pattern). Highlight is a bg/ring on the selected element — no DOM focus.
  const [navKey, setNavKey] = useState<string | null>(null);

  // Reset the cursor and grab focus on the nav container when a task opens (or
  // when drilling into a subtask) — the container holds the single focus,
  // arrows move the highlight (no per-element DOM focus).
  useEffect(() => {
    setNavKey(null);
    if (taskId === null) return;
    const t = setTimeout(() => contentRef.current?.focus({ preventScroll: true }), 60);
    return () => clearTimeout(t);
  }, [taskId]);

  // Maintain the drill-in history stack. A fresh open (closed → id) seeds it; a
  // drill (new id while open) truncates any forward entries and pushes; a
  // back/forward step (navInternal) just updates the enabled flags.
  useEffect(() => {
    const h = detailHistory.current;
    if (taskId === null) {
      detailHistory.current = { stack: [], index: -1 };
      setHistNav({ canBack: false, canFwd: false });
      return;
    }
    if (navInternal.current) {
      navInternal.current = false;
    } else if (h.index >= 0 && h.stack[h.index] === taskId) {
      // already current — no-op
    } else if (h.index === -1) {
      h.stack = [taskId];
      h.index = 0;
    } else {
      h.stack = [...h.stack.slice(0, h.index + 1), taskId];
      h.index = h.stack.length - 1;
    }
    setHistNav({ canBack: h.index > 0, canFwd: h.index < h.stack.length - 1 });
  }, [taskId]);

  function stepHistory(delta: number) {
    const h = detailHistory.current;
    const next = h.index + delta;
    if (next < 0 || next >= h.stack.length) return;
    h.index = next;
    navInternal.current = true;
    setTaskId(h.stack[next]);
    setHistNav({ canBack: next > 0, canFwd: next < h.stack.length - 1 });
  }

  // When a popup (status / date) closes, hand focus back to the nav container
  // so arrow-nav resumes from the cursor.
  useEffect(() => {
    if (taskId === null || statusPickerOpen || dueOpen) return;
    contentRef.current?.focus({ preventScroll: true });
  }, [statusPickerOpen, dueOpen, taskId]);

  // The cursor is state-driven (no DOM focus), so the scroll container won't
  // follow it on its own — bring the highlighted element into view (smoothly).
  // Status/due live first (the rail / properties block); landing on them
  // scrolls the scroll container back to its top so the leading content is
  // visible again.
  useEffect(() => {
    if (!navKey) return;
    if (navKey === "status" || navKey === "due") {
      mainRef.current?.scrollTo({ top: 0, behavior: "smooth" });
      return;
    }
    contentRef.current
      ?.querySelector<HTMLElement>(`[data-nav-key="${CSS.escape(navKey)}"]`)
      ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
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
    // SWR: render the cached detail immediately (no spinner / reload flash),
    // then revalidate silently in the background.
    const cached = detailCache.get(taskId);
    if (cached) {
      setDetail(cached);
      setLoading(false);
    } else {
      setDetail(null);
      setLoading(true);
    }
    setError(null);
    invoke<ClickUpTaskDetailData>("clickup_task_detail", { taskId })
      .then((data) => {
        if (!cancelled) setDetail(data);
      })
      .catch((err) => {
        // A failed background revalidate keeps the cached view; only surface the
        // error when there was nothing cached to show.
        if (!cancelled && !detailCache.has(taskId)) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [taskId]);

  // Keep the cache fresh — covers the initial fetch and every optimistic write
  // (handlers call setDetail), so revisiting the task shows the latest state.
  useEffect(() => {
    if (taskId && detail) detailCache.set(taskId, detail);
  }, [taskId, detail]);

  // Load statuses for the current task's list when the detail changes.
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

  // After a write lands, ClickUp has read-after-write lag: re-fetch a few times
  // until the server reflects our optimistic value, then drop the overlay.
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

  const task = detail?.task ?? null;

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
        // Hold the spinner until the comment actually shows (ClickUp lag).
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

  const subtasks = taskId ? tasks.filter((t) => t.parent_id === taskId) : [];

  // Full parent → children map for the recursive subtask tree (sub-subtasks
  // and deeper). Built from the mirror's open tasks, same source the panel uses.
  const childrenByParent = useMemo(() => {
    const m = new Map<string, ClickUpTask[]>();
    for (const t of tasks) {
      if (t.parent_id) {
        const arr = m.get(t.parent_id);
        if (arr) arr.push(t);
        else m.set(t.parent_id, [t]);
      }
    }
    return m;
  }, [tasks]);

  // Subtasks usually share the parent's list, whose workflow is already loaded
  // in `statuses`; fall back to the panel-cached set for cross-list subtasks.
  function subFraction(sub: ClickUpTask): number {
    const set = sub.list_id === task?.list_id ? statuses : listStatuses[sub.list_id];
    return statusFraction(set, sub.status_name);
  }

  function renderSubtree(parentId: string, depth: number, seen: ReadonlySet<string>): React.ReactNode {
    const children = childrenByParent.get(parentId) ?? [];
    return children.map((sub) => {
      const recurse = !seen.has(sub.id);
      return (
        <div key={sub.id}>
          <button
            type="button"
            onClick={() => setTaskId(sub.id)}
            data-nav-key={`sub:${sub.id}`}
            data-nav-selected={navKey === `sub:${sub.id}` || undefined}
            style={{ paddingLeft: 4 + depth * 16 }}
            className="flex w-full items-center gap-1.5 rounded py-0.5 pr-1 text-left text-[11px] text-foreground/80 outline-none transition-colors hover:bg-secondary/40"
          >
            <StatusIcon
              type={sub.status_type}
              color={sub.status_color}
              fraction={subFraction(sub)}
              size={12}
              className="shrink-0"
              title={sub.status_name ?? undefined}
            />
            <span className="truncate">{sub.name}</span>
          </button>
          {recurse && renderSubtree(sub.id, depth + 1, new Set([...seen, sub.id]))}
        </div>
      );
    });
  }

  // Apply optimistic overlay to status_name for the status pill.
  const overlayStatusName = taskId
    ? (overlay as Record<string, { value: string | null } | undefined>)[`${taskId}:status`]?.value ?? null
    : null;
  const displayStatusName = overlayStatusName ?? task?.status_name ?? null;
  const displayStatusColor = (() => {
    if (!overlayStatusName || !taskId) return task?.status_color ?? null;
    return statuses.find((s) => s.name === overlayStatusName)?.color ?? task?.status_color ?? null;
  })();
  const displayStatusType = (() => {
    if (!overlayStatusName || !taskId) return task?.status_type ?? null;
    return statuses.find((s) => s.name === overlayStatusName)?.status_type ?? task?.status_type ?? null;
  })();

  // Optimistic due-date overlay (mirrors the status overlay).
  const overlayDue = taskId
    ? (overlay as Record<string, { value: string | null } | undefined>)[`${taskId}:dueDate`]
    : undefined;
  const displayDueMs =
    overlayDue !== undefined
      ? overlayDue.value !== null
        ? Number(overlayDue.value)
        : null
      : task?.due_date ?? null;

  function activateNav(key: string) {
    if (busy) return;
    if (key === "taskid") {
      if (task) copyTaskId(task.custom_id ?? task.id);
    } else if (key === "status") setStatusPickerOpen(true);
    else if (key === "due") setDueOpen(true);
    else if (key === "desc") {
      setDescDraft(detail?.description ?? "");
      setEditingDesc(true);
    } else if (key.startsWith("sub:")) setTaskId(key.slice(4));
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

  // Clicking an actionable element moves the cursor there; refocus the
  // container (WebKitGTK doesn't focus buttons on click) so arrow-nav resumes —
  // except when clicking into the comment field.
  function handleContainerClick(e: React.MouseEvent) {
    const target = e.target as HTMLElement | null;
    const el = target?.closest<HTMLElement>("[data-nav-key]");
    if (el?.dataset.navKey) setNavKey(el.dataset.navKey);
    if (target && target.tagName !== "TEXTAREA" && target.tagName !== "INPUT") {
      contentRef.current?.focus({ preventScroll: true });
    }
  }

  return {
    taskId,
    setTaskId,
    detail,
    loading,
    error,
    task,
    statuses,
    statusesLoading,
    editingDesc,
    setEditingDesc,
    descDraft,
    setDescDraft,
    statusPickerOpen,
    setStatusPickerOpen,
    dueOpen,
    setDueOpen,
    commentDraft,
    setCommentDraft,
    commentFocused,
    setCommentFocused,
    postingComment,
    uncertainComment,
    busy,
    navKey,
    setNavKey,
    overlay,
    contentRef,
    mainRef,
    commentRef,
    histNav,
    stepHistory,
    subtasks,
    renderSubtree,
    overlayStatusName,
    displayStatusName,
    displayStatusColor,
    displayStatusType,
    displayDueMs,
    copyTaskId,
    handleStatusChange,
    handleChecklistToggle,
    handleDescSave,
    handleDueDateSave,
    handleRemoveAssignee,
    handlePostComment,
    handleVerifyComment,
    handleNavKeyDown,
    handleContainerClick,
  };
}

/// The scrollable detail content, shared by the modal (two-column: properties
/// rail + main) and the tab (single column). The interactive elements carry the
/// same `data-nav-key` cursor in both layouts; DOM order is the nav source of
/// truth, so the properties block leads in both.
export function ClickUpTaskBody({ c, layout }: { c: TaskController; layout: "modal" | "tab" }) {
  const isTab = layout === "tab";
  const { detail, task, loading, error, navKey, overlay, busy } = c;

  // In the tab the outer container is the single scroll surface, so the
  // scroll-to-top nav effect (mainRef) must target it too.
  const setOuterRef = (el: HTMLDivElement | null) => {
    c.contentRef.current = el;
    if (isTab) c.mainRef.current = el;
  };

  // The focusable container must persist across loading → detail so the index
  // cursor keeps its single focus (the original modal nested the states inside
  // one always-mounted container). Render the placeholder INSIDE the same outer
  // shell rather than as an early return.
  const placeholder =
    loading && !detail ? (
      <div className="flex h-full w-full flex-col items-center justify-center gap-2 px-6">
        <ProgressBar className="max-w-32" />
        <span className="text-xs text-muted-foreground">Loading…</span>
      </div>
    ) : error ? (
      <div className="flex h-full w-full items-center justify-center px-6 text-center text-xs text-red-400">
        {error}
      </div>
    ) : null;
  if (placeholder || !detail) {
    return isTab ? (
      <div
        ref={setOuterRef}
        tabIndex={0}
        onKeyDown={c.handleNavKeyDown}
        onClick={c.handleContainerClick}
        className="h-full overflow-y-auto outline-none"
      >
        {placeholder}
      </div>
    ) : (
      <div
        ref={c.contentRef}
        tabIndex={0}
        onKeyDown={c.handleNavKeyDown}
        onClick={c.handleContainerClick}
        className="flex h-full outline-none"
      >
        {placeholder}
      </div>
    );
  }

  const taskIdEl = task && (task.custom_id ?? task.id) ? (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            data-nav-key="taskid"
            data-nav-selected={navKey === "taskid" || undefined}
            aria-label="Copy task ID"
            onClick={(e) => {
              e.stopPropagation();
              c.copyTaskId(task.custom_id ?? task.id);
            }}
            className="flex items-center gap-1 self-start rounded px-1 font-mono text-[10px] tabular-nums text-muted-foreground outline-none transition-colors hover:text-foreground"
          />
        }
      >
        {task.custom_id ?? task.id}
        <Copy size={9} className="shrink-0" />
      </TooltipTrigger>
      <TooltipContent side="bottom" className="text-[10px]">Copy task ID</TooltipContent>
    </Tooltip>
  ) : null;

  const statusEl = c.displayStatusName ? (
    <StatusPicker
      currentStatus={c.displayStatusName}
      currentColor={c.displayStatusColor}
      currentType={c.displayStatusType}
      statuses={c.statuses}
      loading={c.statusesLoading}
      onSelect={(name) => void c.handleStatusChange(name)}
      pending={!!c.overlayStatusName || busy === "status"}
      open={c.statusPickerOpen}
      onOpenChange={c.setStatusPickerOpen}
      navSelected={navKey === "status"}
    />
  ) : null;

  const priorityEl = task?.priority ? (
    <div className="flex items-center gap-1.5 text-[11px] text-foreground/80">
      <PriorityIcon priority={task.priority} size={13} className="shrink-0" />
      <span className="capitalize">{task.priority}</span>
    </div>
  ) : null;

  const assigneesEls = task?.assignees.map((a) => (
    <div
      key={a.id ?? a.username ?? "?"}
      className="group flex items-center gap-1.5 text-[11px] text-foreground/80"
    >
      <span
        title={a.username ?? undefined}
        className="flex size-4 shrink-0 items-center justify-center rounded-full text-[8px] font-medium text-white"
        style={{ background: a.color ?? "var(--color-secondary)" }}
      >
        {a.initials ?? (a.username?.slice(0, 2).toUpperCase() ?? "?")}
      </span>
      <span className="min-w-0 flex-1 truncate">{a.username ?? "Unknown"}</span>
      {a.id !== null && (
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                aria-label={`Remove ${a.username ?? "assignee"}`}
                onClick={() => void c.handleRemoveAssignee(a.id!)}
                className="hidden size-3 shrink-0 items-center justify-center rounded-full bg-secondary/80 text-muted-foreground group-hover:flex hover:text-red-400"
              />
            }
          >
            <UserMinus size={8} />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">{`Remove ${a.username ?? "assignee"}`}</TooltipContent>
        </Tooltip>
      )}
    </div>
  ));

  const dueEl = (
    <div className="flex items-center gap-1.5 text-[11px]">
      <DatePopover
        valueMs={c.displayDueMs}
        onSelect={(ms) => void c.handleDueDateSave(ms)}
        open={c.dueOpen}
        onOpenChange={c.setDueOpen}
        disabled={busy !== null}
        navSelected={navKey === "due"}
      />
      {busy === "dueDate" && <PulseDots count={1} className="text-muted-foreground" dotClassName="size-1" />}
    </div>
  );

  const listEl = task ? (
    <span
      className="inline-flex max-w-full items-center gap-1 self-start rounded bg-secondary/60 px-1.5 py-0.5 text-[10px] font-medium text-foreground/80"
      title={task.list_name}
    >
      <List size={10} className="shrink-0 text-muted-foreground" />
      <span className="truncate">{task.list_name}</span>
    </span>
  ) : null;

  const tagsEl = task && task.tags.length > 0 ? (
    <div className="flex flex-col gap-1.5">
      <SectionCaps label="Labels" />
      <div className="flex flex-wrap gap-1">
        {task.tags.map((tag) => (
          <span
            key={tag.name}
            className="rounded-full px-1.5 text-[10px] leading-4"
            style={{
              background: tag.tag_bg ? `${tag.tag_bg}33` : "var(--color-secondary)",
              color: tag.tag_fg ?? tag.tag_bg ?? "var(--color-secondary-foreground)",
            }}
          >
            {tag.name}
          </span>
        ))}
      </div>
    </div>
  ) : null;

  const fieldsEl = detail.custom_values.length > 0 ? (
    <div className="flex flex-col gap-1.5">
      <SectionCaps label="Fields" />
      <div className="flex flex-col gap-1.5 text-[11px]">
        {detail.custom_values.map((cv) => {
          const rendered = formatCustomValue(cv);
          if (rendered === null) return null;
          return (
            <div key={cv.field_id} className="flex flex-col">
              <span className="text-[10px] text-muted-foreground">{cv.name}</span>
              <span className="min-w-0 truncate text-foreground/80" title={rendered}>
                {rendered}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  ) : null;

  // Description — untrusted multi-writer markdown rendered through the sanitizing
  // MarkdownView pipeline (no raw-HTML passthrough).
  const descriptionEl = (
    <>
      <div className="flex items-center gap-2">
        <SectionCaps label="Description" />
        {!c.editingDesc && (
          <Tooltip>
            <TooltipTrigger
              render={
                <button
                  type="button"
                  aria-label="Edit description"
                  data-nav-key="desc"
                  data-nav-selected={navKey === "desc" || undefined}
                  onClick={(e) => {
                    e.stopPropagation();
                    c.setDescDraft(detail.description ?? "");
                    c.setEditingDesc(true);
                  }}
                  className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground outline-none hover:bg-secondary/60 hover:text-foreground transition-colors"
                />
              }
            >
              <Pencil size={11} />
            </TooltipTrigger>
            <TooltipContent side="bottom" className="text-[10px]">Edit description</TooltipContent>
          </Tooltip>
        )}
      </div>
      {c.editingDesc ? (
        // data-floating-popup makes the FloatingPanel's window Escape handler
        // skip the close while editing — Escape cancels the edit instead.
        <div className="flex flex-col gap-1.5" data-floating-popup>
          <textarea
            value={c.descDraft}
            onChange={(e) => c.setDescDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.ctrlKey) {
                e.preventDefault();
                void c.handleDescSave();
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                c.setEditingDesc(false);
                c.setNavKey("desc");
                c.contentRef.current?.focus({ preventScroll: true });
              }
            }}
            className="min-h-[80px] rounded border border-input bg-secondary/30 p-2 text-[11px] leading-relaxed text-foreground/90 outline-none focus-visible:ring-1 focus-visible:ring-ring/50 resize-y"
            autoFocus
          />
          <div className="flex gap-1.5">
            <button
              type="button"
              onClick={() => void c.handleDescSave()}
              className="rounded bg-secondary/60 px-2 py-0.5 text-[10px] text-green-400 hover:text-green-300 transition-colors"
            >
              Save (Ctrl+Enter)
            </button>
            <button
              type="button"
              onClick={() => c.setEditingDesc(false)}
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
    </>
  );

  const subtasksEl = c.subtasks.length > 0 ? (
    <>
      <SectionCaps label={`Subtasks · ${c.subtasks.length}`} />
      <div className="flex flex-col">
        {c.renderSubtree(c.taskId ?? "", 0, new Set<string>())}
      </div>
    </>
  ) : null;

  const checklistsEl = detail.checklists.length > 0 ? (
    <>
      <SectionCaps label="Checklists" />
      {detail.checklists.map((cl) => (
        <div key={cl.id} className="flex flex-col gap-0.5">
          {cl.name && <p className="text-[11px] font-medium text-foreground/80">{cl.name}</p>}
          {cl.items.map((item) => {
            const overlayKey = c.taskId ? `${c.taskId}:checklist:${cl.id}:${item.id}` : null;
            const overlayVal = overlayKey ? overlay[overlayKey as `${string}:${string}`]?.value : undefined;
            const resolved = overlayVal !== undefined ? overlayVal === "true" : item.resolved;
            const pending = overlayVal !== undefined;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => void c.handleChecklistToggle(cl.id, item)}
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
                {pending && <PulseDots count={1} className="ml-auto shrink-0 text-muted-foreground" dotClassName="size-1" />}
              </button>
            );
          })}
        </div>
      ))}
    </>
  ) : null;

  const attachmentsEl = detail.attachments.length > 0 ? (
    <>
      <SectionCaps label="Attachments" />
      <div className="flex flex-wrap gap-1.5">
        {detail.attachments.map((att) => (
          <AttachmentChip key={att.id} attachment={att} />
        ))}
      </div>
    </>
  ) : null;

  const commentsEl = (
    <>
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
              <div className="-my-1.5">
                <MarkdownView content={comment.text ?? ""} />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Comment composer — token-gated via closure token. */}
      <div className="flex flex-col gap-1.5 pt-1 border-t border-border/40">
        {c.uncertainComment && (
          <div className="rounded border border-yellow-500/30 bg-yellow-500/10 px-2 py-1 text-[10px] text-yellow-300">
            Status unclear — verify before retrying.{" "}
            <button
              type="button"
              onClick={() => void c.handleVerifyComment()}
              className="underline hover:no-underline"
            >
              Check if it landed
            </button>
          </div>
        )}
        <textarea
          ref={c.commentRef}
          value={c.commentDraft}
          data-floating-popup={c.commentFocused || undefined}
          onFocus={() => c.setCommentFocused(true)}
          onBlur={() => c.setCommentFocused(false)}
          onChange={(e) => c.setCommentDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && e.ctrlKey) {
              e.preventDefault();
              void c.handlePostComment();
            } else if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              c.setNavKey("comment");
              c.contentRef.current?.focus();
            }
          }}
          data-nav-key="comment"
          placeholder="Add a comment… (Ctrl+Enter to post)"
          rows={2}
          className={`rounded border bg-secondary/30 p-1.5 text-[11px] leading-relaxed text-foreground/90 outline-none focus-visible:ring-1 focus-visible:ring-ring/50 resize-none ${navKey === "comment" ? "border-foreground/40" : "border-input"}`}
          disabled={c.postingComment || (busy !== null && busy !== "comment")}
        />
        <div className="flex justify-end">
          <button
            type="button"
            onClick={() => void c.handlePostComment()}
            disabled={!c.commentDraft.trim() || c.postingComment || (busy !== null && busy !== "comment")}
            className="flex items-center gap-1 rounded bg-secondary px-2 py-0.5 text-[10px] text-foreground/80 hover:bg-secondary/80 disabled:opacity-40 transition-colors"
          >
            {c.postingComment ? (
              <>Posting <PulseDots className="ml-0.5" /></>
            ) : (
              <><Send size={9} /> Comment (Ctrl+Enter)</>
            )}
          </button>
        </div>
      </div>
    </>
  );

  if (isTab) {
    // Single column, centered for readability. Properties block leads (so the
    // index cursor still starts at status/due), then the main content stacks.
    return (
      <div
        ref={setOuterRef}
        tabIndex={0}
        data-clickup-nav-root
        onKeyDown={c.handleNavKeyDown}
        onClick={c.handleContainerClick}
        className="h-full overflow-y-auto outline-none"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 px-5 py-4">
          <div className="flex flex-col gap-2">
            <SectionCaps label="Properties" />
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              {statusEl}
              {priorityEl}
              {dueEl}
              {listEl}
              {taskIdEl}
            </div>
            {assigneesEls && assigneesEls.length > 0 && (
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1">{assigneesEls}</div>
            )}
            {tagsEl}
            {fieldsEl}
          </div>
          <div className="border-t border-border" />
          {descriptionEl}
          {subtasksEl}
          {checklistsEl}
          {attachmentsEl}
          {commentsEl}
        </div>
      </div>
    );
  }

  // Modal: properties rail (DOM-first so the cursor leads with status/due,
  // order-2 places it visually on the right — Linear's issue-detail layout).
  return (
    <div
      ref={c.contentRef}
      tabIndex={0}
      onKeyDown={c.handleNavKeyDown}
      onClick={c.handleContainerClick}
      className="flex h-full outline-none"
    >
      <aside className="order-2 flex w-44 shrink-0 flex-col gap-3 border-l border-border/50 px-2.5 py-2">
        <div className="flex flex-col gap-1.5">
          <SectionCaps label="Properties" />
          {taskIdEl}
          {statusEl}
          {priorityEl}
          {assigneesEls}
          {dueEl}
          {listEl}
        </div>
        {tagsEl}
        {fieldsEl}
      </aside>

      <main ref={c.mainRef} className="order-1 flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto px-3 py-2">
        {descriptionEl}
        {subtasksEl}
        {checklistsEl}
        {attachmentsEl}
        {commentsEl}
      </main>
    </div>
  );
}

/// Drill-in history chevrons — modal title bar + tab header.
export function TaskHistoryNav({ c }: { c: TaskController }) {
  return (
    <>
      <button
        type="button"
        aria-label="Back"
        disabled={!c.histNav.canBack}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => c.stepHistory(-1)}
        className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-25 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
      >
        <ChevronLeft size={14} />
      </button>
      <button
        type="button"
        aria-label="Forward"
        disabled={!c.histNav.canFwd}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => c.stepHistory(1)}
        className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground disabled:opacity-25 disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
      >
        <ChevronRight size={14} />
      </button>
    </>
  );
}

/// Status glyph + task name — modal title bar + tab header.
export function TaskTitleContent({ c }: { c: TaskController }) {
  return (
    <>
      <StatusIcon
        type={c.displayStatusType}
        color={c.displayStatusColor}
        fraction={statusFraction(c.statuses, c.displayStatusName)}
        size={13}
        className="shrink-0"
        title={c.displayStatusName ?? undefined}
      />
      <span className="truncate text-sm font-medium text-foreground">
        {c.task?.name ?? "ClickUp task"}
      </span>
    </>
  );
}

/// Contextual task verbs — modal toolbar + tab header. `onConvertToTab`, when
/// provided (modal only), adds the "Open as tab" (T) button.
export function TaskVerbToolbar({
  taskId,
  taskUrl,
  onConvertToTab,
}: {
  taskId: string;
  taskUrl: string | null;
  onConvertToTab?: () => void;
}) {
  const boundTaskId = useAtomValue(activeSessionClickUpTaskAtom);
  const pinnedTaskIds = useAtomValue(activeSessionClickUpPinsAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const requestSend = useSetAtom(requestSendTaskAction);
  const spawnWorktree = useSetAtom(spawnWorktreeWithTaskAction);
  const togglePin = useSetAtom(togglePinTaskAction);
  const requestBind = useSetAtom(requestBindTaskAction);
  const reinject = useSetAtom(reinjectTaskAction);
  const setClosureOffer = useSetAtom(clickupClosureOfferAtom);

  const isPinned = pinnedTaskIds.includes(taskId);
  const isBound = taskId === boundTaskId;

  return (
    <>
      {onConvertToTab && (
        <ToolbarAction label="Open as tab (T)" onClick={onConvertToTab}>
          <PanelRight size={12} />
        </ToolbarAction>
      )}
      <ToolbarAction label={ACTION_LABELS.send} onClick={() => requestSend(taskId)}>
        <Send size={12} />
      </ToolbarAction>
      <ToolbarAction label={ACTION_LABELS.spawn} onClick={() => void spawnWorktree(taskId)}>
        <GitBranchPlus size={12} />
      </ToolbarAction>
      <ToolbarAction
        label={isPinned ? ACTION_LABELS.unpin : ACTION_LABELS.pin}
        onClick={() => void togglePin(taskId)}
        active={isPinned}
      >
        {isPinned ? <PinOff size={12} /> : <Pin size={12} />}
      </ToolbarAction>
      <ToolbarAction
        label={isBound ? ACTION_LABELS.unbind : ACTION_LABELS.bind}
        onClick={() => void requestBind(taskId)}
        active={isBound}
      >
        {isBound ? <Unlink size={12} /> : <Link2 size={12} />}
      </ToolbarAction>
      {(isBound || pinnedTaskIds.includes(taskId)) && (
        <ToolbarAction
          label="Re-inject current task content into the live session (R) — explicit refresh, never automatic"
          onClick={() => void reinject(taskId)}
        >
          <RefreshCw size={12} />
        </ToolbarAction>
      )}
      {isBound && activeSessionId && (
        <ToolbarAction
          label="Close out task (C) — move status and/or post a comment to mark this task done"
          onClick={() => setClosureOffer({ taskId, sessionId: activeSessionId })}
        >
          <CircleCheck size={12} />
        </ToolbarAction>
      )}
      {taskUrl && (
        <ToolbarAction label="Open in ClickUp (O)" onClick={() => openExternalUrl(taskUrl)}>
          <ExternalLink size={12} />
        </ToolbarAction>
      )}
    </>
  );
}

function SectionCaps({ label }: { label: string }) {
  return (
    <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
      {label}
    </span>
  );
}

function AttachmentChip({ attachment }: { attachment: ClickUpAttachment }) {
  const image = isImageAttachment(attachment);
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
