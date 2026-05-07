import { useEffect, useState } from "react";
import { HighlightStyle } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";

// Light variant tuned for cluihud's white card surface (`v1-light`).
// Token families kept to ~6-8 hue groups so syntax stays legible without
// turning into a rainbow on long files. Greens/blues/violets sit on the
// saturated-but-readable end of the Tailwind palette.
export const lightHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: "#7c3aed" },
  {
    tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName],
    color: "#0a0a0b",
  },
  { tag: [t.function(t.variableName), t.labelName], color: "#2563eb" },
  {
    tag: [t.color, t.constant(t.name), t.standard(t.name)],
    color: "#0891b2",
  },
  { tag: [t.definition(t.name), t.separator], color: "#0a0a0b" },
  {
    tag: [
      t.typeName,
      t.className,
      t.number,
      t.changed,
      t.annotation,
      t.modifier,
      t.self,
      t.namespace,
    ],
    color: "#0891b2",
  },
  {
    tag: [
      t.operator,
      t.operatorKeyword,
      t.url,
      t.escape,
      t.regexp,
      t.link,
      t.special(t.string),
    ],
    color: "#dc2626",
  },
  { tag: [t.meta, t.comment], color: "#71717a", fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: "#2563eb", textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: "#7c3aed" },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: "#dc2626" },
  {
    tag: [t.processingInstruction, t.string, t.inserted],
    color: "#16a34a",
  },
  { tag: t.invalid, color: "#dc2626" },
]);

export { oneDarkHighlightStyle as darkHighlightStyle };

export type ThemeName = "v1-light" | "v1-dark";

function readTheme(): ThemeName {
  if (typeof document === "undefined") return "v1-dark";
  return document.documentElement.dataset.theme === "v1-light"
    ? "v1-light"
    : "v1-dark";
}

/// Resolves the `HighlightStyle` matching the current `data-theme` on
/// `<html>`. Called inside CodeMirror extension arrays so each editor mount
/// picks up whichever theme was active at construction time.
export function currentHighlightStyle(): HighlightStyle {
  return readTheme() === "v1-light"
    ? lightHighlightStyle
    : oneDarkHighlightStyle;
}

/// Subscribes to `data-theme` mutations on `<html>` and returns the current
/// value. Editors include the result in their main effect's dependency array
/// so the EditorView remounts on theme swap — `syntaxHighlighting` is part of
/// the initial `extensions` and can't be reconfigured cheaply without a
/// Compartment, and full remount is simpler than threading one through.
export function useThemeName(): ThemeName {
  const [theme, setTheme] = useState<ThemeName>(readTheme);

  useEffect(() => {
    const root = document.documentElement;
    const observer = new MutationObserver(() => {
      const next = readTheme();
      setTheme((prev) => (prev === next ? prev : next));
    });
    observer.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, []);

  return theme;
}
