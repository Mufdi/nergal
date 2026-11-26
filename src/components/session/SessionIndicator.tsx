import { useAtomValue } from "jotai";
import { Check, X } from "lucide-react";
import { modeMapAtom } from "@/stores/workspace";
import { planReviewStatusMapAtom } from "@/stores/plan";
import { pendingAsksAtom, pendingAttentionAtom } from "@/stores/askUser";
import { programStatusMapAtom } from "@/stores/programStatus";
import {
  indicatorDotClass,
  indicatorIcon,
  resolveIndicatorState,
  type IndicatorState,
} from "@/lib/sessionIndicatorState";

interface SessionIndicatorProps {
  sessionId: string;
  sessionStatus: "idle" | "running" | "needs_attention" | "completed";
  size?: "xs" | "sm" | "md";
}

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

  const icon = indicatorIcon(state);
  if (icon === "check") {
    return <Check className="size-3 shrink-0 text-muted-foreground/50" />;
  }
  if (icon === "x") {
    return <X className="size-3 shrink-0 text-red-500" />;
  }

  return (
    <span
      className={`shrink-0 session-indicator ${SIZE_CLASSES[size]} ${indicatorDotClass(state)} ${STATE_ANIMATIONS[state]}`}
      aria-hidden="true"
    />
  );
}
