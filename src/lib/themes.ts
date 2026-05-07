export interface ThemePreview {
  background: string;
  card: string;
  secondary: string;
  foreground: string;
  mutedForeground: string;
  border: string;
  primary: string;
}

export interface Theme {
  id: string;
  label: string;
  preview: ThemePreview;
}

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
  },
  {
    id: "v1-light",
    label: "v1 Light",
    preview: {
      background: "#fafafa",
      card: "#ffffff",
      secondary: "#f4f4f5",
      foreground: "#0a0a0b",
      mutedForeground: "#71717a",
      border: "rgba(0,0,0,0.08)",
      primary: "#f97316",
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

export function applyTheme(themeId: string): void {
  const id = normalizeThemeId(themeId);
  document.documentElement.dataset.theme = id;
}
