import { useAtomValue, useSetAtom } from "jotai";
import { Plus, X } from "lucide-react";
import {
  closeScratchTab,
  createNewScratchTab,
  displayNameFor,
  MAX_TABS_SOFT,
  scratchpadActiveTabIdAtom,
  scratchpadDirtyAtom,
  scratchpadTabsAtom,
} from "@/stores/scratchpad";

export function ScratchpadTabBar() {
  const tabs = useAtomValue(scratchpadTabsAtom);
  const activeId = useAtomValue(scratchpadActiveTabIdAtom);
  const setActiveId = useSetAtom(scratchpadActiveTabIdAtom);
  const dirty = useAtomValue(scratchpadDirtyAtom);
  const overSoftCap = tabs.length >= MAX_TABS_SOFT;

  return (
    <div className="flex h-7 shrink-0 items-stretch gap-1 border-b border-border/50 px-1 overflow-x-auto">
      {tabs.map((tab) => {
        const isActive = tab.tab_id === activeId;
        const isDirty = dirty[tab.tab_id] ?? false;
        return (
          <div
            key={tab.tab_id}
            className={`group flex items-center gap-1 rounded-md px-2 text-[11px] cursor-pointer transition-colors ${
              isActive
                ? "bg-secondary/60 text-foreground"
                : "text-muted-foreground hover:text-foreground hover:bg-secondary/30"
            }`}
            onClick={() => setActiveId(tab.tab_id)}
          >
            <span className="whitespace-nowrap tabular-nums">
              {displayNameFor(tab)}
            </span>
            {isDirty && (
              <span
                aria-label="unsaved"
                className="size-1 rounded-full bg-foreground/70"
              />
            )}
            <button
              type="button"
              aria-label={`Close ${displayNameFor(tab)}`}
              className="hidden group-hover:flex size-3.5 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-red-400"
              onClick={(e) => {
                e.stopPropagation();
                void closeScratchTab(tab.tab_id);
              }}
            >
              <X size={10} />
            </button>
          </div>
        );
      })}
      <button
        type="button"
        aria-label="New scratch tab"
        className="flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-secondary/40 hover:text-foreground"
        onClick={() => void createNewScratchTab()}
      >
        <Plus size={12} />
      </button>
      {overSoftCap && (
        <span className="ml-2 self-center rounded-full bg-yellow-500/10 px-2 text-[9px] text-yellow-400">
          {tabs.length} tabs — consider closing some
        </span>
      )}
    </div>
  );
}
