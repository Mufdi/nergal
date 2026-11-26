import type { ProgramStatusEntry } from "@/stores/programStatus";

export type IndicatorState = "idle" | "thinking" | "working" | "attention" | "completed" | "done" | "error";

interface AwaitingInput {
  pendingAsk: boolean;
  pendingAttention: boolean;
  root: ProgramStatusEntry | undefined;
}

interface IndicatorInput extends AwaitingInput {
  sessionStatus: string;
  mode: string;
  planReview: string | undefined;
}

export function isAwaitingSession({ pendingAsk, pendingAttention, root }: AwaitingInput): boolean {
  return pendingAsk || pendingAttention || root?.state === "blocked";
}

function fromMode(mode: string): IndicatorState {
  if (mode === "idle") return "idle";
  if (mode === "active") return "thinking";
  return "working";
}

export function resolveIndicatorState(input: IndicatorInput): IndicatorState {
  const { sessionStatus, mode, planReview, root } = input;
  if (planReview === "pending_review" || isAwaitingSession(input)) return "attention";
  if (sessionStatus === "completed") return "completed";
  if (!root) return fromMode(mode);
  switch (root.state) {
    case "error":
      return "error";
    case "done":
      return "done";
    case "idle":
      return "idle";
    default:
      return mode === "idle" ? "working" : fromMode(mode);
  }
}
