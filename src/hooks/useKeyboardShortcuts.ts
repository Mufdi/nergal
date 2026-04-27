import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  shortcutRegistryAtom,
  commandPaletteOpenAtom,
} from "@/stores/shortcuts";
import * as terminalService from "@/components/terminal/terminalService";

const KEY_TO_CODE: Record<string, string> = {
  a: "KeyA", b: "KeyB", c: "KeyC", d: "KeyD", e: "KeyE",
  f: "KeyF", g: "KeyG", h: "KeyH", i: "KeyI", j: "KeyJ",
  k: "KeyK", l: "KeyL", m: "KeyM", n: "KeyN", o: "KeyO",
  p: "KeyP", q: "KeyQ", r: "KeyR", s: "KeyS", t: "KeyT",
  u: "KeyU", v: "KeyV", w: "KeyW", x: "KeyX", y: "KeyY",
  z: "KeyZ",
  "1": "Digit1", "2": "Digit2", "3": "Digit3",
  "4": "Digit4", "5": "Digit5", "6": "Digit6",
  "7": "Digit7", "8": "Digit8", "9": "Digit9",
  "0": "Digit0",
  tab: "Tab",
  enter: "Enter",
  backspace: "Backspace",
  arrowleft: "ArrowLeft", arrowright: "ArrowRight",
  arrowup: "ArrowUp", arrowdown: "ArrowDown",
  pagedown: "PageDown", pageup: "PageUp",
  home: "Home", end: "End",
  ñ: "Semicolon",
};

interface ParsedShortcut {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  code: string;
}

function parseKeys(keys: string): ParsedShortcut | null {
  const parts = keys.toLowerCase().split("+");
  const key = parts[parts.length - 1];
  const code = KEY_TO_CODE[key];
  if (!code) return null;
  return {
    ctrl: parts.includes("ctrl"),
    shift: parts.includes("shift"),
    alt: parts.includes("alt"),
    code,
  };
}

export function useKeyboardShortcuts() {
  const registry = useAtomValue(shortcutRegistryAtom);
  const paletteOpen = useAtomValue(commandPaletteOpenAtom);
  const setPaletteOpen = useSetAtom(commandPaletteOpenAtom);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Tab / Shift+Tab routing (single source of truth in capture phase).
      // - Target inside the terminal zone → forward to Claude via the active
      //   PTY directly. We do NOT defer to the textarea's bubble handler
      //   because the bubble route was failing intermittently (focus would
      //   leak / keymap wouldn't fire). Re-focus the textarea on the way
      //   out so the terminal cursor stays visible.
      // - Target in another text input / CodeMirror → native Tab behavior.
      // - Anywhere else → consume silently (no browser focus traversal).
      if (e.code === "Tab" && !e.ctrlKey && !e.altKey) {
        const target = e.target as HTMLElement | null;
        const terminalZone = target?.closest("[data-focus-zone='terminal']");
        if (terminalZone) {
          e.preventDefault();
          e.stopPropagation();
          terminalService.sendSpecialKeyToActive("Tab", "Tab", { shift: e.shiftKey });
          terminalService.focusActive();
          return;
        }
        const inTextInput = target?.tagName === "INPUT"
          || target?.tagName === "TEXTAREA"
          || !!target?.closest(".cm-editor")
          || target?.getAttribute("contenteditable") === "true";
        if (inTextInput) {
          return;
        }
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      // Ctrl+K toggle
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyK") {
        e.preventDefault();
        e.stopPropagation();
        setPaletteOpen(!paletteOpen);
        return;
      }

      if (paletteOpen) return;

      // Modal-open guard: when any dialog is mounted, the modal owns its
      // keyboard space (Ctrl+1/2/3 for action buttons, Ctrl+Enter to
      // confirm, etc.). Without this, Ctrl+Digit shortcuts pass through
      // to global session-switching even while the user is in the Ship
      // modal. Modals attach their own scoped listeners that handle the
      // keys; we just step out of the way.
      const dialogOpen = !!document.querySelector('[data-slot="dialog-content"]');
      if (dialogOpen) return;

      // Ctrl+Ñ — toggle terminal focus
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "Semicolon") {
        e.preventDefault();
        e.stopPropagation();
        const active = document.activeElement;
        const terminalHost = document.querySelector(
          "[data-focus-zone='terminal']",
        ) as HTMLElement | null;
        if (active && terminalHost?.contains(active)) {
          (active as HTMLElement).blur();
        } else {
          const input = terminalHost?.querySelector("textarea") as HTMLElement | null;
          input?.focus();
        }
        return;
      }

      // Let CodeMirror handle Ctrl+S when editor is focused
      const inEditor = (e.target as HTMLElement)?.closest(".cm-editor") != null;

      for (const action of registry) {
        const parsed = parseKeys(action.keys);
        if (!parsed) continue;
        if (
          e.ctrlKey === parsed.ctrl &&
          e.shiftKey === parsed.shift &&
          e.altKey === parsed.alt &&
          e.code === parsed.code
        ) {
          if (inEditor && action.id === "save-file") return;
          e.preventDefault();
          e.stopPropagation();
          action.handler();
          return;
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [registry, paletteOpen, setPaletteOpen]);
}
