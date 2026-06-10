import { useEffect, useState } from "react";
import { useAtom, useAtomValue } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { CheckSquare, ExternalLink, Loader2, Paperclip, Square } from "lucide-react";
import { invoke } from "@/lib/tauri";
import { FloatingPanel } from "@/components/floating/FloatingPanel";
import { MarkdownView } from "@/components/plan/MarkdownView";
import {
  clampGeometryToViewport,
  type FloatingGeometry,
} from "@/stores/scratchpad";
import {
  clickupDetailTaskIdAtom,
  clickupTasksAtom,
  type ClickUpAttachment,
  type ClickUpCustomValue,
  type ClickUpTaskDetailData,
} from "@/stores/clickup";

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

export function ClickUpTaskDetail() {
  const [taskId, setTaskId] = useAtom(clickupDetailTaskIdAtom);
  const tasks = useAtomValue(clickupTasksAtom);
  const [detail, setDetail] = useState<ClickUpTaskDetailData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [geometry, setGeometry] = useState<FloatingGeometry>(DEFAULT_GEOMETRY);

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

  function handleGeometryChange(next: FloatingGeometry) {
    setGeometry(next);
    invoke("scratchpad_set_geometry", {
      panelId: DETAIL_PANEL_ID,
      geometryJson: JSON.stringify(next),
      opacity: 1,
    }).catch(() => {});
  }

  const task = detail?.task ?? null;
  const subtasks = taskId ? tasks.filter((t) => t.parent_id === taskId) : [];

  return (
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
          <span className="truncate">{task?.name ?? "ClickUp task"}</span>
        </>
      }
      toolbar={
        task?.url ? (
          <button
            type="button"
            aria-label="Open in ClickUp"
            title="Open in ClickUp"
            onClick={() => openExternalUrl(task.url)}
            className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground transition-colors"
          >
            <ExternalLink size={12} />
          </button>
        ) : undefined
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
              {task?.status_name && (
                <span
                  className="rounded-full px-2 leading-4"
                  style={{
                    background: task.status_color ? `${task.status_color}26` : "var(--color-secondary)",
                    color: task.status_color ?? "var(--color-secondary-foreground)",
                  }}
                >
                  {task.status_name}
                </span>
              )}
              {task?.priority && <span>priority: {task.priority}</span>}
              {task?.due_date != null && <span>due {formatDateTime(task.due_date)}</span>}
              {task && <span className="truncate">{task.list_name}</span>}
              {task?.assignees.map((a) => (
                <span
                  key={a.id ?? a.username ?? "?"}
                  title={a.username ?? undefined}
                  className="flex size-4 items-center justify-center rounded-full text-[8px] font-medium text-white"
                  style={{ background: a.color ?? "var(--color-secondary)" }}
                >
                  {a.initials ?? (a.username?.slice(0, 2).toUpperCase() ?? "?")}
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
            <SectionCaps label="Description" />
            {detail.description ? (
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
                    {cl.items.map((item) => (
                      <div key={item.id} className="flex items-center gap-1.5 pl-1 text-[11px]">
                        {item.resolved ? (
                          <CheckSquare size={11} className="shrink-0 text-green-500" />
                        ) : (
                          <Square size={11} className="shrink-0 text-muted-foreground" />
                        )}
                        <span className={item.resolved ? "text-muted-foreground line-through" : "text-foreground/80"}>
                          {item.name}
                        </span>
                      </div>
                    ))}
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
              <div className="flex flex-col gap-2 pb-2">
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
          </div>
        ) : null}
      </div>
    </FloatingPanel>
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
