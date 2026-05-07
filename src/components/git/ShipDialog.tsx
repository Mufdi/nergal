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
import { Kbd } from "@/components/ui/kbd";
import {
  Loader2,
  AlertTriangle,
  Check,
  CircleDashed,
  GitBranch,
  ChevronDown,
  FileDiff,
} from "lucide-react";
import { shipDialogAtom, triggerShipAtom, type ShipProgressEvent } from "@/stores/ship";
import { toastsAtom } from "@/stores/toast";
import { refreshGitInfoAtom } from "@/stores/git";
import { activeWorkspaceAtom } from "@/stores/workspace";

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
interface StageEntry {
  path: string;
  status: string;
  source: "staged" | "unstaged" | "untracked";
}

/// Tooling artefacts that show up as untracked when a worktree was
/// branched from a commit predating an updated .gitignore. Never useful
/// to stage — keep them out of the picker noise.
const TOOLING_PATH_PREFIXES = [
  ".memory-opt-out", ".worktrees/", ".claude/plans/",
  ".claude/settings.local.json", ".claude/skills/", ".claude/commands/",
  ".agent/", ".agents/", ".opencode/", ".windsurf/", ".playwright-cli/",
  "node_modules/", "dist/", "target/", "src-tauri/target/",
] as const;

function isToolingPath(p: string): boolean {
  return TOOLING_PATH_PREFIXES.some((prefix) => p === prefix || p.startsWith(prefix));
}

type Action = "commit" | "commit-push" | "commit-push-pr";

const ACTION_LABELS: Record<Action, string> = {
  commit: "Commit",
  "commit-push": "Commit + Push",
  "commit-push-pr": "Commit + Push + PR",
};

