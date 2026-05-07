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
const FONT_JETBRAINS_MONO =
  "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace";
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
