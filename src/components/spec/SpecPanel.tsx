import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { currentSpecArtifactAtom, specSubTabMapAtom } from "@/stores/rightPanel";
import {
  activeAnnotationsAtom,
  addAnnotationAtom,
  annotationModeAtom,
  annotationScopeAtom,
  clearAnnotationsAtom,
  serializeSpecAnnotations,
  specAnnotationMapAtom,
  type AnnotationType,
} from "@/stores/annotations";
import { toastsAtom } from "@/stores/toast";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { AnnotatableMarkdownView } from "@/components/plan/AnnotatableMarkdownView";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import { FileText, Pencil, CheckSquare, ClipboardList, Wrench, Cog, ArrowLeft, Highlighter, MessageSquare, Trash2 } from "lucide-react";

interface SpecEntry {
  name: string;
  path: string;
}

interface OpenSpecChange {
  name: string;
  status: string;
  created: string;
  artifacts: string[];
  specs: SpecEntry[];
}

const ARTIFACT_ICONS: Record<string, typeof FileText> = {
  proposal: FileText,
  design: Pencil,
  tasks: CheckSquare,
  implementation: Wrench,
  specs: ClipboardList,
};

function artifactIcon(name: string) {
  return ARTIFACT_ICONS[name] ?? Cog;
}

function artifactLabel(name: string): string {
  return name.charAt(0).toUpperCase() + name.slice(1);
}

function formatDate(dateStr: string): string {
  if (!dateStr) return "";
  const match = dateStr.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (match) return `${match[3]}-${match[2]}-${match[1]}`;
  return dateStr;
}

interface SpecPanelProps {
  changeName: string;
  sessionId: string;
  initialSpecPath?: string;
  onDirtyChange?: (dirty: boolean) => void;
}

