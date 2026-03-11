import { useAtom } from "jotai";
import { planContentAtom, planModeAtom } from "@/stores/plan";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import type { PlanMode } from "@/lib/types";
import { MarkdownView } from "./MarkdownView";
import { PlanEditor } from "./PlanEditor";

interface PlanPanelProps {
  onCollapse?: () => void;
}

export function PlanPanel({ onCollapse }: PlanPanelProps) {
  const [mode, setMode] = useAtom(planModeAtom);
  const [content] = useAtom(planContentAtom);

  const hasPlan = content.length > 0;

  if (!hasPlan) {
    return (
      <section className="flex h-full w-full flex-col" aria-label="Plan">
        <header className="flex h-9 shrink-0 items-center justify-between border-b border-border/50 px-3">
          <span className="text-xs font-medium text-muted-foreground">Plan</span>
          {onCollapse && <CollapseButton onClick={onCollapse} side="right" />}
        </header>
        <div className="flex flex-1 items-center justify-center">
          <span className="text-xs text-muted-foreground">No plan yet</span>
        </div>
      </section>
    );
  }

  return (
    <section className="flex h-full w-full flex-col" aria-label="Plan">
      <Tabs
        value={mode}
        onValueChange={(value) => setMode(value as PlanMode)}
        className="flex h-full flex-col gap-0"
      >
        <header className="flex h-9 shrink-0 items-center justify-between border-b border-border/50 px-2">
          <TabsList variant="line" className="h-full">
            <TabsTrigger value="view" className="text-xs">View</TabsTrigger>
            <TabsTrigger value="edit" className="text-xs">Edit</TabsTrigger>
          </TabsList>
          {onCollapse && <CollapseButton onClick={onCollapse} side="right" />}
        </header>

        <TabsContent value="view" className="flex-1 overflow-y-auto">
          <MarkdownView content={content} />
        </TabsContent>

        <TabsContent value="edit" className="flex-1 overflow-hidden">
          <PlanEditor />
        </TabsContent>
      </Tabs>
    </section>
  );
}

function CollapseButton({ onClick, side }: { onClick: () => void; side: "left" | "right" }) {
  return (
    <button
      onClick={onClick}
      className="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
      aria-label={side === "right" ? "Collapse panel" : "Collapse sidebar"}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        {side === "right" ? (
          <polyline points="9 18 15 12 9 6" />
        ) : (
          <polyline points="15 18 9 12 15 6" />
        )}
      </svg>
    </button>
  );
}
