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

import type { CustomTheme } from "./types";

export const FONT_GEIST_SANS = '"Geist Variable", "Inter", system-ui, sans-serif';
export const FONT_INTER = '"Inter Variable", "Inter", system-ui, sans-serif';
export const FONT_SPACE_GROTESK =
  '"Space Grotesk Variable", "Space Grotesk", system-ui, sans-serif';
export const FONT_SPACE_MONO =
  '"Space Mono", "JetBrains Mono Variable", "Menlo", monospace';
export const FONT_JETBRAINS_MONO =
  '"JetBrains Mono Variable", "JetBrains Mono", "Fira Code", monospace';
export const FONT_SOURCE_SERIF =
  '"Source Serif 4 Variable", "Source Serif Pro", Georgia, serif';
export const FONT_SYSTEM_SERIF = 'Georgia, "Times New Roman", "Iowan Old Style", serif';
export const FONT_SYSTEM_SANS =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';
export const FONT_BLACKLETTER =
  '"UnifrakturCook", "Cloister Black", "Old English Text MT", "UnifrakturMaguntia", serif';

/// Curated font stacks exposed in the custom-theme editor. Keeping the list
/// closed (vs. an open text input) avoids users picking fonts the bundle
/// doesn't ship — every entry here is either bundled via `@fontsource*`
/// imports in `globals.css` or relies on system fallbacks.
export interface FontOption {
  id: string;
  label: string;
  stack: string;
}

export const INTERFACE_FONTS: FontOption[] = [
  { id: "geist", label: "Geist", stack: FONT_GEIST_SANS },
  { id: "inter", label: "Inter", stack: FONT_INTER },
  { id: "space-grotesk", label: "Space Grotesk", stack: FONT_SPACE_GROTESK },
  { id: "system-sans", label: "System Sans", stack: FONT_SYSTEM_SANS },
  { id: "blackletter", label: "Blackletter", stack: FONT_BLACKLETTER },
];

export const TERMINAL_FONTS: FontOption[] = [
  { id: "jetbrains-mono", label: "JetBrains Mono", stack: FONT_JETBRAINS_MONO },
  { id: "space-mono", label: "Space Mono", stack: FONT_SPACE_MONO },
];

export const MARKDOWN_FONTS: FontOption[] = [
  { id: "geist", label: "Geist", stack: FONT_GEIST_SANS },
  { id: "inter", label: "Inter", stack: FONT_INTER },
  { id: "space-grotesk", label: "Space Grotesk", stack: FONT_SPACE_GROTESK },
  { id: "source-serif", label: "Source Serif", stack: FONT_SOURCE_SERIF },
  { id: "system-serif", label: "System Serif", stack: FONT_SYSTEM_SERIF },
  { id: "blackletter", label: "Blackletter", stack: FONT_BLACKLETTER },
];

