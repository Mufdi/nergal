import { useState, useRef, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  activeTabsAtom,
  activeTabAtom,
  activeTabIdAtom,
  closeTabAction,
  pinTabAction,
  type Tab,
  type TabType,
} from "@/stores/rightPanel";
import {
  FileText,
  GitCompareArrows,
  ClipboardList,
  CheckSquare,
  GitBranch,
  FileCode,
  ScrollText,
  MoreHorizontal,
  X,
} from "lucide-react";

const TAB_ICONS: Record<TabType, typeof FileText> = {
  plan: FileText,
  file: FileCode,
  diff: GitCompareArrows,
  spec: ClipboardList,
  tasks: CheckSquare,
  git: GitBranch,
  transcript: ScrollText,
};

export function TabBar() {
  const tabs = useAtomValue(activeTabsAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const setActiveTabId = useSetAtom(activeTabIdAtom);
  const closeTab = useSetAtom(closeTabAction);
  const pinTab = useSetAtom(pinTabAction);

  const containerRef = useRef<HTMLDivElement>(null);
  const [overflowing, setOverflowing] = useState(false);
  const [dropdownOpen, setDropdownOpen] = useState(false);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const check = () => setOverflowing(el.scrollWidth > el.clientWidth);
    check();
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => observer.disconnect();
  }, [tabs.length]);

  if (tabs.length === 0) return null;

  return (
    <div className="flex shrink-0 items-center border-b border-border/50">
      <div
        ref={containerRef}
        className="flex flex-1 items-center overflow-x-auto scrollbar-none"
      >
        {tabs.map((tab) => (
          <TabItem
            key={tab.id}
            tab={tab}
            isActive={activeTab?.id === tab.id}
            onClick={() => setActiveTabId(tab.id)}
            onDoubleClick={() => pinTab(tab.id)}
            onClose={() => closeTab(tab.id)}
          />
        ))}
      </div>

      {overflowing && (
        <div className="relative shrink-0">
          <button
            onClick={() => setDropdownOpen((v) => !v)}
            className="flex size-7 items-center justify-center text-muted-foreground hover:text-foreground transition-colors"
            aria-label="Show all tabs"
          >
            <MoreHorizontal size={14} />
          </button>
          {dropdownOpen && (
            <>
              <div
                className="fixed inset-0 z-40"
                onClick={() => setDropdownOpen(false)}
              />
              <div className="absolute right-0 top-full z-50 mt-1 w-48 rounded-md border border-border bg-card py-1 shadow-lg">
                {tabs.map((tab) => {
                  const Icon = TAB_ICONS[tab.type];
                  return (
                    <button
                      key={tab.id}
                      onClick={() => {
                        setActiveTabId(tab.id);
                        setDropdownOpen(false);
                      }}
                      className={`flex w-full items-center gap-2 px-3 py-1.5 text-xs transition-colors ${
                        activeTab?.id === tab.id
                          ? "bg-secondary text-foreground"
                          : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
                      }`}
                    >
                      <Icon size={12} />
                      <span className="truncate">{tab.label}</span>
                    </button>
                  );
                })}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function TabItem({
  tab,
  isActive,
  onClick,
  onDoubleClick,
  onClose,
}: {
  tab: Tab;
  isActive: boolean;
  onClick: () => void;
  onDoubleClick: () => void;
  onClose: () => void;
}) {
  const Icon = TAB_ICONS[tab.type];
  return (
    <div
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      className={`group flex min-w-20 max-w-40 shrink-0 cursor-pointer items-center gap-1.5 px-2.5 py-1.5 text-xs transition-colors ${
        isActive
          ? "bg-secondary text-foreground"
          : "text-muted-foreground hover:text-foreground/80"
      }`}
    >
      <Icon size={12} className="shrink-0" />
      <span className={`truncate ${!tab.pinned ? "italic" : ""}`}>
        {tab.label}
      </span>
      <div className="ml-auto shrink-0">
        {tab.dirty ? (
          <span className="block size-1 rounded-full bg-foreground" />
        ) : (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onClose();
            }}
            className="flex size-4 items-center justify-center rounded opacity-0 transition-opacity group-hover:opacity-100 hover:bg-secondary"
            aria-label={`Close ${tab.label}`}
          >
            <X size={10} />
          </button>
        )}
      </div>
    </div>
  );
}
