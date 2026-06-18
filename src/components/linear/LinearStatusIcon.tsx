/// Linear-faithful circular status glyph, driven by Linear's own `StateType`
/// (NOT the shared ClickUp StatusIcon, which only knows open/custom/done/closed).
/// Shapes match Linear's UI:
///   triage     → hollow ring + centered dot (intake)
///   backlog    → dashed hollow ring
///   unstarted  → hollow ring                (Todo)
///   started    → ring + proportional pie     (In Progress)
///   completed  → filled disc + check         (Done / Pending)
///   canceled   → filled disc + ✕
///   duplicate  → filled disc + ✕             (distinct Linear StateType)
/// Hue comes from the workflow state color; a null type/color falls back to a
/// muted dot / gray so pre-migration rows still read.

const MUTED = "var(--color-muted-foreground)";

const PIE_R = 2;
const PIE_CIRC = 2 * Math.PI * PIE_R;

function pieDash(fraction: number): string {
  const filled = Math.max(0, Math.min(1, fraction)) * PIE_CIRC;
  return `${filled} ${PIE_CIRC - filled}`;
}

export function LinearStatusIcon({
  stateType,
  color,
  fraction = 0.5,
  size = 14,
  title,
  className,
}: {
  stateType: string | null | undefined;
  color: string | null;
  fraction?: number;
  size?: number;
  title?: string;
  className?: string;
}) {
  const hue = color ?? MUTED;

  if (stateType == null) {
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

  const filled = stateType === "completed";
  const crossed = stateType === "canceled" || stateType === "cancelled" || stateType === "duplicate";

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
      ) : crossed ? (
        <>
          <circle cx="7" cy="7" r="6" fill={hue} />
          <path
            d="M4.8 4.8 L9.2 9.2 M9.2 4.8 L4.8 9.2"
            stroke="#fff"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        </>
      ) : (
        <>
          <circle
            cx="7"
            cy="7"
            r="6"
            stroke={hue}
            strokeWidth="1.5"
            // Backlog reads as a dashed ring in Linear; the others are solid.
            strokeDasharray={stateType === "backlog" ? "2 2" : undefined}
          />
          {stateType === "started" && (
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
          {stateType === "triage" && <circle cx="7" cy="7" r="1.6" fill={hue} />}
        </>
      )}
    </svg>
  );
}
