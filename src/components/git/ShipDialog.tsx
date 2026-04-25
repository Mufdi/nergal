import { useEffect, useState, useCallback, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { listen, invoke } from "@/lib/tauri";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { Loader2, AlertTriangle, Check, CircleDashed, FileDiff } from "lucide-react";
import { shipDialogAtom, triggerShipAtom, type ShipProgressEvent } from "@/stores/ship";
import { toastsAtom } from "@/stores/toast";
import { refreshGitInfoAtom } from "@/stores/git";

interface PrCommit { hash: string; subject: string }
interface PrDiffstat { added: number; removed: number; files: number }
interface PrPreviewData {
  base: string;
  commits: PrCommit[];
  diffstat: PrDiffstat;
  template: string | null;
  staged_count: number;
  has_staged_diffstat: boolean;
}
interface PrInfo { number: number; title: string; state: string; url: string }
interface ShipResult { commit_hash: string | null; pr_info: PrInfo }
interface ChangedFile { path: string; status: string }
interface GitFullStatus { staged: ChangedFile[]; unstaged: ChangedFile[]; untracked: string[] }
interface UnstagedEntry { path: string; source: "unstaged" | "untracked"; status: string }

/// Tooling artefacts that show up as untracked when a worktree was branched
/// from a commit predating an updated .gitignore. Never useful to stage from
/// the Ship dialog — keep them out of the picker noise.
const TOOLING_PATH_PREFIXES = [
  ".memory-opt-out",
  ".worktrees/",
  ".claude/plans/",
  ".claude/settings.local.json",
  ".claude/skills/",
  ".claude/commands/",
  ".agent/",
  ".agents/",
  ".opencode/",
  ".windsurf/",
  ".playwright-cli/",
  ".gitnexus",
  "node_modules/",
  "dist/",
  "target/",
  "src-tauri/target/",
] as const;

function isToolingPath(p: string): boolean {
  return TOOLING_PATH_PREFIXES.some((prefix) => p === prefix || p.startsWith(prefix));
}

function buildBody(preview: PrPreviewData): string {
  const bullets = preview.commits.map((c) => `- ${c.subject}`).join("\n");
  const stat = preview.diffstat;
  const diffLine = `+${stat.added} -${stat.removed} across ${stat.files} files`;
  const stagedNote = preview.staged_count > 0
    ? `\n\n_Staged changes (${preview.staged_count} files) will be committed as part of Ship._`
    : "";
  let body = bullets.length > 0
    ? `${bullets}\n\n---\n${diffLine}${stagedNote}`
    : `${stagedNote.trim() || "_No commits yet._"}`;
  if (preview.template) {
    body += `\n\n---\n${preview.template.trim()}`;
  }
  return body;
}

export function ShipDialog() {
  const state = useAtomValue(shipDialogAtom);
  const setState = useSetAtom(shipDialogAtom);
  const trigger = useAtomValue(triggerShipAtom);
  const addToast = useSetAtom(toastsAtom);
  const refreshGit = useSetAtom(refreshGitInfoAtom);

  const [loading, setLoading] = useState(false);
  const [ghOk, setGhOk] = useState<boolean | null>(null);
  const [preview, setPreview] = useState<PrPreviewData | null>(null);
  const [existingPr, setExistingPr] = useState<PrInfo | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [shipping, setShipping] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [autoMerge, setAutoMerge] = useState(false);
  const [progress, setProgress] = useState<Record<string, boolean | null>>({ commit: null, push: null, pr: null });
  const [error, setError] = useState<string | null>(null);
  const confirmRef = useRef<HTMLButtonElement | null>(null);

  // Unstaged + untracked files the user can stage inline before Ship/Push.
  const [unstagedEntries, setUnstagedEntries] = useState<UnstagedEntry[]>([]);
  const [toStage, setToStage] = useState<Set<string>>(new Set());
  const [stageCursor, setStageCursor] = useState(0);

  useEffect(() => {
    if (trigger.tick === 0) return;
    setState({ open: true, sessionId: trigger.sessionId, inlineMessage: trigger.inlineMessage });
  }, [trigger, setState]);

  const load = useCallback(async (sessionId: string) => {
    setLoading(true);
    setError(null);
    try {
      const [gh, data, pr, status] = await Promise.all([
        invoke<boolean>("gh_available"),
        invoke<PrPreviewData>("get_pr_preview_data", { sessionId }),
        invoke<PrInfo | null>("get_pr_status", { sessionId }).catch(() => null),
        invoke<GitFullStatus>("get_git_status", { sessionId }).catch(() =>
          ({ staged: [], unstaged: [], untracked: [] }) as GitFullStatus,
        ),
      ]);
      setGhOk(gh);
      setPreview(data);
      setExistingPr(pr);
      const firstSubject = data.commits[0]?.subject ?? "";
      setTitle(firstSubject);
      setBody(buildBody(data));
      const entries: UnstagedEntry[] = [
        ...status.unstaged.map((f) => ({ path: f.path, source: "unstaged" as const, status: f.status })),
        ...status.untracked.map((p) => ({ path: p, source: "untracked" as const, status: "??" })),
      ].filter((e) => !isToolingPath(e.path));
      setUnstagedEntries(entries);
      // Default: pre-select everything so "Ship" stages all unstaged work — the
      // common case. User can deselect exceptions with Space.
      setToStage(new Set(entries.map((e) => e.path)));
      setStageCursor(0);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!state.open || !state.sessionId) return;
    load(state.sessionId);
    setProgress({ commit: null, push: null, pr: null });
  }, [state.open, state.sessionId, load]);

  useEffect(() => {
    const p = listen<ShipProgressEvent>("ship:progress", (payload) => {
      if (payload.session_id !== state.sessionId) return;
      setProgress((prev) => ({ ...prev, [payload.stage]: payload.ok }));
    });
    return () => { p.then((fn) => fn()); };
  }, [state.sessionId]);

  const close = useCallback(() => {
    setState({ open: false, sessionId: null, inlineMessage: null });
  }, [setState]);

  const stageSelected = useCallback(async (sessionId: string) => {
    if (toStage.size === 0) return;
    for (const path of toStage) {
      await invoke("git_stage_file", { sessionId, path });
    }
  }, [toStage]);

  const handleShip = useCallback(async () => {
    if (!state.sessionId || !preview || shipping || ghOk === false) return;
    setShipping(true);
    setError(null);
    try {
      await stageSelected(state.sessionId);
      const result = await invoke<ShipResult>("git_ship", {
        sessionId: state.sessionId,
        message: state.inlineMessage,
        prTitle: title,
        prBody: body,
        autoMerge,
      });
      const desc = autoMerge
        ? `PR #${result.pr_info.number} created — auto-merge enabled`
        : `PR #${result.pr_info.number} created`;
      addToast({ message: "Shipped", description: desc, type: "success" });
      refreshGit(state.sessionId);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setShipping(false);
    }
  }, [state.sessionId, state.inlineMessage, preview, shipping, ghOk, title, body, autoMerge, stageSelected, addToast, refreshGit, close]);

  const pushOnly = useCallback(async () => {
    if (!state.sessionId || pushing) return;
    setPushing(true);
    try {
      // Stage any checked files first — the push-only path reuses the same
      // staging UX even though it won't commit for you. Surface the outcome
      // so the user knows to commit locally if they staged anything here.
      await stageSelected(state.sessionId);
      const pushed = await invoke<boolean>("git_push", { sessionId: state.sessionId });
      addToast({
        message: "Push",
        description: pushed ? "Pushed to remote" : "Nothing to push",
        type: pushed ? "success" : "info",
      });
      refreshGit(state.sessionId);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setPushing(false);
    }
  }, [state.sessionId, pushing, stageSelected, addToast, refreshGit, close]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!state.open) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
        return;
      }
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        // With an existing PR, Ctrl+Enter pushes (updating the PR) instead of
        // attempting to re-create it — the dialog is in "push-only" mode.
        if (existingPr) {
          pushOnly();
        } else {
          handleShip();
        }
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [state.open, close, handleShip, existingPr, pushOnly]);

  const nothingToShip = preview !== null
    && preview.commits.length === 0
    && preview.staged_count === 0
    && toStage.size === 0;
  const hasExistingPr = existingPr !== null;
  const disabled = shipping || loading || ghOk === false || !title.trim() || nothingToShip || hasExistingPr;

  const pushDisabled = pushing || shipping || loading || ghOk === false;

  const toggleStage = useCallback((path: string) => {
    setToStage((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  // Global key handler scoped to the open dialog — arrows / Space / Ctrl+A
  // operate on the staging list regardless of which control owns DOM focus,
  // so the user can interact without first clicking into the file list.
  // Inputs (title/body) keep their native typing behavior because we ignore
  // the keydown when the target is one of those.
  //
  // All comparisons use `e.code` (physical key) + strict modifier checks so a
  // non-US layout, dead-key composition, or stray modifier never causes the
  // Ctrl+A "toggle all" branch to fire on a plain Space press.
  useEffect(() => {
    if (!state.open || unstagedEntries.length === 0) return;
    function onStagingKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inTextField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inTextField) return;

      // Ctrl/Cmd + A — toggle select-all. Checked first so a stuck modifier
      // can't fall through to a different branch.
      if (e.code === "KeyA" && (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        e.stopPropagation();
        setToStage((prev) =>
          prev.size === unstagedEntries.length
            ? new Set()
            : new Set(unstagedEntries.map((x) => x.path)),
        );
        return;
      }

      // Plain Space — toggle ONLY the cursor entry. Reject any modifier so a
      // stray Ctrl down (e.g. flaky modifier state) can never reach this
      // branch alongside the select-all branch.
      if (e.code === "Space" && !e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        const path = unstagedEntries[stageCursor]?.path;
        if (path) toggleStage(path);
        return;
      }

      if (e.code === "ArrowDown" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        setStageCursor((i) => (i + 1) % unstagedEntries.length);
        return;
      }
      if (e.code === "ArrowUp" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        setStageCursor((i) => (i - 1 + unstagedEntries.length) % unstagedEntries.length);
        return;
      }
    }
    window.addEventListener("keydown", onStagingKey, true);
    return () => window.removeEventListener("keydown", onStagingKey, true);
  }, [state.open, unstagedEntries, stageCursor, toggleStage]);

  return (
    <Dialog open={state.open} onOpenChange={(o) => { if (!o) close(); }}>
      <DialogContent className="max-w-2xl sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Ship session</DialogTitle>
        </DialogHeader>

        {loading && (
          <div className="flex items-center gap-2 py-4">
            <Loader2 size={14} className="animate-spin text-muted-foreground" />
            <span className="text-[11px] text-muted-foreground">Loading preview...</span>
          </div>
        )}

        {!loading && ghOk === false && (
          <div className="flex items-start gap-2 rounded border border-yellow-500/40 bg-yellow-500/10 px-3 py-2 text-yellow-300">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <div className="text-[11px]">
              <p className="font-medium">gh CLI not authenticated</p>
              <p className="mt-0.5 text-yellow-300/80">Run <code className="font-mono text-[10px] bg-background/40 px-1 rounded">gh auth login</code> in a terminal to enable Ship.</p>
            </div>
          </div>
        )}

        {!loading && nothingToShip && (
          <div className="flex items-start gap-2 rounded border border-yellow-500/40 bg-yellow-500/10 px-3 py-2 text-yellow-300">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <div className="text-[11px]">
              <p className="font-medium">Nothing to ship</p>
              <p className="mt-0.5 text-yellow-300/80">No commits ahead of <code className="font-mono text-[10px] bg-background/40 px-1 rounded">{preview?.base}</code> and no staged changes. Make a commit or stage files first.</p>
            </div>
          </div>
        )}

        {!loading && hasExistingPr && existingPr && (
          <div className="flex items-start gap-2 rounded border border-blue-500/40 bg-blue-500/10 px-3 py-2 text-blue-300">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <div className="flex-1 text-[11px]">
              <p className="font-medium">PR #{existingPr.number} already exists</p>
              <p className="mt-0.5 text-blue-300/80">
                <a href={existingPr.url} target="_blank" rel="noopener noreferrer" className="underline">{existingPr.title}</a>
                {" — "}
                Ship is disabled; use Push below (or Ctrl+Enter) to update it.
              </p>
            </div>
          </div>
        )}

        {!loading && preview && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Title</label>
              <Input value={title} onChange={(e) => setTitle(e.target.value)} className="text-[13px] antialiased" />
            </div>

            <div className="flex flex-col gap-1">
              <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                Body <span className="normal-case text-muted-foreground/60">(base: {preview.base})</span>
              </label>
              <Textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                className="h-48 resize-none text-[12px] leading-relaxed antialiased"
              />
            </div>

            <div className="rounded border border-border/50 bg-background/30 p-2">
              <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                Commits to ship ({preview.commits.length})
              </span>
              <ul className="mt-1 space-y-0.5">
                {preview.commits.map((c) => (
                  <li key={c.hash} className="flex gap-2 font-mono text-[10px]">
                    <span className="text-muted-foreground/60">{c.hash}</span>
                    <span className="text-foreground/80 truncate">{c.subject}</span>
                  </li>
                ))}
                {preview.commits.length === 0 && (
                  <li className="text-[10px] text-muted-foreground">No new commits in range. Ship will push + open PR only.</li>
                )}
              </ul>
            </div>

            {unstagedEntries.length > 0 && (
              <div className="rounded border border-border/50 bg-background/30 p-2">
                <div className="flex items-center gap-2">
                  <FileDiff size={11} className="text-muted-foreground" />
                  <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                    Stage before ship ({toStage.size}/{unstagedEntries.length})
                  </span>
                  <span className="ml-auto text-[9px] text-muted-foreground/60">↑↓ move · Space toggle · Ctrl+A all</span>
                </div>
                <div className="mt-1 flex max-h-36 flex-col gap-0.5 overflow-y-auto rounded">
                  {unstagedEntries.map((entry, i) => {
                    const isCursor = i === stageCursor;
                    const isChecked = toStage.has(entry.path);
                    return (
                      <div
                        key={entry.path}
                        ref={(el) => {
                          if (el && isCursor) el.scrollIntoView({ block: "nearest" });
                        }}
                        role="button"
                        onClick={() => { setStageCursor(i); toggleStage(entry.path); }}
                        className={`flex cursor-pointer items-center gap-2 rounded px-2 py-0.5 text-left font-mono text-[10px] transition-colors ${
                          isCursor ? "bg-orange-500/15 border border-orange-500/40" : "border border-transparent hover:bg-secondary/40"
                        }`}
                      >
                        <span
                          className={`flex size-3 shrink-0 items-center justify-center rounded-sm border ${
                            isChecked
                              ? "bg-orange-500 border-orange-500 text-background"
                              : "border-muted-foreground/40 bg-transparent"
                          }`}
                        >
                          {isChecked && <Check size={9} strokeWidth={3} />}
                        </span>
                        <span className={`shrink-0 w-5 text-center ${entry.source === "untracked" ? "text-blue-400" : "text-yellow-400"}`}>
                          {entry.status}
                        </span>
                        <span className="flex-1 truncate text-foreground/85">{entry.path}</span>
                        <span className="shrink-0 text-[9px] uppercase tracking-wider text-muted-foreground/60">{entry.source}</span>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            <label className="flex cursor-pointer items-center gap-2 rounded border border-border/50 bg-background/30 px-3 py-2 hover:border-border">
              <input
                type="checkbox"
                checked={autoMerge}
                onChange={(e) => setAutoMerge(e.target.checked)}
                className="accent-green-500"
              />
              <div className="flex-1">
                <span className="text-[11px] font-medium text-foreground">Auto-merge when checks pass</span>
                <p className="text-[10px] text-muted-foreground">Delegates merge to Claude/GitHub: runs <code className="font-mono bg-background/40 px-1 rounded">gh pr merge --auto --squash</code> after PR creation. Requires auto-merge enabled on the repo.</p>
              </div>
            </label>

            {shipping && (
              <div className="flex items-center gap-4 rounded border border-border/50 bg-background/30 p-2">
                {(["commit", "push", "pr"] as const).map((stage) => {
                  const v = progress[stage];
                  return (
                    <div key={stage} className="flex items-center gap-1 text-[10px]">
                      {v === true ? <Check size={12} className="text-green-400" /> : v === false ? <AlertTriangle size={12} className="text-red-400" /> : <CircleDashed size={12} className="animate-spin text-muted-foreground" />}
                      <span className="text-muted-foreground">{stage}</span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}

        {error && (
          <div className="rounded border border-red-500/40 bg-red-500/10 px-2 py-1 text-[10px] text-red-400">{error}</div>
        )}

        <DialogFooter>
          <Button variant="secondary" onClick={close} disabled={shipping || pushing}>Cancel</Button>
          {hasExistingPr ? (
            <Button ref={confirmRef} onClick={pushOnly} disabled={pushDisabled} autoFocus>
              {pushing ? (<><Loader2 size={12} className="mr-1 animate-spin" />Pushing…</>) : "Push"}
            </Button>
          ) : (
            <Button ref={confirmRef} onClick={handleShip} disabled={disabled} autoFocus>
              {shipping ? (<><Loader2 size={12} className="mr-1 animate-spin" />Shipping…</>) : "Ship"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
