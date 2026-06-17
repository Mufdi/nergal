import { useEffect, useRef, useState } from "react";
import { useAtomValue } from "jotai";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { invoke } from "@/lib/tauri";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { StatusIcon } from "@/components/clickup/StatusIcon";
import { PriorityIcon } from "@/components/clickup/PriorityIcon";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { linearIssuesAtom, type IssueView, type LinearComment } from "@/stores/linear";

// ── Shared helpers ──

export function linearStateToIconType(stateType: string | undefined): string | null {
  switch (stateType) {
    case "triage":
    case "backlog":
    case "unstarted":
      return "open";
    case "started":
      return "custom";
    case "completed":
      return "done";
    case "cancelled":
    case "canceled":
      return "closed";
    default:
      return null;
  }
}

export function linearPriorityStr(p: number): string | null {
  switch (p) {
    case 1: return "urgent";
    case 2: return "high";
    case 3: return "normal";
    case 4: return "low";
    default: return null;
  }
}

function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

/// Validates a URL through the backend (browser_validate_url) before opening.
/// The backend strips tracking params and rejects non-http(s) schemes.
async function openValidatedUrl(url: string | undefined): Promise<void> {
  if (!url) return;
  try {
    const safe = await invoke<string>("browser_validate_url", { url });
    await openShell(safe);
  } catch {
    // URL rejected by backend — silently drop.
  }
}

// ── Controller ──

interface LinearDetailData {
  issue: IssueView | null;
  comments: LinearComment[];
}

/// SWR cache for fetched detail payloads (issue + comments). Module-level so
/// it survives a modal/tab unmount — revisiting an issue renders instantly
/// from cache while a background revalidation keeps it fresh.
const detailCache = new Map<string, LinearDetailData>();

export type LinearIssueController = ReturnType<typeof useLinearIssueController>;

