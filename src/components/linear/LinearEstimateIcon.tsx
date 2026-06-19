import { useId } from "react";

/// Linear's t-shirt estimate glyph: an upward triangle with rounded corners,
/// half-filled (the same shape Linear uses for any size — like the "started"
/// state glyph, but a triangle instead of a circle). Inherits the text color via
/// `currentColor`, mirroring LinearStatusIcon.
export function LinearEstimateIcon({ size = 14, className }: { size?: number; className?: string }) {
  // Strip the colons React's useId emits — they break `url(#…)` refs in WebKit.
  const clip = `est-${useId().replace(/:/g, "")}`;
  // Rounded-corner triangle. The arcs round each vertex; the path is filled
  // (bottom half via clip) AND stroked so the half-fill reads cleanly.
  const tri =
    "M6.04 2.32 a1.2 1.2 0 0 1 1.92 0 L12.5 10.9 a1.2 1.2 0 0 1 -0.96 1.86 H2.46 a1.2 1.2 0 0 1 -0.96 -1.86 Z";
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 14 14"
      className={className}
      role="img"
      aria-hidden
    >
      <defs>
        <clipPath id={clip}>
          <path d={tri} />
        </clipPath>
      </defs>
      {/* Right-half (vertical) fill, clipped to the triangle. */}
      <rect x="7" y="0" width="7" height="14" fill="currentColor" clipPath={`url(#${clip})`} />
      {/* Rounded outline on top. */}
      <path d={tri} fill="none" stroke="currentColor" strokeWidth="1.3" />
    </svg>
  );
}