export function SpecPanel({ changeName, sessionId, initialSpecPath, onDirtyChange }: SpecPanelProps) {
  const isMaster = changeName === "_master";
  const specSubTabMap = useAtomValue(specSubTabMapAtom);
  const setSpecSubTabMap = useSetAtom(specSubTabMapAtom);
  const defaultTab = isMaster || initialSpecPath ? "specs" : "proposal";
  const activeTab = specSubTabMap[changeName] ?? defaultTab;
  const setActiveTab = useCallback((tab: string) => {
    setSpecSubTabMap((prev) => ({ ...prev, [changeName]: tab }));
  }, [changeName, setSpecSubTabMap]);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [change, setChange] = useState<OpenSpecChange | null>(null);
  const [activeSpec, setActiveSpec] = useState<string | null>(initialSpecPath ?? null);

  const setCurrentSpecArtifact = useSetAtom(currentSpecArtifactAtom);

  // Annotation state
  const [annotationMode, setAnnotationMode] = useAtom(annotationModeAtom);
  const [scope, setScope] = useAtom(annotationScopeAtom);
  const annotations = useAtomValue(activeAnnotationsAtom);
  const clearAnnotations = useSetAtom(clearAnnotationsAtom);
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const setSpecAnnotationMap = useSetAtom(specAnnotationMapAtom);
  const addToast = useSetAtom(toastsAtom);
  const [showGlobalInput, setShowGlobalInput] = useState(false);
  const [globalComment, setGlobalComment] = useState("");
  const globalInputRef = useRef<HTMLTextAreaElement>(null);

  // Annotation counts per spec_key across the whole change (or master)
  const [fileCounts, setFileCounts] = useState<Record<string, number>>({});

  useEffect(() => {
    onDirtyChange?.(false);
  }, [onDirtyChange]);

  useEffect(() => {
    invoke<OpenSpecChange[]>("list_openspec_changes", { sessionId })
      .then((changes) => {
        const found = changes.find((c) => c.name === changeName);
        if (found) {
          setChange(found);
          // If current tab doesn't exist in artifacts, switch to first available
          if (found.artifacts.length > 0 && !found.artifacts.includes(activeTab) && activeTab !== "specs") {
            setActiveTab(found.artifacts[0]);
          }
        }
      })
      .catch(() => {});
  }, [changeName, sessionId]);

  const currentArtifactPath = activeTab === "specs"
    ? activeSpec
    : `${activeTab}.md`;

  const specScopeKey = currentArtifactPath ? `${changeName}/${currentArtifactPath}` : null;
  const isAnnotatingThis =
    annotationMode && scope?.kind === "spec" && scope.specPath === specScopeKey;
  const isEditable = change?.status === "active";

  const countPrefix = isMaster ? "" : `${changeName}/`;
  const refreshCounts = useCallback(() => {
    if (isMaster) {
      setFileCounts({});
      return;
    }
    invoke<Array<[string, number]>>("count_spec_annotations_by_prefix", { prefix: countPrefix })
      .then((pairs) => {
        const map: Record<string, number> = {};
        for (const [key, count] of pairs) map[key] = count;
        setFileCounts(map);
      })
      .catch(() => {});
  }, [countPrefix, isMaster]);

  useEffect(() => { refreshCounts(); }, [refreshCounts]);

  // Keep the current file's count in sync with the local annotation list
  useEffect(() => {
    if (!specScopeKey) return;
    setFileCounts((prev) =>
      prev[specScopeKey] === annotations.length
        ? prev
        : { ...prev, [specScopeKey]: annotations.length },
    );
  }, [annotations.length, specScopeKey]);

  const totalAnnotations = Object.values(fileCounts).reduce((a, b) => a + b, 0);

  // Count for a given artifact tab key (proposal/design/tasks/…)
  const countForArtifactTab = (key: string) => fileCounts[`${changeName}/${key}.md`] ?? 0;
  // Aggregate counts for anything under specs/
  const countForSpecsTab = Object.entries(fileCounts)
    .filter(([k]) => k.startsWith(`${changeName}/specs/`))
    .reduce((acc, [, n]) => acc + n, 0);

  useEffect(() => {
    if (currentArtifactPath) {
      setCurrentSpecArtifact({ changeName, artifactPath: currentArtifactPath });
    }
    return () => setCurrentSpecArtifact(null);
  }, [changeName, currentArtifactPath, setCurrentSpecArtifact]);

  // Bind annotation scope to the current spec artifact so the drawer and
  // activeAnnotationsAtom always reflect this spec's annotations.
  useEffect(() => {
    if (!specScopeKey) return;
    setScope({ kind: "spec", specPath: specScopeKey });
    // Load persisted annotations for this spec
    invoke<Array<{
      id: string;
      ann_type: string;
      target: string;
      content: string;
      start_meta: string;
      end_meta: string;
    }>>("get_spec_annotations", { specKey: specScopeKey })
      .then((rows) => {
        const loaded = rows.map((r) => ({
          id: r.id,
          type: r.ann_type as AnnotationType,
          target: r.target,
          content: r.content,
          startMeta: JSON.parse(r.start_meta || "{}"),
          endMeta: JSON.parse(r.end_meta || "{}"),
        }));
        setSpecAnnotationMap((prev) => ({ ...prev, [specScopeKey]: loaded }));
      })
      .catch(console.error);
  }, [specScopeKey, setScope, setSpecAnnotationMap]);

  // Release spec scope + annotation mode on unmount so plan panels don't
  // inherit a stale spec scope or mode flag.
  useEffect(() => {
    return () => {
      setScope(null);
      setAnnotationMode(false);
    };
  }, [setScope, setAnnotationMode]);

  // Reset annotation mode when the artifact changes (user must re-enter per artifact)
  useEffect(() => {
    setAnnotationMode(false);
    setShowGlobalInput(false);
  }, [specScopeKey, setAnnotationMode]);

  useEffect(() => {
    if (showGlobalInput) globalInputRef.current?.focus();
  }, [showGlobalInput]);

  // Listen for global shortcut events — dispatched by stores/shortcuts.ts
  useEffect(() => {
    function handleToggleAnnotation() {
      if (!isEditable || !specScopeKey) return;
      if (isAnnotatingThis) {
        setAnnotationMode(false);
        setScope(null);
        setShowGlobalInput(false);
      } else {
        setScope({ kind: "spec", specPath: specScopeKey });
        setAnnotationMode(true);
      }
    }
    function handleToggleGlobal() {
      if (isAnnotatingThis) setShowGlobalInput((prev) => !prev);
    }
    function handleClear() {
      if (isAnnotatingThis) handleClearAnnotations();
    }
    function handleRevise() {
      if (isAnnotatingThis && annotations.length > 0) handleSendToClaude();
    }

    document.addEventListener("cluihud:toggle-annotation-mode", handleToggleAnnotation);
    document.addEventListener("cluihud:toggle-global-comment", handleToggleGlobal);
    document.addEventListener("cluihud:clear-annotations", handleClear);
    document.addEventListener("cluihud:revise-plan", handleRevise);
    return () => {
      document.removeEventListener("cluihud:toggle-annotation-mode", handleToggleAnnotation);
      document.removeEventListener("cluihud:toggle-global-comment", handleToggleGlobal);
      document.removeEventListener("cluihud:clear-annotations", handleClear);
      document.removeEventListener("cluihud:revise-plan", handleRevise);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isEditable, specScopeKey, isAnnotatingThis, annotations.length]);

  function toggleAnnotationMode() {
    if (!isEditable || !specScopeKey) return;
    if (isAnnotatingThis) {
      setAnnotationMode(false);
      setScope(null);
      setShowGlobalInput(false);
      return;
    }
    setScope({ kind: "spec", specPath: specScopeKey });
    setAnnotationMode(true);
  }

  function handleGlobalComment() {
    if (showGlobalInput) {
      if (globalComment.trim()) {
        addAnnotation({
          type: "comment",
          target: "[global]",
          content: globalComment.trim(),
          startMeta: { parentTagName: "", parentIndex: 0, textOffset: 0 },
          endMeta: { parentTagName: "", parentIndex: 0, textOffset: 0 },
        });
        setGlobalComment("");
      }
      setShowGlobalInput(false);
    } else {
      setShowGlobalInput(true);
    }
  }

  function handleSendToClaude() {
    if (!specScopeKey || !currentArtifactPath || annotations.length === 0) return;
    const feedback = serializeSpecAnnotations(annotations, {
      changeName,
      artifactPath: currentArtifactPath,
      isMaster,
    });
    invoke<void>("set_pending_annotations", { feedback })
      .then(() => {
        clearAnnotations();
        setAnnotationMode(false);
        setScope(null);
        addToast({
          message: "Feedback queued",
          description: "Will be sent in your next prompt",
          type: "success",
        });
      })
      .catch((err: unknown) =>
        addToast({ message: "Send failed", description: String(err), type: "error" }),
      );
  }

  function handleClearAnnotations() {
    clearAnnotations();
    addToast({ message: "Cleared", description: "All annotations removed", type: "info" });
  }

  const loadArtifact = useCallback((path: string) => {
    setLoading(true);
    setError(null);
    invoke<string>("read_openspec_artifact", {
      sessionId,
      changeName,
      artifactPath: path,
    })
      .then((text) => {
        setContent(text);
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [sessionId, changeName]);

  useEffect(() => {
    if (activeTab === "specs") {
      if (activeSpec) {
        loadArtifact(activeSpec);
      } else {
        setContent("");
        setLoading(false);
      }
    } else {
      loadArtifact(`${activeTab}.md`);
    }
  }, [activeTab, activeSpec, loadArtifact]);

  function handleSpecClick(specPath: string) {
    setActiveSpec(specPath);
    setActiveTab("specs");
  }

  function handleBackToSpecs() {
    setActiveSpec(null);
    setContent("");
    setLoading(false);
  }

  function handleTabSwitch(key: string) {
    setActiveTab(key);
    if (key !== "specs") setActiveSpec(null);
  }

  // Build tabs dynamically from artifacts + specs
  const tabs: { key: string; label: string; icon: typeof FileText }[] = [];
  if (!isMaster && change) {
    for (const artifact of change.artifacts) {
      tabs.push({ key: artifact, label: artifactLabel(artifact), icon: artifactIcon(artifact) });
    }
    if (change.specs.length > 0) {
      tabs.push({ key: "specs", label: "Specs", icon: ClipboardList });
    }
  } else if (isMaster) {
    tabs.push({ key: "specs", label: "Specs", icon: ClipboardList });
  }

  const displayName = isMaster ? "Consolidated Specs" : changeName;
  const showingContent = activeTab !== "specs" || activeSpec !== null;

  // Keyboard navigation: Backspace to go back, Shift+Left/Right to cycle sub-tabs
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const active = document.activeElement;
      if (active?.tagName === "INPUT" || active?.tagName === "TEXTAREA") return;

      if (e.key === "Backspace" && !e.ctrlKey && !e.altKey && activeSpec) {
        e.preventDefault();
        handleBackToSpecs();
        return;
      }

      if (e.shiftKey && !e.ctrlKey && !e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        if (tabs.length <= 1) return;
        e.preventDefault();
        const currentIdx = tabs.findIndex((t) => t.key === activeTab);
        if (currentIdx === -1) return;
        const nextIdx = e.key === "ArrowRight"
          ? (currentIdx + 1) % tabs.length
          : (currentIdx - 1 + tabs.length) % tabs.length;
        handleTabSwitch(tabs[nextIdx].key);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeSpec, activeTab, tabs.length]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Change header: metadata (left) + annotation toolbar (right) */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1">
        <span className="text-[11px] font-medium text-foreground/80 font-mono truncate">{displayName}</span>
        {change && !isMaster && (
          <span className={`text-[10px] px-1.5 py-0.5 rounded shrink-0 ${
            change.status === "active"
              ? "bg-green-500/15 text-green-400"
              : "bg-muted text-muted-foreground"
          }`}>
            {change.status}
          </span>
        )}
        {change?.created && (
          <span className="text-[10px] text-muted-foreground shrink-0">{formatDate(change.created)}</span>
        )}

        {isEditable && showingContent && (
          <div className="ml-auto flex shrink-0 items-center gap-1.5">
            {totalAnnotations > 0 && (
              <Tooltip>
                <TooltipTrigger>
                  <span className="flex h-5 items-center gap-1 rounded bg-secondary/60 px-1.5 text-[10px] text-muted-foreground">
                    <MessageSquare size={10} />
                    <span className="tabular-nums">{totalAnnotations}</span>
                  </span>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">
                  {totalAnnotations} annotation{totalAnnotations !== 1 ? "s" : ""} across this change
                </TooltipContent>
              </Tooltip>
            )}
            <Tooltip>
              <TooltipTrigger>
                <button
                  onClick={toggleAnnotationMode}
                  className={`flex h-5 items-center gap-1 rounded px-1.5 text-[10px] transition-colors ${
                    isAnnotatingThis
                      ? "bg-primary/15 text-primary"
                      : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
                >
                  <Highlighter size={11} className={isAnnotatingThis ? "animate-pulse" : ""} />
                  {isAnnotatingThis && (
                    <span className="text-[9px] font-medium uppercase tracking-wider">Annotate</span>
                  )}
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="text-[10px]">
                Toggle annotation mode (Ctrl+Shift+H)
              </TooltipContent>
            </Tooltip>

            {isAnnotatingThis && (
              <>
                <span className="text-[10px] text-muted-foreground tabular-nums">
                  {annotations.length}
                </span>
                <Tooltip>
                  <TooltipTrigger>
                    <button
                      onClick={handleGlobalComment}
                      className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                    >
                      <MessageSquare size={11} />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="text-[10px]">
                    Global comment (Ctrl+Shift+O)
                  </TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger>
                    <button
                      onClick={handleClearAnnotations}
                      className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                    >
                      <Trash2 size={11} />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="text-[10px]">
                    Clear all (Ctrl+Shift+X)
                  </TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger>
                    <button
                      onClick={handleSendToClaude}
                      disabled={annotations.length === 0}
                      className={`h-5 rounded px-2 text-[10px] font-medium transition-colors ${
                        annotations.length > 0
                          ? "bg-primary text-primary-foreground hover:bg-primary/90"
                          : "bg-secondary text-muted-foreground cursor-not-allowed"
                      }`}
                    >
                      Send to Claude
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="text-[10px]">
                    Queue feedback for next prompt (Ctrl+Shift+R)
                  </TooltipContent>
                </Tooltip>
              </>
            )}
          </div>
        )}
      </div>

      {/* Artifact tab bar */}
      {tabs.length > 1 && (
        <div className="flex shrink-0 items-center border-b border-border/50">
          <div className="flex flex-1 items-center gap-1 overflow-x-auto scrollbar-none px-2 py-1.5">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.key;
              const annCount = tab.key === "specs" ? countForSpecsTab : countForArtifactTab(tab.key);
              return (
                <button
                  key={tab.key}
                  onClick={() => handleTabSwitch(tab.key)}
                  className={`flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] whitespace-nowrap transition-colors ${
                    isActive
                      ? "bg-secondary text-foreground"
                      : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground/80"
                  }`}
                >
                  <Icon size={11} />
                  {tab.label}
                  {tab.key === "specs" && change && (
                    <span className="text-[10px] text-muted-foreground/60">{change.specs.length}</span>
                  )}
                  {annCount > 0 && (
                    <span
                      className="flex h-3.5 items-center justify-center rounded-full bg-primary/15 px-1 text-[9px] font-medium text-primary tabular-nums"
                      title={`${annCount} annotation${annCount !== 1 ? "s" : ""}`}
                    >
                      {annCount}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Global comment input */}
      {isAnnotatingThis && showGlobalInput && (
        <div className="border-b border-border/50 p-2">
          <textarea
            ref={globalInputRef}
            value={globalComment}
            onChange={(e) => setGlobalComment(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.ctrlKey) {
                e.preventDefault();
                handleGlobalComment();
              }
              if (e.key === "Escape") {
                setShowGlobalInput(false);
                setGlobalComment("");
              }
            }}
            placeholder="General comment about this spec..."
            className="mb-1.5 h-14 w-full resize-none rounded border border-border bg-background p-2 text-xs focus:ring-1 focus:ring-ring outline-none"
          />
          <div className="flex items-center justify-end gap-1">
            <button
              onClick={() => {
                setShowGlobalInput(false);
                setGlobalComment("");
              }}
              className="rounded px-2 py-0.5 text-[10px] text-muted-foreground hover:text-foreground"
            >
              Cancel
            </button>
            <button
              onClick={handleGlobalComment}
              disabled={!globalComment.trim()}
              className="rounded bg-primary px-2 py-0.5 text-[10px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              Add (Ctrl+Enter)
            </button>
          </div>
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        {activeTab === "specs" && !activeSpec ? (
          <div className="h-full overflow-y-auto">
            <SpecsList specs={change?.specs ?? []} onSelect={handleSpecClick} />
          </div>
        ) : loading ? (
          <div className="flex h-32 items-center justify-center">
            <span className="text-[11px] text-muted-foreground">Loading...</span>
          </div>
        ) : error ? (
          <div className="flex h-32 flex-col items-center justify-center gap-2">
            <span className="text-[11px] text-red-400">File not found</span>
            <span className="text-[10px] text-muted-foreground">This change may have been moved or archived.</span>
          </div>
        ) : (
          <div className="h-full overflow-y-auto">
            {activeTab === "specs" && activeSpec && showingContent && (
              <button
                onClick={handleBackToSpecs}
                className="flex items-center gap-1 px-4 pt-2 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
              >
                <ArrowLeft size={12} />
                All specs
              </button>
            )}
            {isAnnotatingThis ? (
              <AnnotatableMarkdownView
                content={content}
                annotationsEnabled
                annotationMode
              />
            ) : (
              <MarkdownView content={content} />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function SpecsList({ specs, onSelect }: { specs: SpecEntry[]; onSelect: (path: string) => void }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedIdxRef = useRef(0);

  useEffect(() => {
    containerRef.current?.focus();
    selectedIdxRef.current = 0;
    requestAnimationFrame(() => {
      const items = containerRef.current?.querySelectorAll("[data-nav-item]");
      if (items?.[0]) items[0].setAttribute("data-nav-selected", "true");
    });
  }, [specs]);

  function getItems(): HTMLElement[] {
    if (!containerRef.current) return [];
    return Array.from(containerRef.current.querySelectorAll("[data-nav-item]"));
  }

  function updateSelection(idx: number) {
    const items = getItems();
    for (const item of items) item.removeAttribute("data-nav-selected");
    if (items[idx]) {
      items[idx].setAttribute("data-nav-selected", "true");
      items[idx].scrollIntoView({ block: "nearest" });
    }
    selectedIdxRef.current = idx;
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    const items = getItems();
    if (items.length === 0) return;
    const idx = selectedIdxRef.current;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      updateSelection(Math.min(idx + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      updateSelection(Math.max(idx - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[idx]?.click();
    }
  }

  if (specs.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No capability specs</span>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="flex flex-col gap-0.5 p-2 outline-none"
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      {specs.map((spec) => (
        <button
          key={spec.path}
          data-nav-item
          onClick={() => onSelect(spec.path)}
          className="flex items-center gap-2 rounded-md px-3 py-2 text-left transition-colors hover:bg-secondary/50"
        >
          <ClipboardList size={13} className="shrink-0 text-muted-foreground" />
          <div className="flex flex-col min-w-0">
            <span className="text-[11px] text-foreground">{spec.name}</span>
            <span className="text-[10px] text-muted-foreground">{spec.path}</span>
          </div>
        </button>
      ))}
    </div>
  );
}
