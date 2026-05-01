import { useEffect } from "react";
import {
  FileText,
  GitCommitHorizontal,
  Archive,
  GitPullRequest,
  AlertTriangle,
} from "lucide-react";
import { CHIP_ORDER, type ChipMode } from "@/stores/git";

interface ChipStripProps {
  active: ChipMode;
  conflictCount: number;
  prCount: number;
  stashCount: number;
  onSelect: (mode: ChipMode) => void;
}

const CHIP_META: Record<ChipMode, { label: string; icon: typeof FileText }> = {
  files: { label: "Files", icon: FileText },
  history: { label: "History", icon: GitCommitHorizontal },
  stashes: { label: "Stashes", icon: Archive },
  prs: { label: "PRs", icon: GitPullRequest },
  conflicts: { label: "Conflicts", icon: AlertTriangle },
};

export function ChipStrip({ active, conflictCount, prCount, stashCount, onSelect }: ChipStripProps) {
  /// Shift+←/→ cycles between chips. Mirrors SpecPanel.tsx:380.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (!e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return;
      if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
      e.preventDefault();
      const idx = CHIP_ORDER.indexOf(active);
      if (idx === -1) return;
      const next = e.code === "ArrowRight"
        ? CHIP_ORDER[(idx + 1) % CHIP_ORDER.length]
        : CHIP_ORDER[(idx - 1 + CHIP_ORDER.length) % CHIP_ORDER.length];
      onSelect(next);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, onSelect]);

  return (
    <div className="flex shrink-0 items-center gap-0.5 border-b border-border/50 px-2 py-1">
      {CHIP_ORDER.map((mode) => {
        const { label, icon: Icon } = CHIP_META[mode];
        const isActive = active === mode;
        const count = mode === "conflicts" ? conflictCount : mode === "prs" ? prCount : mode === "stashes" ? stashCount : 0;
        const conflictHot = mode === "conflicts" && conflictCount > 0;
        const dim = mode === "conflicts" && conflictCount === 0 && !isActive;
        return (
          <button
            key={mode}
            onClick={() => onSelect(mode)}
            className={`relative flex h-6 flex-1 items-center justify-center gap-1 rounded text-[10px] font-medium transition-all ${
              isActive
                ? conflictHot
                  ? "bg-red-500/20 text-red-300 ring-1 ring-red-500/60"
                  : "bg-secondary text-foreground"
                : conflictHot
                ? "bg-red-500/10 text-red-400 ring-1 ring-red-500/40 animate-pulse hover:bg-red-500/15"
                : dim
                ? "text-muted-foreground/40 hover:text-muted-foreground hover:bg-secondary/30"
                : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
            }`}
            aria-pressed={isActive}
            title={`${label} (Shift+←/→)`}
          >
            <Icon size={11} className={conflictHot && !isActive ? "" : ""} />
            <span>{label}</span>
            {count > 0 && (
              <span className={`tabular-nums text-[9px] ${conflictHot ? "text-red-300" : "text-muted-foreground/70"}`}>
                ({count})
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
