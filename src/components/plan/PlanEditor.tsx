import { useCallback, type KeyboardEvent } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { planContentAtom, planPathAtom } from "@/stores/plan";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";

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
      <div className="flex h-7 items-center justify-between border-b border-border px-3">
        <span className="text-xs text-text-muted">Editing</span>
        <button
          onClick={handleSave}
          className="px-2 py-0.5 text-xs text-accent hover:text-text"
          aria-label="Save plan"
        >
          Save
        </button>
      </div>
      <textarea
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        className="flex-1 resize-none bg-surface p-3 font-mono text-xs text-text outline-none"
        spellCheck={false}
        aria-label="Plan editor"
      />
    </div>
  );
}
