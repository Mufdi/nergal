import { useEffect, useState } from "react";

export interface KbdProps {
  /// Shortcut string in `keys` notation (e.g., `"ctrl+shift+y"`).
  /// Matches the registered shortcut id from `stores/shortcuts.ts`.
  keys: string;
  className?: string;
  /// Visual tone — `"subtle"` (default) for muted contexts, `"onPrimary"`
  /// when rendered inside a primary/colored button so the chip stays
  /// legible against the saturated background.
  tone?: "subtle" | "onPrimary";
}

const PLATFORM_GLYPHS: Record<string, { ctrl: string; shift: string; alt: string; meta: string; sep: string }> = {
  mac: { ctrl: "⌃", shift: "⇧", alt: "⌥", meta: "⌘", sep: "" },
  other: { ctrl: "Ctrl", shift: "Shift", alt: "Alt", meta: "Super", sep: "+" },
};

function detectMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent || "";
  return /Mac|iPad|iPhone|iPod/.test(ua);
}

function formatKey(part: string): string {
  if (part.length === 1) return part.toUpperCase();
  if (part === "enter") return "↵";
  if (part === "escape" || part === "esc") return "Esc";
  if (part === "space") return "Space";
  if (part === "tab") return "Tab";
  if (part === "backspace") return "⌫";
  if (part === "arrowup") return "↑";
  if (part === "arrowdown") return "↓";
  if (part === "arrowleft") return "←";
  if (part === "arrowright") return "→";
  return part.charAt(0).toUpperCase() + part.slice(1);
}

/// Compact keyboard-shortcut chip. Renders OS-aware modifier glyphs (⌃⇧⌥⌘
/// on macOS, plain `Ctrl+Shift+Alt+Super` elsewhere) so the user discovers
/// the shortcut without having to hover or open the command palette.
///
/// Pass the same string used in `shortcuts.ts` `keys` field — formatting is
/// handled here so callsites stay terse.
export function Kbd({ keys, className, tone = "subtle" }: KbdProps) {
  const [isMac, setIsMac] = useState(false);
  useEffect(() => {
    setIsMac(detectMac());
  }, []);

  const glyphs = isMac ? PLATFORM_GLYPHS.mac : PLATFORM_GLYPHS.other;
  const parts = keys.toLowerCase().split("+").map((p) => p.trim()).filter(Boolean);
  const rendered = parts.map((p) => {
    if (p === "ctrl" || p === "control") return glyphs.ctrl;
    if (p === "shift") return glyphs.shift;
    if (p === "alt" || p === "option") return glyphs.alt;
    if (p === "meta" || p === "cmd" || p === "command" || p === "super") return glyphs.meta;
    return formatKey(p);
  });
  const text = isMac ? rendered.join("") : rendered.join(glyphs.sep);

  const toneClass = tone === "onPrimary"
    ? "border-current/40 bg-black/25 text-current opacity-80"
    : "border-border/40 bg-background/40 text-muted-foreground/70";

  return (
    <kbd
      className={
        "inline-flex h-4 select-none items-center rounded border px-1 font-mono text-[9px] leading-none " +
        toneClass + " " +
        (className ?? "")
      }
      aria-label={`Shortcut: ${keys}`}
    >
      {text}
    </kbd>
  );
}
