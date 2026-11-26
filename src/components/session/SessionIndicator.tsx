import { useAtomValue } from "jotai";
import { Check } from "lucide-react";
import { modeMapAtom } from "@/stores/workspace";
import { planReviewStatusMapAtom } from "@/stores/plan";
import { pendingAsksAtom, pendingAttentionAtom } from "@/stores/askUser";
import { programStatusMapAtom } from "@/stores/programStatus";
import { resolveIndicatorState, type IndicatorState } from "@/lib/sessionIndicatorState";

interface SessionIndicatorProps {
  sessionId: string;
  sessionStatus: "idle" | "running" | "needs_attention" | "completed";
  size?: "xs" | "sm" | "md";
}

const STATE_COLORS: Record<IndicatorState, string> = {
  idle: "bg-muted-foreground/40",
  thinking: "bg-sky-400",
  working: "bg-green-500 text-green-500",
  attention: "bg-primary text-primary",
  completed: "bg-muted-foreground/30",
  done: "ring-1 ring-inset ring-green-500 bg-transparent",
  error: "bg-red-500",
};

const STATE_ANIMATIONS: Record<IndicatorState, string> = {
  idle: "",
  thinking: "",
  working: "animate-session-working",
  attention: "animate-session-attention",
  completed: "",
  done: "",
  error: "",
};

const SIZE_CLASSES = {
  xs: "size-1.5",
  sm: "size-2",
  md: "size-2.5",
};

export function SessionIndicator({ sessionId, sessionStatus, size = "sm" }: SessionIndicatorProps) {
  const modeMap = useAtomValue(modeMapAtom);
  const planReviewMap = useAtomValue(planReviewStatusMapAtom);
  const pendingAsks = useAtomValue(pendingAsksAtom);
  const pendingAttention = useAtomValue(pendingAttentionAtom);

  const root = useAtomValue(programStatusMapAtom)[sessionId];

  const state = resolveIndicatorState({
    sessionStatus,
    mode: modeMap[sessionId] ?? "idle",
    planReview: planReviewMap[sessionId],
    pendingAsk: !!pendingAsks[sessionId],
    pendingAttention: !!pendingAttention[sessionId],
    root,
  });

  if (state === "completed") {
    return <Check className="size-3 shrink-0 text-muted-foreground/50" />;
  }

  return (
    <span
      className={`shrink-0 session-indicator ${SIZE_CLASSES[size]} ${STATE_COLORS[state]} ${STATE_ANIMATIONS[state]}`}
      aria-hidden="true"
    />
  );
}
