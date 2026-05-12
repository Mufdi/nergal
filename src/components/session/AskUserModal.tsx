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

const OPTION_SEL = "[data-ask-option]";
const MODAL_SEL = "[data-ask-modal]";
const FEEDBACK_SEL = "[data-ask-feedback]";

// Hidden: CC's TUI owns the question UX. The legacy body below is dead at
// runtime but kept around in case we revive the dialog flow.
export function AskUserModal() {
  return null;
}

export function AskUserModalLegacy() {
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

  useEffect(() => {
    if (!askState) return;
    let raf2: number | null = null;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        const modal = document.querySelector<HTMLElement>(MODAL_SEL);
        const firstOption = modal?.querySelector<HTMLButtonElement>(OPTION_SEL);
        firstOption?.focus({ preventScroll: true });
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      if (raf2 !== null) cancelAnimationFrame(raf2);
    };
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
    invoke("submit_ask_answer", {
      decisionPath: askState.decisionPath,
      answers: JSON.stringify(finalAnswers),
      feedback: feedback.trim() || null,
    }).then(() => {
      setAskState(null);
    }).catch(console.error);
  }, [askState, allAnswered, answers, feedback, otherOpen, otherText, setAskState]);

  useEffect(() => {
    if (!askState) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      if (!target) return;
      const modal = document.querySelector<HTMLElement>(MODAL_SEL);
      if (!modal || !modal.contains(target)) return;
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;

      const isArrow =
        e.key === "ArrowRight" || e.key === "ArrowLeft" ||
        e.key === "ArrowUp" || e.key === "ArrowDown";
      const isActivate = e.key === " " || e.key === "Enter";
      if (!isArrow && !isActivate) return;

      const optionBtn = target.closest<HTMLButtonElement>(OPTION_SEL);
      const onFeedback = !!target.closest(FEEDBACK_SEL);

      if (isActivate && optionBtn) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        optionBtn.click();
        return;
      }

      if (!isArrow) return;

      if (onFeedback) {
        const ta = target as HTMLTextAreaElement;
        if (e.key === "ArrowUp" && ta.selectionStart === 0 && ta.selectionEnd === 0) {
          const options = document.querySelectorAll<HTMLButtonElement>(OPTION_SEL);
          const last = options[options.length - 1];
          if (last) {
            e.preventDefault();
            e.stopPropagation();
            e.stopImmediatePropagation();
            last.focus();
          }
        }
        return;
      }

      if (target.tagName === "TEXTAREA") return;

      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();

      const options = Array.from(
        document.querySelectorAll<HTMLButtonElement>(OPTION_SEL),
      );
      if (options.length === 0) return;

      if (!optionBtn) {
        options[0].focus();
        return;
      }

      const idx = options.indexOf(optionBtn);
      const isLast = idx === options.length - 1;
      const isFirst = idx === 0;

      if (e.key === "ArrowDown" && isLast) {
        document.querySelector<HTMLElement>(FEEDBACK_SEL)?.focus();
        return;
      }
      if (e.key === "ArrowUp" && isFirst) {
        document.querySelector<HTMLElement>(FEEDBACK_SEL)?.focus();
        return;
      }
      const delta = (e.key === "ArrowRight" || e.key === "ArrowDown") ? 1 : -1;
      const next = options[(idx + delta + options.length) % options.length];
      next?.focus();
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [askState]);

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
    // Dismissal locked: the CLI hook blocks on a FIFO until the GUI writes an
    // answer, so any close path other than `handleSubmit` would hang the agent.
    <Dialog
      open={!!askState}
      disablePointerDismissal
      onOpenChange={(open, details) => {
        if (open) return;
        if (details?.reason === "imperative-action") return;
        details?.cancel();
      }}
    >
      <DialogContent
        data-ask-modal
        initialFocus={() =>
          document.querySelector<HTMLElement>(OPTION_SEL) ?? true
        }
      >
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
                            data-ask-option
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
                        data-ask-option
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
              data-ask-feedback
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
