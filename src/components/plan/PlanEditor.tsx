import { useCallback, type KeyboardEvent } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { planContentAtom, planPathAtom } from "@/stores/plan";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Textarea } from "@/components/ui/textarea";

export function PlanEditor() {
  const [content, setContent] = useAtom(planContentAtom);
  const planPath = useAtomValue(planPathAtom);
  const addToast = useSetAtom(toastsAtom);

  const handleSave = useCallback(() => {
    if (!planPath) return;
    invoke("save_plan", { content })
      .then(() => {
        addToast({ message: "Plan saved", type: "success" });
      })
      .catch((err: unknown) => {
        addToast({ message: `Save failed: ${String(err)}`, type: "error" });
      });
  }, [content, planPath, addToast]);

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSave();
    }
  }

  return (
    <div className="flex h-full flex-col">
      <Textarea
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        className="h-full flex-1 resize-none rounded-none border-none font-mono text-xs focus-visible:ring-0"
        placeholder="Edit plan markdown... (Ctrl+S to save)"
        spellCheck={false}
        aria-label="Plan editor"
      />
    </div>
  );
}
