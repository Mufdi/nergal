import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { planContentAtom, planModeAtom, planVisibleAtom } from "@/stores/plan";
import { terminalIdAtom } from "@/stores/session";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import type { PlanMode } from "@/lib/types";
import { MarkdownView } from "./MarkdownView";
import { PlanEditor } from "./PlanEditor";
import { DiffView } from "./DiffView";

const MODE_TABS: { mode: PlanMode; label: string }[] = [
  { mode: "view", label: "View" },
  { mode: "edit", label: "Edit" },
  { mode: "diff", label: "Diff" },
];

export function PlanPanel() {
  const [mode, setMode] = useAtom(planModeAtom);
  const content = useAtom(planContentAtom)[0];
  const setVisible = useSetAtom(planVisibleAtom);
  const addToast = useSetAtom(toastsAtom);
  const terminalId = useAtomValue(terminalIdAtom);

  function writeToPty(text: string) {
    if (!terminalId) return;
    invoke("pty_write", { id: terminalId, data: text }).catch((err: unknown) => {
      console.error("pty_write failed:", err);
    });
  }

  function handleApprove() {
    invoke("approve_plan", {})
      .then(() => {
        writeToPty("y\n");
        addToast({ message: "Plan approved", type: "success" });
        setVisible(false);
      })
      .catch((err: unknown) => {
        addToast({ message: `Approve failed: ${String(err)}`, type: "error" });
      });
  }

  function handleReject() {
    const feedback = window.prompt("Feedback for Claude:");
    if (feedback === null) return;
    invoke("reject_plan", {})
      .then(() => {
        // Write "n" to reject, then the feedback on the next prompt
        writeToPty("n\n");
        if (feedback.trim()) {
          // Small delay to let Claude process the rejection before sending feedback
          setTimeout(() => writeToPty(feedback + "\n"), 500);
        }
        addToast({ message: "Plan rejected — feedback sent", type: "info" });
        setVisible(false);
      })
      .catch((err: unknown) => {
        addToast({ message: `Reject failed: ${String(err)}`, type: "error" });
      });
  }

  return (
    <section className="flex h-full flex-col border-l border-border bg-surface" aria-label="Plan">
      <header className="flex h-8 items-center justify-between border-b border-border px-2">
        <nav className="flex gap-0" aria-label="Plan view modes">
          {MODE_TABS.map(({ mode: m, label }) => (
            <button
              key={m}
              onClick={() => setMode(m)}
              className={`px-2 py-1 text-xs ${
                mode === m ? "text-accent" : "text-text-muted hover:text-text"
              }`}
              aria-current={mode === m ? "page" : undefined}
            >
              {label}
            </button>
          ))}
        </nav>

        <button
          onClick={() => setVisible(false)}
          className="flex h-5 w-5 items-center justify-center text-text-muted hover:text-text"
          aria-label="Close plan panel"
        >
          x
        </button>
      </header>

      <div className="flex-1 overflow-y-auto">
        {mode === "view" && <MarkdownView content={content} />}
        {mode === "edit" && <PlanEditor />}
        {mode === "diff" && <DiffView />}
      </div>

      <footer className="flex items-center gap-1 border-t border-border p-2">
        <button
          onClick={handleApprove}
          className="flex-1 bg-success/20 px-2 py-1 text-xs text-success hover:bg-success/30"
        >
          Approve
        </button>
        <button
          onClick={handleReject}
          className="flex-1 bg-warning/20 px-2 py-1 text-xs text-warning hover:bg-warning/30"
        >
          Reject
        </button>
      </footer>
    </section>
  );
}
