import { useAtomValue, useSetAtom } from "jotai";
import { Plus, X } from "lucide-react";
import {
  browserActivateTabAction,
  browserCloseTabAction,
  browserNewTabAction,
  browserSessionForActiveAtom,
} from "@/stores/browser";

interface Props {
  sessionId: string;
}

export function BrowserTabStrip({ sessionId }: Props) {
  const session = useAtomValue(browserSessionForActiveAtom);
  const activate = useSetAtom(browserActivateTabAction);
  const close = useSetAtom(browserCloseTabAction);
  const newTab = useSetAtom(browserNewTabAction);

  return (
    <div className="flex items-center gap-1 border-b border-border/60 bg-card/60 px-1.5 py-1">
      <div className="flex flex-1 items-center gap-1 overflow-x-auto">
        {session.tabs.map((tab) => {
          const isActive = tab.id === session.activeTabId;
          return (
            <div
              key={tab.id}
              className={`group flex shrink-0 items-center gap-1 rounded-md border border-transparent px-2 py-0.5 text-[11px] transition-colors ${
                isActive
                  ? "border-border/60 bg-background text-foreground"
                  : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
              }`}
            >
              <button
                type="button"
                onClick={() => activate({ sessionId, tabId: tab.id })}
                onMouseDown={(e) => {
                  // Middle-click to close — standard browser convention.
                  if (e.button === 1) {
                    e.preventDefault();
                    close({ sessionId, tabId: tab.id });
                  }
                }}
                onAuxClick={(e) => {
                  // Some browsers route the middle-click to auxclick
                  // instead of mousedown. Belt-and-suspenders.
                  if (e.button === 1) {
                    e.preventDefault();
                    close({ sessionId, tabId: tab.id });
                  }
                }}
                className="max-w-44 truncate font-mono"
                title={tab.url}
              >
                {tab.label}
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  close({ sessionId, tabId: tab.id });
                }}
                className="rounded text-muted-foreground/60 opacity-0 transition group-hover:opacity-100 hover:bg-secondary hover:text-foreground"
                aria-label="Close tab"
                title="Close tab"
              >
                <X size={10} />
              </button>
            </div>
          );
        })}
      </div>
      <button
        type="button"
        onClick={() => void newTab({ sessionId })}
        className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-muted-foreground transition hover:bg-secondary/60 hover:text-foreground"
        aria-label="New tab"
        title="New tab"
      >
        <Plus size={12} />
      </button>
    </div>
  );
}
