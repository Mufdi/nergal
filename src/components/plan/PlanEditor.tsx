import { useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { planDocumentsAtom, defaultPlanState, setPlanDocContentAtom } from "@/stores/plan";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { RichMarkdownEditor } from "@/components/editor/RichMarkdownEditor";

interface PlanEditorProps {
  path: string;
}

export function PlanEditor({ path }: PlanEditorProps) {
  const docs = useAtomValue(planDocumentsAtom);
  const plan = path ? (docs[path] ?? defaultPlanState) : defaultPlanState;
  const setContent = useSetAtom(setPlanDocContentAtom);
  const addToast = useSetAtom(toastsAtom);

  const handleSave = useCallback(() => {
    if (!plan.path || !plan.claudeSessionId) {
      addToast({ message: "Error", description: "No plan path — cannot save", type: "error" });
      return;
    }
    invoke("save_plan", { sessionId: plan.claudeSessionId, content: plan.content })
      .then(() => invoke("reject_plan", { sessionId: plan.claudeSessionId }))
      .then(() => {
        addToast({ message: "Plan Saved", description: "Will re-read on next prompt", type: "success" });
      })
      .catch((err: unknown) => {
        addToast({ message: "Save Failed", description: String(err), type: "error" });
      });
  }, [plan.content, plan.path, plan.claudeSessionId, addToast]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <RichMarkdownEditor
        markdown={plan.content}
        onChange={(value) => setContent({ path, content: value })}
        onSave={handleSave}
        placeholder="Edit plan markdown..."
      />
    </div>
  );
}
