import { useCallback, type KeyboardEvent } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activePlanAtom, setPlanContentAtom } from "@/stores/plan";
import { activeSessionIdAtom } from "@/stores/workspace";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Textarea } from "@/components/ui/textarea";

export function PlanEditor() {
  const plan = useAtomValue(activePlanAtom);
  const setContent = useSetAtom(setPlanContentAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const addToast = useSetAtom(toastsAtom);

  const handleSave = useCallback(() => {
    if (!plan.path || !sessionId) {
      addToast({ message: "No plan path — cannot save", type: "error" });
      return;
    }
    invoke("save_plan", { sessionId, content: plan.content })
      .then(() => invoke("reject_plan", { sessionId }))
      .then(() => {
        addToast({ message: "Plan saved — will re-read on next prompt", type: "success" });
      })
      .catch((err: unknown) => {
        addToast({ message: `Save failed: ${String(err)}`, type: "error" });
      });
  }, [plan.content, plan.path, sessionId, addToast]);

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
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        className="h-full flex-1 resize-none rounded-none border-none font-mono text-[11px] leading-relaxed focus-visible:ring-0"
        placeholder="Edit plan markdown... (Ctrl+S to save)"
        spellCheck={false}
        aria-label="Plan editor"
      />
    </div>
  );
}