export function useLinearIssueController({
  issueId,
  setIssueId,
}: {
  issueId: string | null;
  setIssueId: (id: string | null) => void;
}) {
  const issues = useAtomValue(linearIssuesAtom);
  const [detail, setDetail] = useState<LinearDetailData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const mainRef = useRef<HTMLDivElement | null>(null);

  // Reset + grab focus when issue opens
  useEffect(() => {
    if (issueId === null) return;
    const t = setTimeout(() => contentRef.current?.focus({ preventScroll: true }), 60);
    return () => clearTimeout(t);
  }, [issueId]);

  useEffect(() => {
    if (!issueId) {
      setDetail(null);
      setError(null);
      return;
    }
    let cancelled = false;
    const cached = detailCache.get(issueId);
    if (cached) {
      setDetail(cached);
      setLoading(false);
    } else {
      setDetail(null);
      setLoading(true);
    }
    setError(null);

    // Fetch issue view from mirror + comments lazily
    Promise.all([
      invoke<IssueView[]>("linear_read_issues", { includeStale: true }),
      invoke<LinearComment[]>("linear_issue_comments", { issueId }),
    ])
      .then(([issueList, comments]) => {
        if (cancelled) return;
        const issue = issueList.find((i) => i.id === issueId) ?? null;
        const data: LinearDetailData = { issue, comments };
        setDetail(data);
        detailCache.set(issueId, data);
      })
      .catch((err) => {
        if (cancelled && !detailCache.has(issueId)) setError(String(err));
        if (!cancelled && !detailCache.has(issueId)) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, [issueId]);

  // Mirror atom is the fast path for the issue snapshot (no extra fetch)
  const mirrorIssue = issueId ? issues.find((i) => i.id === issueId) ?? null : null;
  const issue = detail?.issue ?? mirrorIssue;
  const comments = detail?.comments ?? [];

  // Sub-issues from the mirror (parentId === issueId)
  const subIssues = issueId ? issues.filter((i) => i.parentId === issueId) : [];

  return {
    issueId,
    setIssueId,
    detail,
    loading,
    error,
    issue,
    comments,
    subIssues,
    contentRef,
    mainRef,
  };
}

// ── Body (shared by modal + future tab) ──

function SectionCaps({ label }: { label: string }) {
  return (
    <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
      {label}
    </span>
  );
}

export function LinearIssueBody({
  c,
  layout,
}: {
  c: LinearIssueController;
  layout: "modal" | "tab";
}) {
  const isTab = layout === "tab";
  const { detail, issue, loading, error, comments, subIssues } = c;

  const setOuterRef = (el: HTMLDivElement | null) => {
    c.contentRef.current = el;
    if (isTab) c.mainRef.current = el;
  };

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

  if (placeholder || !issue) {
    return (
      <div
        ref={setOuterRef}
        tabIndex={0}
        className={`outline-none ${isTab ? "h-full overflow-y-auto" : "flex h-full"}`}
      >
        {placeholder}
      </div>
    );
  }

  const iconType = linearStateToIconType(issue.stateType);
  const priorityStr = linearPriorityStr(issue.priority);

  const propertiesEl = (
    <div className="flex flex-col gap-2">
      <SectionCaps label="Properties" />
      <div className="flex flex-col gap-1.5 text-[11px]">
        {issue.stateName && (
          <div className="flex items-center gap-1.5">
            <StatusIcon type={iconType} color={issue.stateColor ?? null} size={13} className="shrink-0" />
            <span className="text-foreground/80">{issue.stateName}</span>
          </div>
        )}
        {priorityStr && (
          <div className="flex items-center gap-1.5">
            <PriorityIcon priority={priorityStr} size={13} className="shrink-0" />
            <span className="capitalize text-foreground/80">{priorityStr}</span>
          </div>
        )}
        {issue.assigneeName && (
          <div className="flex items-center gap-1.5">
            <span className="flex size-4 shrink-0 items-center justify-center rounded-full bg-secondary text-[8px] font-medium text-foreground/70">
              {issue.assigneeName.slice(0, 2).toUpperCase()}
            </span>
            <span className="truncate text-foreground/80">{issue.assigneeName}</span>
          </div>
        )}
        {issue.projectName && (
          <div className="flex items-center gap-1.5 text-foreground/80">
            <span className="size-2 shrink-0 rounded-sm bg-primary/40" aria-hidden />
            <span className="truncate">{issue.projectName}</span>
          </div>
        )}
        {issue.updatedAt && (
          <div className="text-[10px] text-muted-foreground">
            Updated {formatDate(issue.updatedAt)}
          </div>
        )}
        {issue.identifier && (
          <div className="font-mono text-[10px] text-muted-foreground/60">{issue.identifier}</div>
        )}
        {issue.url && (
          <button
            type="button"
            onClick={() => void openValidatedUrl(issue.url)}
            className="self-start text-[10px] text-accent underline hover:no-underline"
          >
            Open in Linear
          </button>
        )}
      </div>

      {issue.labels.length > 0 && (
        <div className="flex flex-col gap-1">
          <SectionCaps label="Labels" />
          <div className="flex flex-wrap gap-1">
            {issue.labels.map((label) => (
              <span
                key={label.id}
                className="rounded-full px-1.5 text-[10px] leading-4"
                style={{
                  background: label.color ? `${label.color}33` : "var(--color-secondary)",
                  color: label.color ?? "var(--color-secondary-foreground)",
                }}
              >
                {label.name}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );

  // Description from Linear issue is untrusted markdown — gate remote images
  // to prevent tracking pixels / SSRF per the spec security requirement.
  const descEl = (
    <>
      <SectionCaps label="Description" />
      {issue.description && issue.description.trim() ? (
        <div className="-mx-3 -my-2 rounded">
          <MarkdownView content={issue.description} gateRemoteImages />
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">No description</p>
      )}
    </>
  );

  const subIssuesEl = subIssues.length > 0 ? (
    <>
      <SectionCaps label={`Sub-issues · ${subIssues.length}`} />
      <div className="flex flex-col gap-0.5">
        {subIssues.map((sub) => (
          <button
            key={sub.id}
            type="button"
            onClick={() => c.setIssueId(sub.id)}
            className="flex items-center gap-1.5 rounded py-0.5 text-left text-[11px] text-foreground/80 transition-colors hover:bg-secondary/40"
          >
            <StatusIcon
              type={linearStateToIconType(sub.stateType)}
              color={sub.stateColor ?? null}
              size={12}
              className="shrink-0"
            />
            {sub.identifier && (
              <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">{sub.identifier}</span>
            )}
            <span className="truncate">{sub.title}</span>
          </button>
        ))}
      </div>
    </>
  ) : null;

  const commentsEl = (
    <>
      <SectionCaps label={`Comments · ${comments.length}`} />
      {comments.length === 0 ? (
        <p className="text-xs text-muted-foreground">No comments</p>
      ) : (
        <div className="flex flex-col gap-2">
          {comments.map((comment) => (
            <div key={comment.id} className="rounded border border-border/50 bg-secondary/20">
              <div className="flex items-center gap-1.5 px-2 pt-1.5 text-[10px] text-muted-foreground">
                <span
                  className="flex size-4 items-center justify-center rounded-full bg-secondary text-[8px] font-medium text-foreground/70"
                >
                  {(comment.author ?? "?").slice(0, 2).toUpperCase()}
                </span>
                <span className="font-medium text-foreground/70">{comment.author ?? "Unknown"}</span>
                {comment.createdAt != null && (
                  <span>{formatDate(comment.createdAt)}</span>
                )}
              </div>
              <div className="-my-1.5">
                {/* Comments are also untrusted — gate images */}
                <MarkdownView content={comment.body ?? ""} gateRemoteImages />
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );

  if (isTab) {
    return (
      <div
        ref={setOuterRef}
        tabIndex={0}
        className="h-full overflow-y-auto outline-none"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 px-5 py-4">
          {propertiesEl}
          <div className="border-t border-border" />
          {descEl}
          {subIssuesEl}
          {commentsEl}
        </div>
      </div>
    );
  }

  // Modal: properties rail on the right (order-2), main content scrolls left
  return (
    <div ref={c.contentRef} tabIndex={0} className="flex h-full outline-none">
      <aside className="order-2 flex w-44 shrink-0 flex-col gap-3 border-l border-border/50 px-2.5 py-2">
        {propertiesEl}
      </aside>
      <main ref={c.mainRef} className="order-1 flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto px-3 py-2">
        {descEl}
        {subIssuesEl}
        {commentsEl}
      </main>
    </div>
  );
}

/// Title bar content for the floating modal.
export function LinearIssueTitleContent({ c }: { c: LinearIssueController }) {
  const iconType = linearStateToIconType(c.issue?.stateType);
  return (
    <>
      <StatusIcon
        type={iconType}
        color={c.issue?.stateColor ?? null}
        size={13}
        className="shrink-0"
        title={c.issue?.stateName}
      />
      <span className="truncate text-sm font-medium text-foreground">
        {c.issue?.title ?? "Linear issue"}
      </span>
    </>
  );
}
