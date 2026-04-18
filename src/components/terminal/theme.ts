// Default colors and font settings for the canvas terminal renderer.

export const TERM_THEME = {
  background: "#0a0a0b",
  foreground: "#ededef",
  cursor: "#f97316",
  cursorAccent: "#0a0a0b",
  selectionBackground: "rgba(249, 115, 22, 0.2)",
} as const;

export const TERM_FONT = {
  family: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  size: 13,
  /// Line-height multiplier applied to the base size. Gives a tiny bit of
  /// breathing room so adjacent lines don't visually touch.
  lineHeight: 1.25,
} as const;

export function rgbaToCss(rgba: [number, number, number, number] | null, fallback: string): string {
  if (!rgba) return fallback;
  const [r, g, b, a] = rgba;
  if (a === 255) return `rgb(${r}, ${g}, ${b})`;
  return `rgba(${r}, ${g}, ${b}, ${(a / 255).toFixed(3)})`;
}
