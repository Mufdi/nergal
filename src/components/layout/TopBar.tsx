import { useAtomValue } from "jotai";
import { sessionModeAtom } from "@/stores/session";

export function TopBar() {
  const mode = useAtomValue(sessionModeAtom);

  return (
    <div className="flex h-10 shrink-0 items-center justify-between border-b border-border/30 bg-background px-3">
      <div className="flex items-center gap-3">
        <span className="text-sm font-semibold text-foreground">cluihud</span>
        <span className="text-xs text-muted-foreground">Claude Code Desktop</span>
      </div>

      <div className="flex items-center gap-2">
        {mode !== "idle" && (
          <div className="flex items-center gap-1.5">
            <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-orange-500" />
            <span className="text-xs text-muted-foreground">{mode}</span>
          </div>
        )}
      </div>
    </div>
  );
}
