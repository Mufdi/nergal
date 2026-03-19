import { useAtomValue, useSetAtom } from "jotai";
import { planDocumentsAtom, defaultPlanState, setPlanDocModeAtom } from "@/stores/plan";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import type { PlanMode } from "@/lib/types";
import { MarkdownView } from "./MarkdownView";
import { PlanEditor } from "./PlanEditor";

interface PlanPanelProps {
  path: string;
}

export function PlanPanel({ path }: PlanPanelProps) {
  const docs = useAtomValue(planDocumentsAtom);
  const plan = path ? (docs[path] ?? defaultPlanState) : defaultPlanState;
  const setMode = useSetAtom(setPlanDocModeAtom);
  const addToast = useSetAtom(toastsAtom);

  const hasPlan = plan.content.length > 0;
  const hasEdits = plan.content !== plan.original;
  const backendSessionId = plan.claudeSessionId;

  function handleSave() {
    if (!plan.path || !backendSessionId) {
      addToast({ message: "Error", description: "No plan path — cannot save", type: "error" });
      return;
    }
    invoke("save_plan", { sessionId: backendSessionId, content: plan.content })
      .then(() => invoke("reject_plan", { sessionId: backendSessionId }))
      .then(() => {
        addToast({ message: "Plan Saved", description: "Will re-read on next prompt", type: "success" });
      })
      .catch((err: unknown) => {
        addToast({ message: "Save Failed", description: String(err), type: "error" });
      });
  }

  if (!hasPlan) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No plan yet</span>
      </div>
    );
  }

  return (
    <Tabs
      value={plan.mode}
      onValueChange={(value) => setMode({ path, mode: value as PlanMode })}
      className="flex h-full flex-col gap-0"
    >
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-border/50 px-2">
        <TabsList variant="line" className="h-full">
          <TabsTrigger value="view" className="text-[11px]">View</TabsTrigger>
          <TabsTrigger value="edit" className="text-[11px]">Edit</TabsTrigger>
        </TabsList>

        {plan.mode === "edit" && (
          <button
            onClick={handleSave}
            disabled={!hasEdits}
            className={`h-5 rounded px-2 text-[10px] font-medium transition-colors ${
              hasEdits
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-secondary text-muted-foreground cursor-not-allowed"
            }`}
          >
            Save
          </button>
        )}
      </div>

      <TabsContent value="view" className="flex-1 overflow-y-auto">
        <MarkdownView content={plan.content} />
      </TabsContent>

      <TabsContent value="edit" className="flex-1 overflow-hidden">
        <PlanEditor path={path} />
      </TabsContent>
    </Tabs>
  );
}
