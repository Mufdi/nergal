/// Linear-style circular status glyph, colored with ClickUp's own status
/// color. ClickUp statuses carry a workflow `type` identical to Linear's
/// categories, so the shape is driven by `type` and the hue by `color`:
///
///   open    → hollow ring          (unstarted / "to do")
///   custom  → proportional pie      (started / "in progress")
///   done    → filled disc + check   (terminal, not archived)
///   closed  → filled disc + check   (terminal, archived)
///
/// `done`/`closed` share the filled-check shape; the distinction reads through
/// ClickUp's own colors (done is usually green, closed gray). `fraction` sets
/// the pie fill for the started state — the detail modal computes the exact
/// value from the list's ordered workflow; the panel passes nothing and gets a
/// fixed half-pie (Linear shows a category glyph in dense lists, not a precise
/// gauge). A null `type` falls back to a plain dot (pre-migration rows).

const MUTED = "var(--color-muted-foreground)";

// The "in progress" wedge is a thick-stroked inner circle (r=2, stroke 4 → it
// fills the disc from the centre out to r=4). The outer ring sits at r=6, so a
// ~1.25px gap reads between the wedge and the ring — Linear's signature look,
// not a half-filled disc that touches the ring.
const PIE_R = 2;
const PIE_CIRC = 2 * Math.PI * PIE_R;

function pieDash(fraction: number): string {
  const filled = Math.max(0, Math.min(1, fraction)) * PIE_CIRC;
  return `${filled} ${PIE_CIRC - filled}`;
}

export function StatusIcon({
  type,
  color,
  fraction = 0.5,
  size = 14,
  title,
  className,
}: {
  type: string | null;
  color: string | null;
  fraction?: number;
  size?: number;
  title?: string;
  className?: string;
}) {
  const hue = color ?? MUTED;

  // Pre-migration rows have no type — keep the legacy dot so they still read.
  if (type === null) {
    return (
      <span
        className={className}
        style={{
          display: "inline-block",
          width: size * 0.43,
          height: size * 0.43,
          borderRadius: "9999px",
          background: hue,
        }}
        title={title}
        aria-label={title}
      />
    );
  }

  const filled = type === "done" || type === "closed";

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 14 14"
      fill="none"
      className={className}
      role="img"
      aria-label={title}
    >
      {title && <title>{title}</title>}
      {filled ? (
        <>
          <circle cx="7" cy="7" r="6" fill={hue} />
          <path
            d="M4.4 7.1 L6.2 8.9 L9.7 5.1"
            fill="none"
            stroke="#fff"
            strokeWidth="1.6"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      ) : (
        <>
          <circle cx="7" cy="7" r="6" stroke={hue} strokeWidth="1.5" />
          {type === "custom" && (
            <circle
              cx="7"
              cy="7"
              r={PIE_R}
              stroke={hue}
              strokeWidth={PIE_R * 2}
              strokeDasharray={pieDash(fraction)}
              transform="rotate(-90 7 7)"
            />
          )}
        </>
      )}
    </svg>
  );
}
