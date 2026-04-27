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
import { Loader2, AlertTriangle, Check, CircleDashed, FileDiff, ArrowLeft, GitBranch, ChevronDown } from "lucide-react";
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

/// Auto-generated PR body from the commit range. Filters out merge commits
/// (`Merge branch 'X' into Y`, `Merge remote-tracking branch …`) since they
/// are noise in a PR description — the squash-merge target won't include
/// them anyway.
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
  // Auto-merge state stripped in v3 cleanup; ShipDialog full rewrite in
  // phase 3 will replace this stub with the single-pane 3-button surface.
  const [autoMerge, setAutoMerge] = useState(false);
  const workspace = useAtomValue(activeWorkspaceAtom);

  // ── Top-level lifecycle state ──
  const [loading, setLoading] = useState(false);
  const [ghOk, setGhOk] = useState<boolean | null>(null);
  const [preview, setPreview] = useState<PrPreviewData | null>(null);
  const [existingPr, setExistingPr] = useState<PrInfo | null>(null);
  const [shipping, setShipping] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [progress, setProgress] = useState<Record<string, boolean | null>>({ commit: null, push: null, pr: null });
  const [error, setError] = useState<string | null>(null);

  // ── Step 1: Stage picker ──
  const [unstagedEntries, setUnstagedEntries] = useState<UnstagedEntry[]>([]);
  const [toStage, setToStage] = useState<Set<string>>(new Set());
  const [stageCursor, setStageCursor] = useState(0);

  // ── Step 2: Commit + PR ──
  const [step, setStep] = useState<1 | 2>(1);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [branches, setBranches] = useState<string[]>([]);
  const [targetBranch, setTargetBranch] = useState("");

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
      // Only OPEN PRs gate the new Ship — once a PR is CLOSED or MERGED,
      // the branch is free to ship a new round (the merged path also
      // triggers cleanup elsewhere; closed-without-merge means the user
      // dropped that PR and is iterating).
      setExistingPr(pr && pr.state === "OPEN" ? pr : null);
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
      // Branch picker — exclude cluihud/* worktree branches (never a real
      // PR target) and the session's own branch (can't merge into self).
      const filtered = branchList.filter((b) => !b.startsWith("cluihud/"));
      setBranches(filtered);
      // Default selection: the preview's base if it's in the list, else
      // `main` if present, else the first available.
      const initial = filtered.includes(data.base)
        ? data.base
        : filtered.includes("main")
          ? "main"
          : filtered[0] ?? data.base;
      setTargetBranch(initial);
      // Always start on Step 1 — staging decisions come first.
      setStep(1);
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
      // Commit message resolution priority:
      //   1. inlineMessage typed in the GitPanel commit textarea (explicit)
      //   2. title (when there are staged files but no commits yet — using
      //      the PR title as the commit subject keeps the flow single-step)
      //   3. null (no commit needed: commits already exist in base..HEAD)
      const willCommit = (preview?.staged_count ?? 0) > 0 || toStage.size > 0;
      const noPriorCommits = (preview?.commits.length ?? 0) === 0;
      const commitMessage = state.inlineMessage
        ?? (willCommit && noPriorCommits ? title : null);
      const result = await invoke<ShipResult>("git_ship", {
        sessionId: state.sessionId,
        message: commitMessage,
        prTitle: title,
        prBody: body,
        autoMerge,
        targetBranch: targetBranch || null,
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
  }, [state.sessionId, state.inlineMessage, preview, shipping, ghOk, title, body, autoMerge, targetBranch, stageSelected, addToast, refreshGit, close]);

  const pushOnly = useCallback(async () => {
    if (!state.sessionId || pushing) return;
    setPushing(true);
    try {
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

  const advanceToStep2 = useCallback(() => setStep(2), []);
  const backToStep1 = useCallback(() => setStep(1), []);

  // Global key handler scoped to the open dialog
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!state.open) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
        return;
      }
      // Ctrl+Enter on Step 1 = skip-to-Ship using all defaults; on Step 2 = confirm.
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        if (existingPr) {
          pushOnly();
        } else if (step === 1 && !shipping) {
          // Skip-to-Ship: skip Step 2 entirely, use default title/body/target.
          handleShip();
        } else {
          handleShip();
        }
        return;
      }
      // Plain Enter on Step 1 (without input focus) = advance to Step 2.
      if (e.key === "Enter" && step === 1 && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
        const target = e.target as HTMLElement | null;
        const inField = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";
        if (inField) return;
        if (existingPr) return;
        e.preventDefault();
        e.stopPropagation();
        advanceToStep2();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [state.open, step, shipping, close, handleShip, existingPr, pushOnly, advanceToStep2]);

  const nothingToShip = preview !== null
    && preview.commits.length === 0
    && preview.staged_count === 0
    && toStage.size === 0;
  const hasExistingPr = existingPr !== null;
  // When a PR exists, there's nothing to push if the local branch has no
  // commits beyond what the PR already tracks AND nothing staged. Push
  // would be a no-op — disable the button instead of letting the user
  // hit it and see "Nothing to push" anti-feedback.
  const noCommitsAhead = hasExistingPr
    && preview !== null
    && preview.commits.length === 0
    && preview.staged_count === 0
    && toStage.size === 0;
  const disabled = shipping || loading || ghOk === false || !title.trim() || nothingToShip || hasExistingPr;
  const pushDisabled = pushing || shipping || loading || ghOk === false || noCommitsAhead;

  const toggleStage = useCallback((path: string) => {
    setToStage((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  // Step 1 keyboard nav (arrows / Space / Ctrl+A) — only active on Step 1.
  useEffect(() => {
    if (!state.open || step !== 1 || unstagedEntries.length === 0) return;
    function onStagingKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inTextField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inTextField) return;

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
  }, [state.open, step, unstagedEntries, stageCursor, toggleStage]);

  return (
    <Dialog open={state.open} onOpenChange={(o) => { if (!o) close(); }}>
      <DialogContent className="max-w-2xl sm:max-w-2xl">
        <DialogHeader>
          <div className="flex items-center justify-between">
            <DialogTitle>Ship session</DialogTitle>
            <Stepper step={step} disabled={hasExistingPr} />
          </div>
        </DialogHeader>

        {loading && (
          <div className="flex items-center gap-2 py-4">
            <Loader2 size={14} className="animate-spin text-muted-foreground" />
            <span className="text-[11px] text-muted-foreground">Loading preview...</span>
          </div>
        )}

        {!loading && ghOk === false && (
          <Banner color="yellow" title="gh CLI not authenticated">
            Run <code className="font-mono text-[10px] bg-background/40 px-1 rounded">gh auth login</code> in a terminal to enable Ship.
          </Banner>
        )}

        {!loading && nothingToShip && (
          <Banner color="yellow" title="Nothing to ship">
            No commits ahead of <code className="font-mono text-[10px] bg-background/40 px-1 rounded">{preview?.base}</code> and no staged changes. Make a commit or stage files first.
          </Banner>
        )}

        {!loading && hasExistingPr && existingPr && (
          <Banner color="blue" title={`PR #${existingPr.number} already exists`}>
            <a href={existingPr.url} target="_blank" rel="noopener noreferrer" className="underline">{existingPr.title}</a>
            {noCommitsAhead
              ? " — Nothing to push (local branch is already in sync with the PR). Make a commit first if you have changes to add."
              : <> — Ship is disabled; use Push below (or <Kbd keys="ctrl+enter" />) to update it.</>}
          </Banner>
        )}

        {!loading && preview && !hasExistingPr && step === 1 && (
          <Step1Stage
            unstagedEntries={unstagedEntries}
            toStage={toStage}
            stageCursor={stageCursor}
            setStageCursor={setStageCursor}
            toggleStage={toggleStage}
          />
        )}

        {!loading && preview && step === 2 && (
          <Step2CommitPr
            preview={preview}
            title={title}
            setTitle={setTitle}
            body={body}
            setBody={setBody}
            branches={branches}
            targetBranch={targetBranch}
            setTargetBranch={setTargetBranch}
            autoMerge={autoMerge}
            setAutoMerge={setAutoMerge}
            shipping={shipping}
            progress={progress}
            willCommitFromTitle={
              ((preview.staged_count ?? 0) > 0 || toStage.size > 0)
              && (preview.commits.length ?? 0) === 0
              && !state.inlineMessage
            }
          />
        )}

        {error && (
          <div className="rounded border border-red-500/40 bg-red-500/10 px-2 py-1 text-[10px] text-red-400">{error}</div>
        )}

        <DialogFooter>
          <Button variant="secondary" onClick={close} disabled={shipping || pushing}>
            Cancel <Kbd keys="esc" className="ml-1.5" />
          </Button>
          {hasExistingPr ? (
            <Button onClick={pushOnly} disabled={pushDisabled}>
              {pushing ? (<><Loader2 size={12} className="mr-1 animate-spin" />Pushing…</>) : noCommitsAhead ? "Nothing to push" : "Push"}
              {!noCommitsAhead && <Kbd keys="ctrl+enter" tone="onPrimary" className="ml-1.5" />}
            </Button>
          ) : step === 1 ? (
            <Button onClick={advanceToStep2} disabled={loading || ghOk === false || nothingToShip}>
              Next →
              <Kbd keys="enter" tone="onPrimary" className="ml-1.5" />
            </Button>
          ) : (
            <>
              <Button variant="secondary" onClick={backToStep1} disabled={shipping}>
                <ArrowLeft size={12} className="mr-1" /> Back
              </Button>
              <Button onClick={handleShip} disabled={disabled}>
                {shipping ? (<><Loader2 size={12} className="mr-1 animate-spin" />Shipping…</>) : "Ship"}
                <Kbd keys="ctrl+enter" tone="onPrimary" className="ml-1.5" />
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Stepper indicator ──

function Stepper({ step, disabled }: { step: 1 | 2; disabled: boolean }) {
  if (disabled) return null;
  return (
    <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
      <span className={step >= 1 ? "text-foreground" : ""}>● Stage</span>
      <span className="text-muted-foreground/30">→</span>
      <span className={step >= 2 ? "text-foreground" : ""}>{step >= 2 ? "●" : "○"} Commit + PR</span>
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

// ── Step 1: Stage picker ──

function Step1Stage({
  unstagedEntries,
  toStage,
  stageCursor,
  setStageCursor,
  toggleStage,
}: {
  unstagedEntries: UnstagedEntry[];
  toStage: Set<string>;
  stageCursor: number;
  setStageCursor: (i: number) => void;
  toggleStage: (path: string) => void;
}) {
  if (unstagedEntries.length === 0) {
    return (
      <div className="rounded border border-border/50 bg-background/30 p-3">
        <span className="text-[11px] text-muted-foreground">
          No unstaged changes. Press <Kbd keys="enter" /> to continue with already-staged files.
        </span>
      </div>
    );
  }
  return (
    <div className="rounded border border-border/50 bg-background/30 p-2">
      <div className="flex items-center gap-2">
        <FileDiff size={11} className="text-muted-foreground" />
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Stage for ship ({toStage.size}/{unstagedEntries.length})
        </span>
        <span className="ml-auto flex items-center gap-1 text-[9px] text-muted-foreground/60">
          <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> move
          <span className="ml-1">·</span>
          <Kbd keys="space" /> toggle
          <span>·</span>
          <Kbd keys="ctrl+a" /> all
          <span>·</span>
          <Kbd keys="ctrl+enter" /> ship
        </span>
      </div>
      <div className="mt-1 flex max-h-72 flex-col gap-0.5 overflow-y-auto rounded">
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
  );
}

// ── Step 2: Commit + PR form ──

function Step2CommitPr({
  preview,
  title,
  setTitle,
  body,
  setBody,
  branches,
  targetBranch,
  setTargetBranch,
  autoMerge,
  setAutoMerge,
  shipping,
  progress,
  willCommitFromTitle,
}: {
  preview: PrPreviewData;
  title: string;
  setTitle: (s: string) => void;
  body: string;
  setBody: (s: string) => void;
  branches: string[];
  targetBranch: string;
  setTargetBranch: (s: string) => void;
  autoMerge: boolean;
  setAutoMerge: (b: boolean) => void;
  shipping: boolean;
  progress: Record<string, boolean | null>;
  willCommitFromTitle: boolean;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Title
          {willCommitFromTitle && (
            <span className="ml-2 normal-case text-[10px] font-normal text-orange-400">
              · also used as commit subject for staged changes
            </span>
          )}
        </label>
        <Input value={title} onChange={(e) => setTitle(e.target.value)} className="text-[13px] antialiased" />
      </div>

      <div className="flex flex-col gap-1">
        <div className="flex items-center justify-between">
          <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Body</label>
          <BranchPicker
            branches={branches}
            value={targetBranch || preview.base}
            onChange={setTargetBranch}
          />
        </div>
        <Textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          className="h-44 resize-none text-[12px] leading-relaxed antialiased"
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

      <label className="flex cursor-pointer items-center gap-2 rounded border border-border/50 bg-background/30 px-3 py-2 hover:border-border">
        <input
          type="checkbox"
          checked={autoMerge}
          onChange={(e) => setAutoMerge(e.target.checked)}
          className="accent-green-500"
        />
        <div className="flex-1">
          <span className="text-[11px] font-medium text-foreground">Auto-merge when checks pass</span>
          <p className="text-[10px] text-muted-foreground">Runs <code className="font-mono bg-background/40 px-1 rounded">gh pr merge --auto --squash</code> after PR creation. Default persisted in config.</p>
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

  // Close on outside click + Escape; arrows navigate, Enter selects.
  // Native <select> rendered the dropdown panel via the OS and ignored
  // the dark theme — this custom popover keeps theming consistent.
  useEffect(() => {
    if (!open) return;
    // When opening, position cursor on the currently selected branch.
    setCursor(Math.max(0, branches.indexOf(value)));
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
  }, [open, branches, value, cursor, onChange]);

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
