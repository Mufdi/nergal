import { cn } from "@/lib/utils";

export interface KeyHint {
  keys: string;
  label: string;
}

/// Footer key-hint strip in the Quick Capture style: a `<kbd>` chip per key
/// followed by its action label, `·`-separated. Use in modals/pickers so the
/// keyboard affordances read consistently (patterns.md §5.3).
export function KeyHints({
  hints,
  className,
}: {
  hints: readonly KeyHint[];
  className?: string;
}) {
  return (
    <p className={cn("text-[10px] text-muted-foreground/60", className)}>
      {hints.map((h, i) => (
        <span key={`${h.keys}-${h.label}`}>
          {i > 0 && " · "}
          <kbd className="rounded bg-secondary px-1 py-0.5">{h.keys}</kbd> {h.label}
        </span>
      ))}
    </p>
  );
}
