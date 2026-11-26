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
  const workingState = (): IndicatorState => (mode === "idle" ? "working" : fromMode(mode));
  switch (root.state) {
    case "error":
      return "error";
    case "done":
      return "done";
    case "idle":
      return "idle";
    case "working":
    case "blocked":
      return workingState();
    default: {
      const _exhaustive: never = root.state;
      void _exhaustive;
      return fromMode(mode);
    }
  }
}

export function indicatorIcon(state: IndicatorState): "check" | "x" | null {
  if (state === "completed") return "check";
  if (state === "error") return "x";
  return null;
}

const DOT_CLASSES: Record<IndicatorState, string> = {
  idle: "bg-muted-foreground/40",
  thinking: "bg-sky-400",
  working: "bg-green-500 text-green-500",
  attention: "bg-primary text-primary",
  completed: "bg-muted-foreground/30",
  done: "ring-1 ring-inset ring-green-500 bg-transparent",
  error: "bg-red-500",
};

export function indicatorDotClass(state: IndicatorState): string {
  return DOT_CLASSES[state];
}

export function statusBarDot(state: IndicatorState): { icon: "check" | "x" | null; className: string } {
  return { icon: indicatorIcon(state), className: indicatorDotClass(state) };
}
