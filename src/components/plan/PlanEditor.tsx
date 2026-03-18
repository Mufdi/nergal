import { useCallback, type KeyboardEvent } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { planDocumentsAtom, defaultPlanState, setPlanDocContentAtom } from "@/stores/plan";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Textarea } from "@/components/ui/textarea";

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
      addToast({ message: "No plan path — cannot save", type: "error" });
      return;
    }
    invoke("save_plan", { sessionId: plan.claudeSessionId, content: plan.content })
      .then(() => invoke("reject_plan", { sessionId: plan.claudeSessionId }))
      .then(() => {
        addToast({ message: "Plan saved — will re-read on next prompt", type: "success" });
      })
      .catch((err: unknown) => {
        addToast({ message: `Save failed: ${String(err)}`, type: "error" });
      });
  }, [plan.content, plan.path, plan.claudeSessionId, addToast]);

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSave();
    }
  }

  return (
    <div className="flex h-full flex-col">
      <Textarea
        value={plan.content}
        onChange={(e) => setContent({ path, content: e.target.value })}
        onKeyDown={handleKeyDown}
        className="h-full flex-1 resize-none rounded-none border-none font-mono text-[11px] leading-relaxed focus-visible:ring-0"
        placeholder="Edit plan markdown... (Ctrl+S to save)"
        spellCheck={false}
        aria-label="Plan editor"
      />
    </div>
  );
}
