/// Linear-style priority glyph: three ascending signal bars, filled up to the
/// priority level. ClickUp priorities map low→1, normal→2, high→3, urgent→3
/// (urgent reads through its red hue). A null priority renders three faint
/// bars so list rows stay column-aligned (Linear's "no priority" placeholder).

const PRIORITY_LEVEL: Record<string, number> = { low: 1, normal: 2, high: 3, urgent: 3 };
const PRIORITY_COLOR: Record<string, string> = {
  urgent: "#f50000",
  high: "#ffcc00",
  normal: "#6fddff",
  low: "#d8d8d8",
};

const BARS = [
  { x: 1.5, y: 9, h: 4 },
  { x: 5.5, y: 6, h: 7 },
  { x: 9.5, y: 3, h: 10 },
];

export function PriorityIcon({
  priority,
  size = 14,
  className,
}: {
  priority: string | null;
  size?: number;
  className?: string;
}) {
  const level = priority ? PRIORITY_LEVEL[priority] ?? 0 : 0;
  const color = priority ? PRIORITY_COLOR[priority] ?? "#d8d8d8" : "#d8d8d8";

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 14 14"
      className={className}
      role="img"
      aria-label={priority ? `Priority: ${priority}` : "No priority"}
    >
      {BARS.map((b, i) => (
        <rect
          key={b.x}
          x={b.x}
          y={b.y}
          width={3}
          height={b.h}
          rx={0.6}
          fill={i < level ? color : "var(--color-muted-foreground)"}
          opacity={i < level ? 1 : 0.25}
        />
      ))}
    </svg>
  );
}
