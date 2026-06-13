/// Slim indeterminate progress bar. Reusable across surfaces that wait on a
/// network round-trip (ClickUp status resolve, closure apply, …). Indeterminate
/// only — a moving sliver, no percentage. Honors the design tokens.
export function ProgressBar({ className = "" }: { className?: string }) {
  return (
    <div
      role="progressbar"
      aria-busy="true"
      className={`h-0.5 w-full overflow-hidden rounded-full bg-secondary/40 ${className}`}
    >
      <div className="cluihud-progress-indeterminate h-full w-1/3 rounded-full bg-primary/70" />
    </div>
  );
}
