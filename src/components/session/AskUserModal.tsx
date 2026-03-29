import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom } from "jotai";
import { askUserAtom } from "@/stores/askUser";
import { invoke } from "@/lib/tauri";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

export function AskUserModal() {
  const [askState, setAskState] = useAtom(askUserAtom);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [otherOpen, setOtherOpen] = useState<Record<string, boolean>>({});
  const [otherText, setOtherText] = useState<Record<string, string>>({});
  const [feedback, setFeedback] = useState("");
  const otherRefs = useRef<Record<string, HTMLInputElement | null>>({});

  useEffect(() => {
    if (askState) {
      setAnswers({});
      setOtherOpen({});
      setOtherText({});
      setFeedback("");
    }
  }, [askState]);

  const allAnswered = askState?.questions.every((q) => {
    const hasOptions = (q.options?.length ?? 0) > 0;
    if (hasOptions) {
      if (otherOpen[q.question]) return !!otherText[q.question]?.trim();
      return !!answers[q.question];
    }
    return !!answers[q.question]?.trim();
  }) ?? false;

  const handleSubmit = useCallback(() => {
    if (!askState || !allAnswered) return;
    const finalAnswers: Record<string, string> = {};
    for (const q of askState.questions) {
      if (otherOpen[q.question]) {
        finalAnswers[q.question] = otherText[q.question]?.trim() ?? "";
      } else {
        finalAnswers[q.question] = answers[q.question] ?? "";
      }
    }
    if (feedback.trim()) {
      finalAnswers["_feedback"] = feedback.trim();
    }
    invoke("submit_ask_answer", {
      decisionPath: askState.decisionPath,
      answers: JSON.stringify(finalAnswers),
    }).then(() => {
      setAskState(null);
    }).catch(console.error);
  }, [askState, allAnswered, answers, feedback, otherOpen, otherText, setAskState]);

  useEffect(() => {
    if (!askState) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Enter" && e.ctrlKey) {
        e.preventDefault();
        handleSubmit();
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [askState, handleSubmit]);

  if (!askState) return null;

  function selectOption(question: string, value: string) {
    setAnswers((prev) => ({ ...prev, [question]: value }));
    setOtherOpen((prev) => ({ ...prev, [question]: false }));
  }

  function toggleOther(question: string) {
    const opening = !otherOpen[question];
    setOtherOpen((prev) => ({ ...prev, [question]: opening }));
    if (opening) {
      setAnswers((prev) => ({ ...prev, [question]: "" }));
      requestAnimationFrame(() => otherRefs.current[question]?.focus());
    }
  }

  function setFreeText(question: string, value: string) {
    setAnswers((prev) => ({ ...prev, [question]: value }));
  }

  const unanswered = askState.questions.filter((q) => {
    const hasOptions = (q.options?.length ?? 0) > 0;
    if (hasOptions) {
      if (otherOpen[q.question]) return !otherText[q.question]?.trim();
      return !answers[q.question];
    }
    return !answers[q.question]?.trim();
  }).length;

  return (
    <Dialog open={!!askState} onOpenChange={(open) => { if (!open) setAskState(null); }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="text-sm">Claude needs your input</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 max-h-[60vh] overflow-y-auto p-1">
          {askState.questions.map((q) => {
            const hasOptions = (q.options?.length ?? 0) > 0;
            const isOther = otherOpen[q.question];

            return (
              <div key={q.question} className="flex flex-col gap-2">
                {q.header && (
                  <span className="text-[11px] font-medium uppercase text-muted-foreground">{q.header}</span>
                )}
                <p className="text-xs text-foreground/90 leading-relaxed">{q.question}</p>

                {hasOptions ? (
                  <div className="flex flex-col gap-1.5">
                    <div className="flex flex-wrap gap-1.5">
                      {q.options.map((opt) => {
                        const selected = !isOther && (q.multi_select
                          ? (answers[q.question] ?? "").split(",").includes(opt)
                          : answers[q.question] === opt);
                        return (
                          <button
                            key={opt}
                            type="button"
                            onClick={() => {
                              if (q.multi_select) {
                                setOtherOpen((prev) => ({ ...prev, [q.question]: false }));
                                const current = (answers[q.question] ?? "").split(",").filter(Boolean);
                                const next = selected
                                  ? current.filter((o) => o !== opt)
                                  : [...current, opt];
                                setAnswers((prev) => ({ ...prev, [q.question]: next.join(",") }));
                              } else {
                                selectOption(q.question, opt);
                              }
                            }}
                            className={`rounded-md border px-3 py-1.5 text-[11px] transition-colors ${
                              selected
                                ? "border-orange-500 bg-orange-500/10 text-foreground"
                                : "border-border bg-card text-muted-foreground hover:text-foreground"
                            }`}
                          >
                            {opt}
                          </button>
                        );
                      })}
                      <button
                        type="button"
                        onClick={() => toggleOther(q.question)}
                        className={`rounded-md border px-3 py-1.5 text-[11px] italic transition-colors ${
                          isOther
                            ? "border-orange-500 bg-orange-500/10 text-foreground"
                            : "border-border bg-card text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        Other...
                      </button>
                    </div>
                    {isOther && (
                      <input
                        ref={(el) => { otherRefs.current[q.question] = el; }}
                        type="text"
                        value={otherText[q.question] ?? ""}
                        onChange={(e) => setOtherText((prev) => ({ ...prev, [q.question]: e.target.value }))}
                        placeholder="Type your answer..."
                        className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-xs focus:ring-1 focus:ring-ring"
                      />
                    )}
                  </div>
                ) : (
                  <textarea
                    autoFocus
                    value={answers[q.question] ?? ""}
                    onChange={(e) => setFreeText(q.question, e.target.value)}
                    placeholder="Your answer..."
                    className="h-20 w-full resize-none rounded-md border border-border bg-background p-3 text-xs focus:ring-1 focus:ring-ring"
                  />
                )}
              </div>
            );
          })}

          <div className="flex flex-col gap-1.5 border-t border-border/50 pt-3">
            <span className="text-[11px] text-muted-foreground">Additional feedback (optional)</span>
            <textarea
              value={feedback}
              onChange={(e) => setFeedback(e.target.value)}
              placeholder="Any extra context or comments..."
              className="h-16 w-full resize-none rounded-md border border-border bg-background p-3 text-xs focus:ring-1 focus:ring-ring"
            />
          </div>
        </div>

        <DialogFooter className="flex items-center justify-between">
          {!allAnswered && Object.keys(answers).length > 0 && (
            <span className="text-[10px] text-orange-400">{unanswered} unanswered</span>
          )}
          <Button
            variant="default"
            size="sm"
            onClick={handleSubmit}
            disabled={!allAnswered}
          >
            Send (Ctrl+Enter)
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