function buildBody(preview: PrPreviewData): string {
  const isMergeCommit = (subject: string) =>
    /^Merge (branch|remote-tracking branch|pull request) /.test(subject.trim());
  const realCommits = preview.commits.filter((c) => !isMergeCommit(c.subject));
  const bullets = realCommits.map((c) => `- ${c.subject}`).join("\n");
  const stat = preview.diffstat;
  const diffLine = `+${stat.added} -${stat.removed} across ${stat.files} files`;
  const stagedNote = preview.staged_count > 0
    ? `\n\n_Staged changes (${preview.staged_count} files) will be committed as part of Ship._`
    : "";
  let body = bullets.length > 0
    ? `${bullets}\n\n---\n${diffLine}${stagedNote}`
    : `${stagedNote.trim() || "_No commits yet — staged work will become the first commit._"}`;
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
  const workspace = useAtomValue(activeWorkspaceAtom);

  const [loading, setLoading] = useState(false);
  const [ghOk, setGhOk] = useState<boolean | null>(null);
  const [preview, setPreview] = useState<PrPreviewData | null>(null);
  const [existingPr, setExistingPr] = useState<PrInfo | null>(null);
  const [shipping, setShipping] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [progress, setProgress] = useState<Record<string, boolean | null>>({ commit: null, push: null, pr: null });
  const [error, setError] = useState<string | null>(null);

  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [branches, setBranches] = useState<string[]>([]);
  const [targetBranch, setTargetBranch] = useState("");

  // Stage picker state: every changed file in a unified list, with a
  // checked-set the user can flip with Space (only files in `toStage`
  // get staged before commit). Default = all checked so the common case
  // is one keystroke.
  const [stageEntries, setStageEntries] = useState<StageEntry[]>([]);
  const [toStage, setToStage] = useState<Set<string>>(new Set());
  const [stageCursor, setStageCursor] = useState(0);

  // Action arming: default to the most common path, update on hover/focus.
  const [armedAction, setArmedAction] = useState<Action>("commit-push-pr");

  useEffect(() => {
    if (trigger.tick === 0) return;
    setState({ open: true, sessionId: trigger.sessionId, inlineMessage: trigger.inlineMessage });
  }, [trigger, setState]);

  const load = useCallback(async (sessionId: string) => {
    setLoading(true);
    setError(null);
    try {
      const [gh, data, pr, status, branchList] = await Promise.all([
        invoke<boolean>("gh_available"),
        invoke<PrPreviewData>("get_pr_preview_data", { sessionId }),
        invoke<PrInfo | null>("get_pr_status", { sessionId }).catch(() => null),
        invoke<GitFullStatus>("get_git_status", { sessionId }).catch(() =>
          ({ staged: [], unstaged: [], untracked: [] }) as GitFullStatus,
        ),
        workspace
          ? invoke<string[]>("list_branches", { workspaceId: workspace.id }).catch(() => [] as string[])
          : Promise.resolve([] as string[]),
      ]);
      setGhOk(gh);
      setPreview(data);
      setExistingPr(pr && pr.state === "OPEN" ? pr : null);
      const firstSubject = data.commits[0]?.subject ?? "";
      setTitle(firstSubject);
      setBody(buildBody(data));
      // Build the unified stage entries (staged + unstaged + untracked)
      // filtered against tooling paths. Default-check everything so the
      // user can hit Ctrl+1/2/3 immediately for the common case.
      const entries: StageEntry[] = [
        ...status.staged.map((f) => ({ path: f.path, status: f.status, source: "staged" as const })),
        ...status.unstaged.map((f) => ({ path: f.path, status: f.status, source: "unstaged" as const })),
        ...status.untracked.map((p) => ({ path: p, status: "??", source: "untracked" as const })),
      ].filter((e) => !isToolingPath(e.path));
      setStageEntries(entries);
      setToStage(new Set(entries.map((e) => e.path)));
      setStageCursor(0);
      const filtered = branchList.filter((b) => !b.startsWith("cluihud/"));
      setBranches(filtered);
      const initial = filtered.includes(data.base)
        ? data.base
        : filtered.includes("main")
          ? "main"
          : filtered[0] ?? data.base;
      setTargetBranch(initial);
      // If a PR already exists, dim the PR option so the user reaches for
      // Commit+Push to update the existing PR instead.
      if (pr && pr.state === "OPEN") setArmedAction("commit-push");
      else setArmedAction("commit-push-pr");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [workspace]);

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

  // Reconcile the index against the user's stage selection. For each
  // changed file: if the user wants it staged (in `toStage`), make sure
  // it is; if they unchecked it, unstage it. This respects per-file
  // intent instead of `git add -A` which would force-include everything.
  const applyStageSelection = useCallback(async (sessionId: string) => {
    for (const entry of stageEntries) {
      const wanted = toStage.has(entry.path);
      const isStaged = entry.source === "staged";
      if (wanted && !isStaged) {
        await invoke("git_stage_file", { sessionId, path: entry.path });
      } else if (!wanted && isStaged) {
        await invoke("git_unstage_file", { sessionId, path: entry.path });
      }
    }
  }, [stageEntries, toStage]);

  // Commit-only path: stage everything → git_commit with title as subject.
  // Body is appended as the commit body (multi-line message).
  const runCommit = useCallback(async () => {
    if (!state.sessionId) return;
    if (!title.trim()) { setError("Title required for commit subject"); return; }
    setShipping(true);
    setError(null);
    try {
      await applyStageSelection(state.sessionId);
      const message = body.trim().length > 0 ? `${title.trim()}\n\n${body.trim()}` : title.trim();
      await invoke<string>("git_commit", { sessionId: state.sessionId, message });
      addToast({ message: "Committed", description: "Local commit created", type: "success" });
      refreshGit(state.sessionId);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setShipping(false);
    }
  }, [state.sessionId, title, body, applyStageSelection, addToast, refreshGit, close]);

  // Commit + Push: same as Commit, then push.
  const runCommitPush = useCallback(async () => {
    if (!state.sessionId) return;
    if (!title.trim()) { setError("Title required for commit subject"); return; }
    setShipping(true);
    setError(null);
    try {
      await applyStageSelection(state.sessionId);
      const message = body.trim().length > 0 ? `${title.trim()}\n\n${body.trim()}` : title.trim();
      // Only commit if there are staged files (avoid empty commits when the
      // user just wants to push existing local commits).
      if (toStage.size > 0 || (preview?.staged_count ?? 0) > 0) {
        await invoke<string>("git_commit", { sessionId: state.sessionId, message });
      }
      await invoke<boolean>("git_push", { sessionId: state.sessionId });
      addToast({ message: "Pushed", description: "Commit + push complete", type: "success" });
      refreshGit(state.sessionId);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setShipping(false);
    }
  }, [state.sessionId, title, body, toStage, preview, applyStageSelection, addToast, refreshGit, close]);

  // Commit + Push + PR: full ship via existing atomic backend command.
  const runCommitPushPr = useCallback(async () => {
    if (!state.sessionId || !preview || ghOk === false) return;
    if (!title.trim()) { setError("Title required for PR"); return; }
    setShipping(true);
    setError(null);
    try {
      await applyStageSelection(state.sessionId);
      const willCommit = (preview.staged_count > 0) || toStage.size > 0;
      const noPriorCommits = preview.commits.length === 0;
      const commitMessage = state.inlineMessage
        ?? (willCommit && noPriorCommits ? title : null);
      const result = await invoke<ShipResult>("git_ship", {
        sessionId: state.sessionId,
        message: commitMessage,
        prTitle: title,
        prBody: body,
        autoMerge: false,
        targetBranch: targetBranch || null,
      });
      addToast({
        message: "Shipped",
        description: `PR #${result.pr_info.number} created`,
        type: "success",
      });
      refreshGit(state.sessionId);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setShipping(false);
    }
  }, [state.sessionId, state.inlineMessage, preview, ghOk, title, body, targetBranch, toStage, applyStageSelection, addToast, refreshGit, close]);

  const pushOnly = useCallback(async () => {
    if (!state.sessionId || pushing) return;
    setPushing(true);
    try {
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
  }, [state.sessionId, pushing, addToast, refreshGit, close]);

  const dispatchAction = useCallback((action: Action) => {
    if (action === "commit") return runCommit();
    if (action === "commit-push") return runCommitPush();
    return runCommitPushPr();
  }, [runCommit, runCommitPush, runCommitPushPr]);

  const toggleStage = useCallback((path: string) => {
    setToStage((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  // Keyboard nav for the stage picker section. Active only when focus is
  // inside the picker container itself — keeps Space/arrows from hijacking
  // footer buttons or the BranchPicker dropdown.
  useEffect(() => {
    if (!state.open || stageEntries.length === 0) return;
    function onStagingKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inStagePicker = !!target?.closest("[data-stage-picker]");
      if (!inStagePicker) return;

      if (e.code === "KeyA" && (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        e.stopPropagation();
        setToStage((prev) =>
          prev.size === stageEntries.length
            ? new Set()
            : new Set(stageEntries.map((x) => x.path)),
        );
        return;
      }
      if (e.code === "Space" && !e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        const path = stageEntries[stageCursor]?.path;
        if (path) toggleStage(path);
        return;
      }
      if (e.code === "ArrowDown" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        setStageCursor((i) => (i + 1) % stageEntries.length);
        return;
      }
      if (e.code === "ArrowUp" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        setStageCursor((i) => (i - 1 + stageEntries.length) % stageEntries.length);
        return;
      }
    }
    window.addEventListener("keydown", onStagingKey, true);
    return () => window.removeEventListener("keydown", onStagingKey, true);
  }, [state.open, stageEntries, stageCursor, toggleStage]);

  // Modal-scoped keyboard:
  // - Esc closes
  // - Ctrl+1/2/3 fire the corresponding action (capture phase, so it
  //   bypasses the global session-switching shortcuts on these key combos)
  // - Ctrl+Enter fires the currently-armed action (whatever the user
  //   hovered/focused last)
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!state.open) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        if (e.code === "Digit1") {
          e.preventDefault();
          e.stopPropagation();
          if (!shipping) dispatchAction("commit");
          return;
        }
        if (e.code === "Digit2") {
          e.preventDefault();
          e.stopPropagation();
          if (!shipping) dispatchAction("commit-push");
          return;
        }
        if (e.code === "Digit3") {
          e.preventDefault();
          e.stopPropagation();
          if (!shipping && !existingPr) dispatchAction("commit-push-pr");
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          if (!shipping) dispatchAction(armedAction);
        }
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [state.open, shipping, existingPr, armedAction, close, dispatchAction]);

  // BaseUI Dialog's built-in focus trap doesn't always cycle Tab between
  // our focusables (textarea, branch picker, footer buttons). Override it
  // with a manual trap: collect all focusables inside the dialog, walk by
  // index, stopImmediatePropagation to win over BaseUI's capture handler.
  useEffect(() => {
    if (!state.open) return;
    const FOCUSABLE = 'input:not([disabled]):not([aria-disabled="true"]), textarea:not([disabled]):not([aria-disabled="true"]), button:not([disabled]):not([aria-disabled="true"]), [tabindex]:not([tabindex="-1"]):not([disabled]):not([aria-disabled="true"])';
    function handleTab(e: KeyboardEvent) {
      if (e.key !== "Tab" || e.ctrlKey || e.altKey || e.metaKey) return;
      const dialogRoot = document.querySelector<HTMLElement>('[data-slot="dialog-content"]');
      if (!dialogRoot) return;
      const target = e.target as HTMLElement | null;
      if (!target || !dialogRoot.contains(target)) return;
      const focusables = Array.from(dialogRoot.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (focusables.length === 0) return;
      const idx = focusables.indexOf(target);
      if (idx === -1) return;
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      const nextIdx = e.shiftKey
        ? (idx === 0 ? focusables.length - 1 : idx - 1)
        : (idx === focusables.length - 1 ? 0 : idx + 1);
      focusables[nextIdx].focus();
    }
    window.addEventListener("keydown", handleTab, true);
    return () => window.removeEventListener("keydown", handleTab, true);
  }, [state.open]);

  // Per-action gates. The user's flow assumption: once a commit lands
  // (no more staged/unstaged work), the canonical next step is shipping
  // to a PR — Commit and Commit+Push only make sense while there's still
  // local work to commit. Push-only-existing-commits is available via
  // the global Ctrl+Alt+P shortcut, not via this modal.
  const hasUncommitted = (preview?.staged_count ?? 0) > 0 || toStage.size > 0;
  const hasCommitsAhead = (preview?.commits.length ?? 0) > 0;
  const nothingToShip = !hasUncommitted && !hasCommitsAhead;
  const titleEmpty = !title.trim();

  const baseDisabled = shipping || loading;
  const commitDisabled = baseDisabled || titleEmpty || !hasUncommitted;
  const commitPushDisabled = baseDisabled || titleEmpty || !hasUncommitted;
  const commitPushPrDisabled = baseDisabled || titleEmpty || ghOk === false
    || existingPr !== null || nothingToShip;

  return (
    <Dialog open={state.open} onOpenChange={(o) => { if (!o) close(); }}>
      <DialogContent className="max-w-2xl sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Ship session</DialogTitle>
        </DialogHeader>

        {loading && (
          <div className="flex items-center gap-2 py-4">
            <Loader2 size={14} className="animate-spin text-muted-foreground" />
            <span className="text-[11px] text-muted-foreground">Loading…</span>
          </div>
        )}

        {!loading && (
          <div className="rounded border border-orange-500/40 bg-orange-500/10 px-3 py-2 text-[11px] text-orange-900 dark:text-orange-200">
            <div className="flex items-start gap-2">
              <AlertTriangle size={14} className="mt-0.5 shrink-0 text-orange-600 dark:text-orange-400" />
              <p>
                Ship leads to: commit → push → PR → review → merge. Once the PR is merged,
                this session is deleted (worktree, branch). Plans are archived to{" "}
                <code className="font-mono text-[10px] bg-secondary/60 px-1 rounded">.claude/plans/archive/</code>{" "}
                first.
              </p>
            </div>
          </div>
        )}

        {!loading && ghOk === false && (
          <Banner color="yellow" title="gh CLI not authenticated">
            Run <code className="font-mono text-[10px] bg-background/40 px-1 rounded">gh auth login</code> in a terminal to enable PR creation.
            Commit and Commit + Push still work.
          </Banner>
        )}

        {!loading && nothingToShip && (
          <Banner color="yellow" title="Nothing to ship">
            No commits ahead of <code className="font-mono text-[10px] bg-background/40 px-1 rounded">{preview?.base}</code> and no staged or unstaged changes.
          </Banner>
        )}

        {!loading && existingPr && (
          <Banner color="blue" title={`PR #${existingPr.number} already exists`}>
            <a href={existingPr.url} target="_blank" rel="noopener noreferrer" className="underline">{existingPr.title}</a>
            {" — "}Use Commit + Push to update the existing PR. The PR action is disabled.
          </Banner>
        )}

        {!loading && preview && stageEntries.length > 0 && (
          <StagePicker
            entries={stageEntries}
            toStage={toStage}
            cursor={stageCursor}
            setCursor={setStageCursor}
            toggle={toggleStage}
          />
        )}

        {!loading && preview && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                Title
                {(preview.staged_count > 0 || toStage.size > 0) && (
                  <span className="ml-2 normal-case text-[10px] font-normal text-orange-400">
                    · used as commit subject
                  </span>
                )}
              </label>
              <Input value={title} onChange={(e) => setTitle(e.target.value)} className="text-[13px] antialiased" />
            </div>

            <div className="flex flex-col gap-1">
              <div className="flex items-center justify-between">
                <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  Body
                  <span className="ml-2 normal-case text-[10px] font-normal text-muted-foreground/60">
                    {armedAction === "commit-push-pr" ? "PR description" : "commit body (optional)"}
                  </span>
                </label>
                {armedAction === "commit-push-pr" && (
                  <BranchPicker
                    branches={branches}
                    value={targetBranch || preview.base}
                    onChange={setTargetBranch}
                  />
                )}
              </div>
              <Textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                className="h-40 resize-none text-[12px] leading-relaxed antialiased"
              />
            </div>

            {preview.commits.length > 0 && (
              <div className="rounded border border-border/50 bg-background/30 p-2">
                <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  Commits in range ({preview.commits.length})
                </span>
                <ul className="mt-1 space-y-0.5">
                  {preview.commits.slice(0, 5).map((c) => (
                    <li key={c.hash} className="flex gap-2 font-mono text-[10px]">
                      <span className="text-muted-foreground/60">{c.hash}</span>
                      <span className="text-foreground/80 truncate">{c.subject}</span>
                    </li>
                  ))}
                  {preview.commits.length > 5 && (
                    <li className="text-[10px] text-muted-foreground">… +{preview.commits.length - 5} more</li>
                  )}
                </ul>
              </div>
            )}

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

        {/* Armed indicator + footer */}
        {!loading && (
          <div className="flex items-center justify-between border-t border-border/40 pt-2 text-[10px] text-muted-foreground">
            <span>
              About to: <span className="text-orange-400 font-medium">{ACTION_LABELS[armedAction]}</span>
              {" · "}<Kbd keys="ctrl+enter" /> fires the armed action
            </span>
          </div>
        )}

        <DialogFooter className="flex-wrap gap-1.5">
          <Button variant="secondary" onClick={close} disabled={shipping || pushing}>
            Cancel <Kbd keys="esc" className="ml-1.5" />
          </Button>
          {existingPr ? (
            <Button onClick={pushOnly} disabled={pushing || shipping || loading}>
              {pushing ? (<><Loader2 size={12} className="mr-1 animate-spin" />Pushing…</>) : "Push (update PR)"}
            </Button>
          ) : (
            <>
              <Button
                variant="secondary"
                onClick={runCommit}
                onMouseEnter={() => setArmedAction("commit")}
                onFocus={() => setArmedAction("commit")}
                disabled={commitDisabled}
              >
                Commit <Kbd keys="ctrl+1" className="ml-1.5" />
              </Button>
              <Button
                variant="secondary"
                onClick={runCommitPush}
                onMouseEnter={() => setArmedAction("commit-push")}
                onFocus={() => setArmedAction("commit-push")}
                disabled={commitPushDisabled}
              >
                Commit + Push <Kbd keys="ctrl+2" className="ml-1.5" />
              </Button>
              <Button
                onClick={runCommitPushPr}
                onMouseEnter={() => setArmedAction("commit-push-pr")}
                onFocus={() => setArmedAction("commit-push-pr")}
                disabled={commitPushPrDisabled}
              >
                {shipping ? (<><Loader2 size={12} className="mr-1 animate-spin" />Shipping…</>) : "Commit + Push + PR"}
                <Kbd keys="ctrl+3" tone="onPrimary" className="ml-1.5" />
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Stage picker ──

function StagePicker({
  entries,
  toStage,
  cursor,
  setCursor,
  toggle,
}: {
  entries: StageEntry[];
  toStage: Set<string>;
  cursor: number;
  setCursor: (i: number) => void;
  toggle: (path: string) => void;
}) {
  return (
    <div
      tabIndex={0}
      data-stage-picker
      className="rounded border border-border/50 bg-background/30 p-2 outline-none focus-visible:ring-2 focus-visible:ring-orange-500/40"
    >
      <div className="flex items-center gap-2">
        <FileDiff size={11} className="text-muted-foreground" />
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Stage for commit ({toStage.size}/{entries.length})
        </span>
        <span className="ml-auto flex items-center gap-1 text-[9px] text-muted-foreground/60">
          <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> move
          <span className="ml-1">·</span>
          <Kbd keys="space" /> toggle
          <span>·</span>
          <Kbd keys="ctrl+a" /> all
        </span>
      </div>
      <div className="mt-1 flex max-h-40 flex-col gap-0.5 overflow-y-auto rounded">
        {entries.map((entry, i) => {
          const isCursor = i === cursor;
          const isChecked = toStage.has(entry.path);
          const sourceColor =
            entry.source === "untracked" ? "text-blue-400"
            : entry.source === "staged" ? "text-green-400"
            : "text-yellow-400";
          return (
            <div
              key={entry.path}
              ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
              role="button"
              onClick={() => { setCursor(i); toggle(entry.path); }}
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
              <span className={`shrink-0 w-5 text-center ${sourceColor}`}>{entry.status}</span>
              <span className="flex-1 truncate text-foreground/85">{entry.path}</span>
              <span className="shrink-0 text-[9px] uppercase tracking-wider text-muted-foreground/60">{entry.source}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Reusable banner ──

function Banner({ color, title, children }: { color: "yellow" | "blue"; title: string; children: React.ReactNode }) {
  const palette =
    color === "yellow"
      ? "border-yellow-500/40 bg-yellow-500/10 text-yellow-300"
      : "border-blue-500/40 bg-blue-500/10 text-blue-300";
  const subtle = color === "yellow" ? "text-yellow-300/80" : "text-blue-300/80";
  return (
    <div className={`flex items-start gap-2 rounded border px-3 py-2 ${palette}`}>
      <AlertTriangle size={14} className="mt-0.5 shrink-0" />
      <div className="flex-1 text-[11px]">
        <p className="font-medium">{title}</p>
        <p className={`mt-0.5 ${subtle}`}>{children}</p>
      </div>
    </div>
  );
}

// ── Branch picker ──

function BranchPicker({
  branches,
  value,
  onChange,
}: {
  branches: string[];
  value: string;
  onChange: (s: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(0);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  // Initialize cursor only when the dropdown opens. Putting cursor in the
  // listener effect's deps caused an infinite reset loop: arrow → setCursor
  // → re-run effect → setCursor(initial) → arrows looked dead.
  useEffect(() => {
    if (!open) return;
    setCursor(Math.max(0, branches.indexOf(value)));
  }, [open, branches, value]);

  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (!wrapperRef.current?.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
        triggerRef.current?.focus();
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopPropagation();
        setCursor((i) => (i + 1) % branches.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        setCursor((i) => (i - 1 + branches.length) % branches.length);
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        const picked = branches[cursor];
        if (picked) onChange(picked);
        setOpen(false);
        triggerRef.current?.focus();
        return;
      }
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open, branches, cursor, onChange]);

  if (branches.length === 0) {
    return (
      <span className="flex items-center gap-1 text-[10px] text-muted-foreground/60">
        <GitBranch size={10} /> No remote branches
      </span>
    );
  }

  return (
    <div ref={wrapperRef} className="relative flex items-center gap-1 text-[10px] text-muted-foreground">
      <GitBranch size={10} />
      <span>PR base:</span>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex h-5 items-center gap-1 rounded border border-border/50 bg-background px-1.5 font-mono text-[10px] text-foreground/85 hover:border-border focus:outline-none focus:ring-1 focus:ring-orange-500/50"
      >
        <span className="truncate max-w-[140px]">{value || branches[0]}</span>
        <ChevronDown size={10} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div
          className="absolute right-0 top-full z-50 mt-1 max-h-56 min-w-[160px] overflow-y-auto rounded border border-border bg-popover shadow-lg"
          role="listbox"
        >
          {branches.map((b, i) => {
            const selected = b === value;
            const focused = i === cursor;
            return (
              <button
                key={b}
                type="button"
                role="option"
                aria-selected={selected}
                ref={(el) => { if (el && focused) el.scrollIntoView({ block: "nearest" }); }}
                onMouseEnter={() => setCursor(i)}
                onClick={() => { onChange(b); setOpen(false); triggerRef.current?.focus(); }}
                className={`flex w-full items-center gap-1.5 px-2 py-1 text-left font-mono text-[10px] transition-colors ${
                  focused
                    ? "bg-orange-500/15 text-foreground"
                    : selected
                      ? "bg-orange-500/5 text-foreground"
                      : "text-foreground/85 hover:bg-secondary"
                }`}
              >
                <Check size={9} className={selected ? "text-orange-400" : "opacity-0"} />
                <span className="truncate">{b}</span>
              </button>
            );
          })}
          <div className="sticky bottom-0 flex items-center justify-end gap-1 border-t border-border/40 bg-popover/95 px-2 py-1 text-[9px] text-muted-foreground/70">
            <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> move · <Kbd keys="enter" /> select
          </div>
        </div>
      )}
    </div>
  );
}
