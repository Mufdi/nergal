import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  shortcutRegistryAtom,
  commandPaletteOpenAtom,
} from "@/stores/shortcuts";

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
      if (e.ctrlKey || e.altKey || e.shiftKey) {
        console.log("[cluihud-shortcut]", e.code, { ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey, key: e.key });
      }
      // Ctrl+K toggle
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyK") {
        e.preventDefault();
        e.stopPropagation();
        setPaletteOpen(!paletteOpen);
        return;
      }

      if (paletteOpen) return;

      // Ctrl+Ñ — toggle terminal focus
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "Semicolon") {
        e.preventDefault();
        e.stopPropagation();
        const active = document.activeElement;
        if (active?.closest(".xterm")) {
          (active as HTMLElement).blur();
        } else {
          const textarea = document.querySelector(".xterm-helper-textarea") as HTMLElement | null;
          textarea?.focus();
        }
        return;
      }

      for (const action of registry) {
        const parsed = parseKeys(action.keys);
        if (!parsed) continue;
        if (
          e.ctrlKey === parsed.ctrl &&
          e.shiftKey === parsed.shift &&
          e.altKey === parsed.alt &&
          e.code === parsed.code
        ) {
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
