import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MarkdownView } from "@/components/plan/MarkdownView";
import { FileText, Pencil, CheckSquare, ClipboardList, ArrowLeft } from "lucide-react";

type ArtifactTab = "proposal" | "design" | "tasks" | "specs";

interface SpecEntry {
  name: string;
  path: string;
}

interface OpenSpecChange {
  name: string;
  status: string;
  created: string;
  has_proposal: boolean;
  has_design: boolean;
  has_tasks: boolean;
  specs: SpecEntry[];
}

const ARTIFACT_TABS: { key: ArtifactTab; label: string; icon: typeof FileText; field: keyof OpenSpecChange }[] = [
  { key: "proposal", label: "Proposal", icon: FileText, field: "has_proposal" },
  { key: "design", label: "Design", icon: Pencil, field: "has_design" },
  { key: "tasks", label: "Tasks", icon: CheckSquare, field: "has_tasks" },
  { key: "specs", label: "Specs", icon: ClipboardList, field: "specs" },
];

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
}

export function SpecPanel({ changeName, sessionId, initialSpecPath }: SpecPanelProps) {
  const isMaster = changeName === "_master";
  const [activeTab, setActiveTab] = useState<ArtifactTab>(
    isMaster || initialSpecPath ? "specs" : "proposal"
  );
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [change, setChange] = useState<OpenSpecChange | null>(null);
  const [activeSpec, setActiveSpec] = useState<string | null>(initialSpecPath ?? null);

  useEffect(() => {
    invoke<OpenSpecChange[]>("list_openspec_changes", { sessionId })
      .then((changes) => {
        const found = changes.find((c) => c.name === changeName);
        if (found) setChange(found);
      })
      .catch(() => {});
  }, [changeName, sessionId]);

  const loadArtifact = useCallback((path: string) => {
    setLoading(true);
    setError(null);
    invoke<string>("read_openspec_artifact", {
      sessionId,
      changeName,
      artifactPath: path,
    })
      .then((text) => setContent(text))
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

  const availableTabs = isMaster
    ? [ARTIFACT_TABS[3]]
    : ARTIFACT_TABS.filter((tab) => {
        if (!change) return tab.key === "proposal";
        if (tab.key === "specs") return change.specs.length > 0;
        return change[tab.field as keyof OpenSpecChange] === true;
      });

  const displayName = isMaster ? "Consolidated Specs" : changeName;

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

      {/* Artifact tab bar */}
      {availableTabs.length > 1 && (
        <div className="flex shrink-0 border-b border-border/50">
          {availableTabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.key;
            return (
              <button
                key={tab.key}
                onClick={() => { setActiveTab(tab.key); if (tab.key !== "specs") setActiveSpec(null); }}
                className={`flex items-center gap-1.5 px-3 py-1 text-[11px] transition-colors ${
                  isActive
                    ? "border-b-2 border-blue-500 text-foreground"
                    : "text-muted-foreground hover:text-foreground/80"
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

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {activeTab === "specs" && !activeSpec ? (
          <SpecsList specs={change?.specs ?? []} onSelect={handleSpecClick} />
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
          <>
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
          </>
        )}
      </div>
    </div>
  );
}

function SpecsList({ specs, onSelect }: { specs: SpecEntry[]; onSelect: (path: string) => void }) {
  if (specs.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No capability specs</span>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5 p-2">
      {specs.map((spec) => (
        <button
          key={spec.path}
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
