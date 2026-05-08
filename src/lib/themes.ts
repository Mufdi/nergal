export interface ThemePreview {
  background: string;
  card: string;
  secondary: string;
  foreground: string;
  mutedForeground: string;
  border: string;
  primary: string;
}

export interface ThemeFonts {
  /** Body / chrome / forms — matches `--font-sans` once theme is active. */
  interface: string;
  /** Canvas terminal renderer — picked up by TERM_FONT via CSS var. */
  terminal: string;
  /** Markdown reader (plan + spec viewers). */
  markdown: string;
}

export interface Theme {
  id: string;
  label: string;
  /** When true, hidden from the Settings picker. Selectable only via legacy
   *  config values (e.g. user upgrading from "light"). */
  hidden?: boolean;
  preview: ThemePreview;
  fonts: ThemeFonts;
}

const FONT_GEIST_SANS = '"Geist Variable", "Inter", system-ui, sans-serif';
const FONT_INTER = '"Inter Variable", "Inter", system-ui, sans-serif';
const FONT_SPACE_GROTESK =
  '"Space Grotesk Variable", "Space Grotesk", system-ui, sans-serif';
const FONT_SPACE_MONO =
  '"Space Mono", "JetBrains Mono Variable", "Menlo", monospace';
const FONT_JETBRAINS_MONO =
  '"JetBrains Mono Variable", "JetBrains Mono", "Fira Code", monospace';
const FONT_SOURCE_SERIF =
  '"Source Serif 4 Variable", "Source Serif Pro", Georgia, serif';
const FONT_SYSTEM_SERIF = 'Georgia, "Times New Roman", "Iowan Old Style", serif';
const FONT_SYSTEM_SANS = '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';

export const THEMES: Theme[] = [
  {
    id: "v1-dark",
    label: "v1",
    preview: {
      background: "#141415",
      card: "#0a0a0b",
      secondary: "#1c1c1e",
      foreground: "#ededef",
      mutedForeground: "#5c5c5f",
      border: "rgba(255,255,255,0.08)",
      primary: "#f97316",
    },
    fonts: {
      interface: FONT_GEIST_SANS,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_GEIST_SANS,
    },
  },
  {
    id: "v1-light",
    label: "v1 Light",
    hidden: true,
    preview: {
      background: "#ebebed",
      card: "#ffffff",
      secondary: "#e4e4e7",
      foreground: "#18181b",
      mutedForeground: "#52525b",
      border: "rgba(0,0,0,0.14)",
      primary: "#ea580c",
    },
    fonts: {
      interface: FONT_GEIST_SANS,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_GEIST_SANS,
    },
  },
  {
    id: "v2-editorial",
    label: "v2 Editorial",
    preview: {
      background: "#1a1625",
      card: "#15111f",
      secondary: "#2a2438",
      foreground: "#e9e6ee",
      mutedForeground: "#867d99",
      border: "rgba(255,255,255,0.07)",
      primary: "#a78bfa",
    },
    fonts: {
      interface: FONT_SYSTEM_SANS,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_SYSTEM_SERIF,
    },
  },
  {
    id: "v3-crystal",
    label: "v3 Crystal",
    preview: {
      background: "#0c0e16",
      card: "#050813",
      secondary: "#1a2030",
      foreground: "#e2e8f0",
      mutedForeground: "#94a3b8",
      border: "rgba(255,255,255,0.08)",
      primary: "#06b6d4",
    },
    fonts: {
      interface: FONT_INTER,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_SOURCE_SERIF,
    },
  },
  {
    id: "v4-brutalist",
    label: "v4 Brutalist",
    preview: {
      background: "#1a1a1a",
      card: "#0a0a0a",
      secondary: "#2a2a2a",
      foreground: "#ffffff",
      mutedForeground: "#888888",
      border: "rgba(255,255,255,0.15)",
      primary: "#facc15",
    },
    fonts: {
      interface: FONT_SPACE_GROTESK,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_SPACE_GROTESK,
    },
  },
  {
    /** Adapted from VoltAgent/awesome-design-md → raycast/DESIGN.md.
     *  Surface ladder Canvas/Surface/SurfaceCard, Hairline borders, Inter
     *  with `ss03` stylistic set, and Accent Blue as the brand color. */
    id: "v5-raycast",
    label: "v5 Raycast",
    preview: {
      background: "#07080a",
      card: "#121212",
      secondary: "#18191a",
      foreground: "#f4f4f6",
      mutedForeground: "#9c9c9d",
      border: "rgba(255,255,255,0.08)",
      primary: "#57c1ff",
    },
    fonts: {
      interface: FONT_INTER,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_INTER,
    },
  },
  {
    /** Adapted from R0122 dominikmartn/nothing-design-skill (Nothing OS).
     *  OLED black canvas, Nothing red interrupt accent, Space Grotesk
     *  workhorse + Space Mono for terminal/data. Doto dot-matrix font is
     *  loaded for future hero-moment use (see globals.css). */
    id: "v6-nothing",
    label: "v6 Nothing",
    preview: {
      background: "#000000",
      card: "#111111",
      secondary: "#1a1a1a",
      foreground: "#e8e8e8",
      mutedForeground: "#999999",
      border: "rgba(255,255,255,0.13)",
      primary: "#d71921",
    },
    fonts: {
      interface: FONT_SPACE_GROTESK,
      terminal: FONT_SPACE_MONO,
      markdown: FONT_SPACE_GROTESK,
    },
  },
];

export const DEFAULT_THEME_ID = "v1-dark";

const LEGACY_ALIASES: Record<string, string> = {
  dark: "v1-dark",
  light: "v1-light",
};

export function normalizeThemeId(value: string | null | undefined): string {
  if (!value) return DEFAULT_THEME_ID;
  if (THEMES.some((t) => t.id === value)) return value;
  return LEGACY_ALIASES[value] ?? DEFAULT_THEME_ID;
}

/** Themes shown in the Settings grid. Hidden themes (e.g. v1-light) remain
 *  selectable via legacy config but are not exposed for new selection. */
export const VISIBLE_THEMES: Theme[] = THEMES.filter((t) => !t.hidden);

export function getTheme(id: string): Theme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

export function applyTheme(themeId: string): void {
  const id = normalizeThemeId(themeId);
  const theme = getTheme(id);
  const root = document.documentElement;
  root.dataset.theme = id;
  // Set font CSS vars so `--font-sans` (chrome) and the terminal canvas
  // pick them up. Defaults in globals.css fall back to Geist when unset.
  root.style.setProperty("--theme-font-interface", theme.fonts.interface);
  root.style.setProperty("--theme-font-terminal", theme.fonts.terminal);
  root.style.setProperty("--theme-font-markdown", theme.fonts.markdown);
}
