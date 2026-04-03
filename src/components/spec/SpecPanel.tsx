import { useState, useEffect, useCallback, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { currentSpecArtifactAtom, specSubTabMapAtom } from "@/stores/rightPanel";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { RichMarkdownEditor } from "@/components/editor/RichMarkdownEditor";
import { FileText, Pencil, CheckSquare, ClipboardList, Wrench, Cog, ArrowLeft } from "lucide-react";

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
  const [editContent, setEditContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [change, setChange] = useState<OpenSpecChange | null>(null);
  const [activeSpec, setActiveSpec] = useState<string | null>(initialSpecPath ?? null);
  const [mode, setMode] = useState<"view" | "edit">("view");
  const [saving, setSaving] = useState(false);

  const setCurrentSpecArtifact = useSetAtom(currentSpecArtifactAtom);
  const isEditable = change?.status === "active";
  const dirty = mode === "edit" && editContent !== content;

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

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

  useEffect(() => {
    if (currentArtifactPath) {
      setCurrentSpecArtifact({ changeName, artifactPath: currentArtifactPath });
    }
    return () => setCurrentSpecArtifact(null);
  }, [changeName, currentArtifactPath, setCurrentSpecArtifact]);

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
        setEditContent(text);
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
        setEditContent("");
        setLoading(false);
      }
    } else {
      loadArtifact(`${activeTab}.md`);
    }
    setMode("view");
  }, [activeTab, activeSpec, loadArtifact]);

  function handleSave() {
    if (!currentArtifactPath || !dirty) return;
    setSaving(true);
    invoke("write_openspec_artifact", {
      sessionId,
      changeName,
      artifactPath: currentArtifactPath,
      content: editContent,
    })
      .then(() => {
        setContent(editContent);
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setSaving(false));
  }

  const handleSaveCallback = useCallback(() => {
    handleSave();
  }, [currentArtifactPath, editContent, changeName, sessionId]);

  function handleSpecClick(specPath: string) {
    setActiveSpec(specPath);
    setActiveTab("specs");
  }

  function handleBackToSpecs() {
    setActiveSpec(null);
    setContent("");
    setEditContent("");
    setLoading(false);
    setMode("view");
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

      if (e.key === "Backspace" && !e.ctrlKey && !e.altKey && activeSpec && mode !== "edit") {
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
  }, [activeSpec, mode, activeTab, tabs.length]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Change header */}
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
          <span className="text-[10px] text-muted-foreground ml-auto shrink-0">{formatDate(change.created)}</span>
        )}
      </div>

      {/* Artifact tab bar + View/Edit toggle */}
      {(tabs.length > 1 || isEditable) && (
        <div className="flex shrink-0 items-center border-b border-border/50">
          {tabs.length > 1 && (
            <div className="flex flex-1 items-center gap-1 overflow-x-auto scrollbar-none px-2 py-1.5">
              {tabs.map((tab) => {
                const Icon = tab.icon;
                const isActive = activeTab === tab.key;
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
                  </button>
                );
              })}
            </div>
          )}
          {tabs.length <= 1 && <div className="flex-1" />}

          {/* View/Edit toggle — only for active changes when showing content */}
          {isEditable && showingContent && (
            <div className="flex shrink-0 items-center gap-1 pr-2">
              {mode === "edit" && (
                <button
                  onClick={handleSave}
                  disabled={!dirty || saving}
                  className={`h-5 rounded px-2 text-[10px] font-medium transition-colors ${
                    dirty && !saving
                      ? "bg-primary text-primary-foreground hover:bg-primary/90"
                      : "bg-secondary text-muted-foreground cursor-not-allowed"
                  }`}
                >
                  {saving ? "Saving..." : "Save"}
                </button>
              )}
              <button
                onClick={() => setMode("view")}
                className={`h-5 rounded px-1.5 text-[10px] transition-colors ${
                  mode === "view" ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"
                }`}
              >
                View
              </button>
              <button
                onClick={() => setMode("edit")}
                className={`h-5 rounded px-1.5 text-[10px] transition-colors ${
                  mode === "edit" ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"
                }`}
              >
                Edit
              </button>
            </div>
          )}
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
        ) : mode === "edit" && isEditable ? (
          <RichMarkdownEditor
            markdown={editContent}
            onChange={setEditContent}
            onSave={handleSaveCallback}
            placeholder="Edit artifact..."
          />
        ) : (
          <div className="h-full overflow-y-auto">
            {activeTab === "specs" && activeSpec && (
              <button
                onClick={handleBackToSpecs}
                className="flex items-center gap-1 px-4 pt-2 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
              >
                <ArrowLeft size={12} />
                All specs
              </button>
            )}
            <MarkdownView content={content} />
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
