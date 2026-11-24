import type { ReactNode } from "react";
import { Kbd } from "./kbd";

export function HintBar({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={
        "z-10 flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-t border-border/40 bg-card/95 px-3 py-1.5 text-[10px] text-muted-foreground backdrop-blur " +
        (className ?? "")
      }
    >
      {children}
    </div>
  );
}

export function Hint({ keys, children }: { keys: string | string[]; children: ReactNode }) {
  const list = Array.isArray(keys) ? keys : [keys];
  return (
    <span className="inline-flex items-center gap-1">
      {list.map((k) => (
        <Kbd key={k} keys={k} size="md" />
      ))}
      <span>{children}</span>
    </span>
  );
}
