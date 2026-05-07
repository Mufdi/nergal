// Default colors and font settings for the canvas terminal renderer.

/// Mutable so a theme switch can repoint `background`, `foreground`, and
/// `cursorAccent` to the active CSS tokens without forcing every consumer
/// to re-import. Cursor + selection are theme-stable (orange brand).
export const TERM_THEME = {
  background: "#0a0a0b",
  foreground: "#ededef",
  cursor: "#f97316",
  cursorAccent: "#0a0a0b",
  selectionBackground: "rgba(249, 115, 22, 0.2)",
};

/// Re-read `--terminal-surface` and `--terminal-foreground` from the document
/// root and update [`TERM_THEME`] in place. Call once at module load and
/// again whenever `data-theme` flips so the canvas tracks light/dark mode.
/// Falls back silently when the tokens are empty (e.g. headless tests).
export function refreshTermTheme(): void {
  if (typeof window === "undefined" || typeof document === "undefined") return;
  const cs = getComputedStyle(document.documentElement);
  const surface = cs.getPropertyValue("--terminal-surface").trim();
  const fg = cs.getPropertyValue("--terminal-foreground").trim();
  if (surface) {
    TERM_THEME.background = surface;
    TERM_THEME.cursorAccent = surface;
  }
  if (fg) TERM_THEME.foreground = fg;
}

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