/// Accent presets exposed as quick swatches in the custom-theme editor.
/// Pulled from existing builtin theme palettes so customs blend visually.
export const ACCENT_PRESETS: { label: string; value: string }[] = [
  { label: "Orange", value: "#f97316" },
  { label: "Cyan", value: "#22d3ee" },
  { label: "Violet", value: "#a78bfa" },
  { label: "Yellow", value: "#facc15" },
  { label: "Blue", value: "#06b6d4" },
  { label: "Red", value: "#d71921" },
];

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
    /** True OLED black + white accent, blackletter chrome. Cloister Black
     *  is proprietary, so UnifrakturCook (OFL) is the runtime fallback —
     *  closest rounded Schwabacher to Cloister Black. Set the user's local
     *  "Cloister Black" / "Old English Text MT" first in the stack so it
     *  wins if installed system-wide. */
    id: "v8-gothic",
    label: "v8 Gothic",
    preview: {
      background: "#000000",
      card: "#050505",
      secondary: "#0d0d0d",
      foreground: "#f5f5f5",
      mutedForeground: "#777777",
      border: "rgba(255,255,255,0.14)",
      primary: "#ffffff",
    },
    fonts: {
      interface: FONT_BLACKLETTER,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_BLACKLETTER,
    },
  },
  {
    /** Neutral grayscale (no tint) + Omarchy/Hyprland-style cyan accent.
     *  Pure mono mood: monospace interface, borders & focus all share the
     *  single cyan token. Dark-only — light variants are blocked by the
     *  WebKitGTK shadow-copy bug (see Known Platform Limitations). */
    id: "v7-mono",
    label: "v7 Mono",
    preview: {
      background: "#0d0d0d",
      card: "#050505",
      secondary: "#1a1a1a",
      foreground: "#e5e5e5",
      mutedForeground: "#808080",
      border: "rgba(255,255,255,0.10)",
      primary: "#22d3ee",
    },
    fonts: {
      interface: FONT_JETBRAINS_MONO,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_JETBRAINS_MONO,
    },
  },
  {
    /** Dracula — classic palette by Zeno Rocha. Purple-violet canvas with
     *  pink/magenta accent. Soft on the eyes, popular across editors. */
    id: "v9-dracula",
    label: "v9 Dracula",
    preview: {
      background: "#282a36",
      card: "#21222c",
      secondary: "#44475a",
      foreground: "#f8f8f2",
      mutedForeground: "#9aa0b6",
      border: "rgba(255,255,255,0.10)",
      primary: "#ff79c6",
    },
    fonts: {
      interface: FONT_INTER,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_INTER,
    },
  },
  {
    /** Monokai — by Wimer Hazenberg. Warm dark canvas with neon-green
     *  signature. Iconic since TextMate; ported everywhere. */
    id: "v10-monokai",
    label: "v10 Monokai",
    preview: {
      background: "#272822",
      card: "#1d1e19",
      secondary: "#3e3d32",
      foreground: "#f8f8f2",
      mutedForeground: "#a59f85",
      border: "rgba(255,255,255,0.10)",
      primary: "#a6e22e",
    },
    fonts: {
      interface: FONT_INTER,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_INTER,
    },
  },
  {
    /** Tokyo Night — by Enkia. Deep blue canvas, cyan-blue accent, calm
     *  and high-contrast. Modern editor staple. */
    id: "v11-tokyo-night",
    label: "v11 Tokyo Night",
    preview: {
      background: "#1a1b26",
      card: "#16161e",
      secondary: "#24283b",
      foreground: "#c0caf5",
      mutedForeground: "#7a88cf",
      border: "rgba(255,255,255,0.08)",
      primary: "#7aa2f7",
    },
    fonts: {
      interface: FONT_INTER,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_INTER,
    },
  },
  {
    /** Mono Vampire — v7 Mono surfaces and fonts (JetBrains Mono across the
     *  board) with a deep blood-red accent in place of cyan. Reads like a
     *  monospace terminal painted in arterial red. */
    id: "v12-mono-vampire",
    label: "v12 Mono Vampire",
    preview: {
      background: "#0d0d0d",
      card: "#050505",
      secondary: "#1a1a1a",
      foreground: "#e5e5e5",
      mutedForeground: "#808080",
      border: "rgba(255,255,255,0.10)",
      primary: "#7f1d1d",
    },
    fonts: {
      interface: FONT_JETBRAINS_MONO,
      terminal: FONT_JETBRAINS_MONO,
      markdown: FONT_JETBRAINS_MONO,
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

/// Find a custom theme by id (returns `undefined` for builtins).
export function findCustomTheme(
  id: string,
  customs: CustomTheme[] | undefined,
): CustomTheme | undefined {
  return customs?.find((c) => c.id === id);
}

/// Convert a `CustomTheme` (overrides on a base) into a flat `Theme` for the
/// picker grid + previews. The base supplies surface tokens (background /
/// card / etc.) and the custom replaces accent + fonts.
export function resolveCustomTheme(custom: CustomTheme): Theme {
  const base = getTheme(custom.base_id);
  return {
    id: custom.id,
    label: custom.label,
    preview: { ...base.preview, primary: custom.primary },
    fonts: { ...custom.fonts },
  };
}

/// All themes the picker should expose: visible builtins + customs.
export function visibleThemesWithCustoms(customs: CustomTheme[]): Theme[] {
  return [...VISIBLE_THEMES, ...customs.map(resolveCustomTheme)];
}

/// Allocate a unique custom theme id (slug-safe).
function newCustomThemeId(): string {
  const random = Math.random().toString(36).slice(2, 8);
  return `custom-${Date.now().toString(36)}-${random}`;
}

/// Auto-generate a label like "v7 Mono Custom", "v7 Mono Custom 2", etc.
function newCustomThemeLabel(baseLabel: string, customs: CustomTheme[]): string {
  const prefix = `${baseLabel} Custom`;
  const taken = new Set(customs.map((c) => c.label));
  if (!taken.has(prefix)) return prefix;
  for (let n = 2; ; n++) {
    const candidate = `${prefix} ${n}`;
    if (!taken.has(candidate)) return candidate;
  }
}

/// Build a `CustomTheme` forked from a builtin. Used by the "Customize"
/// action in Settings → Appearance.
export function forkBuiltinTheme(
  baseId: string,
  existingCustoms: CustomTheme[],
): CustomTheme {
  const base = getTheme(baseId);
  return {
    id: newCustomThemeId(),
    label: newCustomThemeLabel(base.label, existingCustoms),
    base_id: base.id,
    primary: base.preview.primary,
    fonts: { ...base.fonts },
  };
}

/// Snapshot persisted to localStorage for synchronous boot. Stores the
/// fully resolved apply state so we don't need to round-trip through
/// `get_config` to render the right theme on the first paint.
interface ThemeSnapshot {
  /// Theme id — builtin id or custom id (informational).
  id: string;
  /// Drives `data-theme` on `<html>` for surface tokens.
  base: string;
  /// Optional accent override. `null` means "use the CSS default" (clears
  /// any leftover inline `--primary` from a previous custom theme).
  primary: string | null;
  /// Fonts always apply explicitly (CSS vars), independent of base.
  fonts: ThemeFonts;
}

/// Key under which the active theme snapshot is mirrored in localStorage.
export const THEME_CACHE_KEY = "cluihud:theme";

function writeCacheSnapshot(snapshot: ThemeSnapshot): void {
  try {
    localStorage.setItem(THEME_CACHE_KEY, JSON.stringify(snapshot));
  } catch {
    // localStorage unavailable (private mode, quota) — skip the cache;
    // we'll fall back to default on next boot.
  }
}

function readCacheSnapshot(): ThemeSnapshot | null {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(THEME_CACHE_KEY);
  } catch {
    return null;
  }
  if (!raw) return null;
  // Legacy format was a bare theme id string. Migrate to a snapshot using
  // the corresponding builtin's fonts so the boot still applies cleanly.
  if (!raw.startsWith("{")) {
    const id = normalizeThemeId(raw);
    const theme = getTheme(id);
    return { id, base: id, primary: null, fonts: { ...theme.fonts } };
  }
  try {
    return JSON.parse(raw) as ThemeSnapshot;
  } catch {
    return null;
  }
}

/// Apply the active theme to the DOM. When `themeId` matches a custom in
/// `customs`, it inherits surface tokens from `custom.base_id` and overrides
/// `--primary` / `--ring` / `--accent` plus the font stacks.
export function applyTheme(themeId: string, customs?: CustomTheme[]): void {
  const root = document.documentElement;
  const custom = findCustomTheme(themeId, customs);

  let snapshot: ThemeSnapshot;
  if (custom) {
    const baseId = normalizeThemeId(custom.base_id);
    snapshot = {
      id: custom.id,
      base: baseId,
      primary: custom.primary,
      fonts: { ...custom.fonts },
    };
  } else {
    const id = normalizeThemeId(themeId);
    const theme = getTheme(id);
    snapshot = { id, base: id, primary: null, fonts: { ...theme.fonts } };
  }

  applySnapshot(root, snapshot);
  writeCacheSnapshot(snapshot);
}

function applySnapshot(root: HTMLElement, snapshot: ThemeSnapshot): void {
  root.dataset.theme = snapshot.base;
  if (snapshot.primary) {
    root.style.setProperty("--primary", snapshot.primary);
    root.style.setProperty("--ring", snapshot.primary);
    root.style.setProperty("--accent", snapshot.primary);
  } else {
    root.style.removeProperty("--primary");
    root.style.removeProperty("--ring");
    root.style.removeProperty("--accent");
  }
  root.style.setProperty("--theme-font-interface", snapshot.fonts.interface);
  root.style.setProperty("--theme-font-terminal", snapshot.fonts.terminal);
  root.style.setProperty("--theme-font-markdown", snapshot.fonts.markdown);
}

/// Synchronous bootstrap — reads the cached snapshot and applies it before
/// React mounts. Falls back to the default theme on first launch / corrupt
/// cache. The authoritative theme arrives later via `configAtom` and may
/// overwrite this if it diverges.
export function applyCachedTheme(): void {
  const snapshot = readCacheSnapshot();
  if (snapshot) {
    applySnapshot(document.documentElement, snapshot);
    return;
  }
  applyTheme(DEFAULT_THEME_ID);
}
