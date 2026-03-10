import { useAtomValue } from "jotai";
import { activeSessionAtom, sessionModeAtom, costSummaryAtom } from "@/stores/session";

interface StatusBarProps {
  hasPlan: boolean;
  planVisible: boolean;
  onTogglePlan: () => void;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function StatusBar({ hasPlan, planVisible, onTogglePlan }: StatusBarProps) {
  const session = useAtomValue(activeSessionAtom);
  const mode = useAtomValue(sessionModeAtom);
  const cost = useAtomValue(costSummaryAtom);

  return (
    <footer
      className="flex h-6 items-center justify-between border-t border-border bg-surface px-3 text-xs"
      role="status"
    >
      <div className="flex items-center gap-3">
        <span
          className={`flex items-center gap-1 ${
            mode === "idle" ? "text-text-muted" : "text-accent"
          }`}
        >
          <span
            className={`inline-block h-1.5 w-1.5 ${
              mode === "idle" ? "bg-text-muted" : "bg-success"
            }`}
            aria-hidden="true"
          />
          {mode}
        </span>

        {session && (
          <span className="text-text-muted" title={session.id}>
            {session.id.slice(0, 8)}
          </span>
        )}
      </div>

      <div className="flex items-center gap-3 text-text-muted">
        {hasPlan && (
          <button
            onClick={onTogglePlan}
            className={`hover:text-text ${planVisible ? "text-accent" : ""}`}
            title={planVisible ? "Hide plan" : "Show plan"}
          >
            Plan
          </button>
        )}
        <span title="Input tokens">in:{formatTokens(cost.input_tokens)}</span>
        <span title="Output tokens">out:{formatTokens(cost.output_tokens)}</span>
        {cost.cache_read > 0 && <span title="Cache read">cr:{formatTokens(cost.cache_read)}</span>}
        <span className="text-accent" title="Total cost">${cost.total_usd.toFixed(4)}</span>
      </div>
    </footer>
  );
}
