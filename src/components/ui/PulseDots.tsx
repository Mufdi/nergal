/// In-progress indicator that pulses a row of dots instead of spinning — the
/// inline counterpart to ProgressBar (which is for region/panel loaders). Dots
/// inherit the current text color (`bg-current`) so they match each action
/// label's tone; `count={1}` gives the icon-slot variant (no surrounding text).
export function PulseDots({
  count = 3,
  className = "",
  dotClassName = "size-1",
}: {
  count?: number;
  className?: string;
  dotClassName?: string;
}) {
  return (
    <span aria-hidden className={`inline-flex items-center gap-0.5 ${className}`}>
      {Array.from({ length: count }, (_, i) => (
        <span
          key={i}
          className={`cluihud-dot shrink-0 rounded-full bg-current ${dotClassName}`}
          style={{ animationDelay: `${i * 150}ms` }}
        />
      ))}
    </span>
  );
}
