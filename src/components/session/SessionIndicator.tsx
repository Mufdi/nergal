import { useAtomValue } from "jotai";
import { Check } from "lucide-react";
import { modeMapAtom } from "@/stores/workspace";
import { planReviewStatusMapAtom } from "@/stores/plan";
import { askUserAtom } from "@/stores/askUser";

type IndicatorState = "idle" | "thinking" | "working" | "attention" | "completed";

interface SessionIndicatorProps {
  sessionId: string;
  sessionStatus: "idle" | "running" | "needs_attention" | "completed";
  size?: "xs" | "sm" | "md";
}

const STATE_COLORS: Record<IndicatorState, string> = {
  idle: "bg-muted-foreground/40",
  thinking: "bg-sky-400",
  working: "bg-green-500 text-green-500",
  attention: "bg-orange-500",
  completed: "bg-muted-foreground/30",
};

const STATE_ANIMATIONS: Record<IndicatorState, string> = {
  idle: "",
  thinking: "",
  working: "animate-session-working",
  attention: "animate-session-attention",
  completed: "",
};

const SIZE_CLASSES = {
  xs: "size-1.5",
  sm: "size-2",
  md: "size-2.5",
};

export function SessionIndicator({ sessionId, sessionStatus, size = "sm" }: SessionIndicatorProps) {
  const modeMap = useAtomValue(modeMapAtom);
  const planReviewMap = useAtomValue(planReviewStatusMapAtom);
  const askUser = useAtomValue(askUserAtom);

  const state = resolveState(sessionId, sessionStatus, modeMap, planReviewMap, askUser);

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

function resolveState(
  sessionId: string,
  sessionStatus: string,
  modeMap: Record<string, string>,
  planReviewMap: Record<string, string>,
  askUser: { sessionId: string } | null,
): IndicatorState {
  if (planReviewMap[sessionId] === "pending_review") return "attention";
  if (askUser?.sessionId === sessionId) return "attention";
  if (sessionStatus === "completed") return "completed";

  const mode = modeMap[sessionId] ?? "idle";
  if (mode === "idle") return "idle";
  if (mode === "active") return "thinking";
  return "working";
}
