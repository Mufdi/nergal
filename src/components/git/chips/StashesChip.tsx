import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@/lib/tauri";
import { useSetAtom } from "jotai";
import { toastsAtom } from "@/stores/toast";
import { Textarea } from "@/components/ui/textarea";
import { Kbd } from "@/components/ui/kbd";
import {
  Archive,
  ChevronRight,
  Loader2,
  GitBranch,
} from "lucide-react";
import type { StashEntry } from "@/stores/git";

interface StashesChipProps {
  sessionId: string;
}

export function StashesChip({ sessionId }: StashesChipProps) {
  const [stashes, setStashes] = useState<StashEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [cursor, setCursor] = useState(0);
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);
  const [filesByIdx, setFilesByIdx] = useState<Record<number, string[]>>({});
  const [createMsg, setCreateMsg] = useState("");
  const [creating, setCreating] = useState(false);
  const [confirmingDropIdx, setConfirmingDropIdx] = useState<number | null>(null);
  const [branchModeIdx, setBranchModeIdx] = useState<number | null>(null);
  const [branchInput, setBranchInput] = useState("");
  const branchInputRef = useRef<HTMLInputElement>(null);
  const dropConfirmTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const addToast = useSetAtom(toastsAtom);

  const refresh = useCallback(() => {
    invoke<StashEntry[]>("git_stash_list", { sessionId })
      .then((rows) => { setStashes(rows); setLoading(false); })
      .catch((err: unknown) => { addToast({ message: "Stash list failed", description: String(err), type: "error" }); setLoading(false); });
  }, [sessionId, addToast]);

  useEffect(() => { refresh(); }, [refresh]);

  useEffect(() => {
    if (cursor >= stashes.length) setCursor(Math.max(0, stashes.length - 1));
  }, [cursor, stashes.length]);

  useEffect(() => {
    if (branchModeIdx !== null) branchInputRef.current?.focus();
  }, [branchModeIdx]);

  function clearDropConfirm() {
    if (dropConfirmTimeoutRef.current) {
      clearTimeout(dropConfirmTimeoutRef.current);
      dropConfirmTimeoutRef.current = null;
    }
    setConfirmingDropIdx(null);
  }

  function toggleExpand(stashIdx: number) {
    if (expandedIdx === stashIdx) {
      setExpandedIdx(null);
      return;
    }
    setExpandedIdx(stashIdx);
    if (!filesByIdx[stashIdx]) {
      invoke<string[]>("git_stash_show", { sessionId, index: stashIdx })
        .then((files) => setFilesByIdx((prev) => ({ ...prev, [stashIdx]: files })))
        .catch(() => {});
    }
  }

  function handleCreate() {
    if (creating) return;
    setCreating(true);
    invoke<void>("git_stash_create", { sessionId, message: createMsg.trim() })
      .then(() => {
        addToast({ message: "Stash saved", description: createMsg.trim() || "WIP saved", type: "success" });
        setCreateMsg("");
        refresh();
      })
      .catch((err: unknown) => addToast({ message: "Stash failed", description: String(err), type: "error" }))
      .finally(() => setCreating(false));
  }

  function handleApply(stashIdx: number) {
    invoke<void>("git_stash_apply", { sessionId, index: stashIdx })
      .then(() => { addToast({ message: "Stash applied", description: `stash@{${stashIdx}}`, type: "success" }); refresh(); })
      .catch((err: unknown) => addToast({ message: "Apply failed", description: String(err), type: "error" }));
  }

  function handlePop(stashIdx: number) {
    invoke<void>("git_stash_pop", { sessionId, index: stashIdx })
      .then(() => { addToast({ message: "Stash popped", description: `stash@{${stashIdx}}`, type: "success" }); refresh(); })
      .catch((err: unknown) => addToast({ message: "Pop failed", description: String(err), type: "error" }));
  }

  function handleDrop(stashIdx: number) {
    invoke<void>("git_stash_drop", { sessionId, index: stashIdx })
      .then(() => { addToast({ message: "Stash dropped", description: `stash@{${stashIdx}}`, type: "info" }); refresh(); })
      .catch((err: unknown) => addToast({ message: "Drop failed", description: String(err), type: "error" }));
    clearDropConfirm();
  }

  function handleBranchSubmit(stashIdx: number) {
    const name = branchInput.trim();
    if (!name) return;
    invoke<void>("git_stash_branch", { sessionId, index: stashIdx, branchName: name })
      .then(() => { addToast({ message: "Branch created", description: `${name} from stash@{${stashIdx}}`, type: "success" }); refresh(); })
      .catch((err: unknown) => addToast({ message: "Branch failed", description: String(err), type: "error" }));
    setBranchModeIdx(null);
    setBranchInput("");
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (stashes.length === 0) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;

      if (e.code === "ArrowDown" || e.code === "KeyJ") {
        if (e.shiftKey) return;
        e.preventDefault();
        setCursor((i) => (i + 1) % stashes.length);
        clearDropConfirm();
        return;
      }
      if (e.code === "ArrowUp" || e.code === "KeyK") {
        if (e.shiftKey) return;
        e.preventDefault();
        setCursor((i) => (i - 1 + stashes.length) % stashes.length);
        clearDropConfirm();
        return;
      }
      if (e.code === "Space" && !e.shiftKey) {
        const s = stashes[cursor];
        if (!s) return;
        e.preventDefault();
        toggleExpand(s.index);
        return;
      }
      if (e.code === "Enter" && !e.shiftKey) {
        const s = stashes[cursor];
        if (!s) return;
        e.preventDefault();
        handleApply(s.index);
        return;
      }
      if (e.code === "Enter" && e.shiftKey) {
        const s = stashes[cursor];
        if (!s) return;
        e.preventDefault();
        handlePop(s.index);
        return;
      }
      if (e.code === "KeyP" && !e.shiftKey) {
        const s = stashes[cursor];
        if (!s) return;
        e.preventDefault();
        handlePop(s.index);
        return;
      }
      if (e.code === "KeyD" && !e.shiftKey) {
        const s = stashes[cursor];
        if (!s) return;
        e.preventDefault();
        if (confirmingDropIdx === s.index) {
          handleDrop(s.index);
        } else {
          setConfirmingDropIdx(s.index);
          if (dropConfirmTimeoutRef.current) clearTimeout(dropConfirmTimeoutRef.current);
          dropConfirmTimeoutRef.current = setTimeout(() => setConfirmingDropIdx(null), 3000);
        }
        return;
      }
      if (e.code === "KeyB" && !e.shiftKey) {
        const s = stashes[cursor];
        if (!s) return;
        e.preventDefault();
        setBranchModeIdx(s.index);
        clearDropConfirm();
        return;
      }
      if (e.code === "Escape") {
        clearDropConfirm();
        setBranchModeIdx(null);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stashes, cursor, confirmingDropIdx]);

  function onCreateKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleCreate();
    }
  }

  function onBranchKeyDown(e: React.KeyboardEvent<HTMLInputElement>, stashIdx: number) {
    if (e.key === "Enter") {
      e.preventDefault();
      handleBranchSubmit(stashIdx);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setBranchModeIdx(null);
      setBranchInput("");
    }
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center gap-2">
        <Loader2 size={14} className="animate-spin text-muted-foreground" />
        <span className="text-[11px] text-muted-foreground">Loading stashes...</span>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border/40 px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Stashes ({stashes.length})
        </span>
      </div>

      <div className="flex-1 overflow-y-auto">
        {stashes.length === 0 ? (
          <div className="flex h-full items-center justify-center px-6 py-10">
            <div className="text-center">
              <Archive size={20} className="mx-auto mb-2 text-muted-foreground/40" />
              <p className="text-[11px] text-muted-foreground/80">No stashes yet</p>
              <p className="mt-1 text-[10px] text-muted-foreground/50">Save current changes with the input below</p>
            </div>
          </div>
        ) : (
          stashes.map((s, i) => {
            const isCursor = cursor === i;
            const isExpanded = expandedIdx === s.index;
            const isConfirmingDrop = confirmingDropIdx === s.index;
            const isBranchMode = branchModeIdx === s.index;
            const files = filesByIdx[s.index] ?? [];
            return (
              <div key={s.index}>
                <div
                  ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
                  onMouseEnter={() => setCursor(i)}
                  onClick={() => toggleExpand(s.index)}
                  className={`flex items-center gap-2 px-3 py-1 transition-colors cursor-pointer border-l-2 ${
                    isCursor
                      ? "border-l-orange-500 bg-orange-500/10"
                      : isExpanded
                      ? "border-l-transparent bg-secondary/40"
                      : "border-l-transparent hover:bg-secondary/30"
                  }`}
                >
                  <ChevronRight
                    size={10}
                    className={`shrink-0 text-muted-foreground/50 transition-transform ${isExpanded ? "rotate-90" : ""}`}
                  />
                  <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">
                    stash@{`{${s.index}}`}
                  </span>
                  {s.branch && (
                    <span className="shrink-0 flex items-center gap-0.5 rounded bg-secondary/60 px-1 py-0.5 font-mono text-[9px] text-muted-foreground">
                      <GitBranch size={8} />
                      {s.branch}
                    </span>
                  )}
                  <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80">{s.message}</span>
                  <span className="shrink-0 text-[9px] text-muted-foreground/50">{s.age}</span>
                </div>

                {isExpanded && (
                  <div className="border-b border-border/30 bg-card/40 px-3 py-1">
                    {files.length === 0 ? (
                      <span className="text-[10px] text-muted-foreground/50">Loading files...</span>
                    ) : (
                      <ul className="ml-3">
                        {files.map((f) => (
                          <li key={f} className="flex items-center gap-1.5 py-0.5 text-[10px] text-muted-foreground">
                            <span className="size-1 shrink-0 rounded-full bg-muted-foreground/40" />
                            <span className="truncate">{f}</span>
                          </li>
                        ))}
                      </ul>
                    )}
                    {isBranchMode ? (
                      <div className="mt-2 flex items-center gap-1.5">
                        <span className="text-[10px] text-muted-foreground">New branch:</span>
                        <input
                          ref={branchInputRef}
                          type="text"
                          value={branchInput}
                          onChange={(e) => setBranchInput(e.target.value)}
                          onKeyDown={(e) => onBranchKeyDown(e, s.index)}
                          placeholder="branch-name"
                          className="h-5 flex-1 rounded border border-border/50 bg-background px-1.5 font-mono text-[10px] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
                        />
                        <Kbd keys="enter" />
                        <Kbd keys="esc" />
                      </div>
                    ) : (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5">
                        <button
                          onClick={(e) => { e.stopPropagation(); handleApply(s.index); }}
                          className="flex h-5 items-center gap-1 rounded bg-secondary px-2 text-[10px] text-foreground hover:bg-secondary/80"
                        >
                          Apply <Kbd keys="enter" />
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); handlePop(s.index); }}
                          className="flex h-5 items-center gap-1 rounded bg-primary/15 px-2 text-[10px] text-primary hover:bg-primary/25"
                        >
                          Pop <Kbd keys="p" />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            if (isConfirmingDrop) handleDrop(s.index);
                            else {
                              setConfirmingDropIdx(s.index);
                              if (dropConfirmTimeoutRef.current) clearTimeout(dropConfirmTimeoutRef.current);
                              dropConfirmTimeoutRef.current = setTimeout(() => setConfirmingDropIdx(null), 3000);
                            }
                          }}
                          className={`flex h-5 items-center gap-1 rounded px-2 text-[10px] transition-colors ${
                            isConfirmingDrop
                              ? "bg-red-500/30 text-red-200"
                              : "bg-red-500/10 text-red-400 hover:bg-red-500/20"
                          }`}
                        >
                          {isConfirmingDrop ? "Confirm drop" : "Drop"} <Kbd keys="d" />
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); setBranchModeIdx(s.index); }}
                          className="flex h-5 items-center gap-1 rounded bg-secondary px-2 text-[10px] text-muted-foreground hover:text-foreground"
                        >
                          Branch <Kbd keys="b" />
                        </button>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      <div className="shrink-0 border-t border-border/50 p-2">
        <Textarea
          value={createMsg}
          onChange={(e) => setCreateMsg(e.target.value)}
          onKeyDown={onCreateKeyDown}
          placeholder="Stash message (optional)... (Ctrl+Enter to save)"
          className="mb-2 h-12 resize-none rounded border-border/50 bg-background font-mono text-[11px] leading-relaxed focus-visible:ring-1"
          spellCheck={false}
        />
        <button
          onClick={handleCreate}
          disabled={creating}
          className="flex h-6 w-full items-center justify-center gap-1.5 rounded bg-primary text-[11px] font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
        >
          {creating ? <Loader2 size={10} className="animate-spin" /> : <Archive size={10} />}
          Save stash <Kbd keys="ctrl+enter" />
        </button>
        {stashes.length > 0 && (
          <div className="mt-1 flex flex-wrap items-center gap-1 text-[9px] text-muted-foreground/60">
            <Kbd keys="enter" /> apply · <Kbd keys="p" /> pop · <Kbd keys="d" /> drop · <Kbd keys="b" /> branch · <Kbd keys="space" /> expand
          </div>
        )}
      </div>
    </div>
  );
}
