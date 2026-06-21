import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  resolvedShortcutsAtom,
  keymapOverridesAtom,
  keymapCaptureActiveAtom,
  commandPaletteOpenAtom,
  toggleQuake,
} from "@/stores/shortcuts";
import { parseKeys } from "@/lib/keymap";
import { zenModeAtom, prZenAtom } from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import {
  scratchpadOpenAtom,
  scratchpadActiveTabIdAtom,
  closeScratchTab,
  createNewScratchTab,
  cycleScratchTab,
  restoreLastClosedScratchTab,
} from "@/stores/scratchpad";
import { appStore } from "@/stores/jotaiStore";
import { activeSessionIdAtom } from "@/stores/workspace";
import { addAdHocShell, closeActiveQuakeShell } from "@/stores/quake";
import * as terminalService from "@/components/terminal/terminalService";

export function useKeyboardShortcuts() {
  const registry = useAtomValue(resolvedShortcutsAtom);
  const overrides = useAtomValue(keymapOverridesAtom);
  const paletteOpen = useAtomValue(commandPaletteOpenAtom);
  const setPaletteOpen = useSetAtom(commandPaletteOpenAtom);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const zenOpen = zenState.open || conflictsZen || prZen !== null;

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // While the keymap editor is recording, every global shortcut steps
      // aside so the keystroke reaches only the recorder.
      if (appStore.get(keymapCaptureActiveAtom)) return;
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
        // Quake shells need the same capture-phase route — the bubble path
        // this fix replaced was failing intermittently for center, and the
        // quake textarea has the identical wiring.
        if (target?.closest("[data-focus-zone='quake']")) {
          e.preventDefault();
          e.stopPropagation();
          terminalService.sendSpecialKeyToActive("Tab", "Tab", { shift: e.shiftKey }, "quake");
          terminalService.focusActive("quake");
          return;
        }
        const inTextInput = target?.tagName === "INPUT"
          || target?.tagName === "TEXTAREA"
          || !!target?.closest(".cm-editor")
          || target?.getAttribute("contenteditable") === "true";
        // The worktree gate is a self-contained form (prompt → branch → agent →
        // preset → buttons) — let native Tab traverse its controls instead of
        // consuming the key, so the user can move off the picker triggers.
        const inGate = !!target?.closest("[data-focus-zone='worktree-gate']");
        if (inTextInput || inGate) {
          return;
        }
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      // Quake-focused overrides: Ctrl+W closes the active shell tab (the
      // global binding would soft-close the whole session) and Ctrl+Shift+T
      // opens a new one (terminal-emulator muscle memory; plain Ctrl+T
      // belongs to the shell itself).
      {
        const target = e.target as HTMLElement | null;
        if (target?.closest("[data-focus-zone='quake']") && e.ctrlKey && !e.altKey) {
          if (!e.shiftKey && e.code === "KeyW") {
            e.preventDefault();
            e.stopPropagation();
            closeActiveQuakeShell();
            return;
          }
          if (e.shiftKey && e.code === "KeyT") {
            e.preventDefault();
            e.stopPropagation();
            const sid = appStore.get(activeSessionIdAtom);
            if (sid) addAdHocShell(sid);
            return;
          }
        }
      }

      // Ctrl+K toggle
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyK") {
        e.preventDefault();
        e.stopPropagation();
        setPaletteOpen(!paletteOpen);
        return;
      }

      // Scratchpad-focused override: when the panel is open AND focus is
      // inside it, hijack Ctrl+Tab/Ctrl+Shift+Tab/Ctrl+W/Ctrl+Shift+T
      // so they operate on scratchpad tabs instead of session/right-panel
      // tabs. Other shortcuts (Ctrl+B, Ctrl+1..9, Ctrl+S, Ctrl+Alt+L) keep
      // working as global controls.
      const scratchpadOpen = appStore.get(scratchpadOpenAtom);
      if (scratchpadOpen) {
        const target = e.target as HTMLElement | null;
        const inScratchpad = !!target?.closest('[data-floating-panel-id="scratchpad"]');
        if (inScratchpad) {
          if (e.ctrlKey && !e.altKey && e.code === "Tab") {
            e.preventDefault();
            e.stopPropagation();
            cycleScratchTab(e.shiftKey ? -1 : 1);
            return;
          }
          if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyW") {
            e.preventDefault();
            e.stopPropagation();
            const activeId = appStore.get(scratchpadActiveTabIdAtom);
            if (activeId) void closeScratchTab(activeId);
            return;
          }
          if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyT") {
            e.preventDefault();
            e.stopPropagation();
            void createNewScratchTab();
            return;
          }
          if (e.ctrlKey && e.shiftKey && !e.altKey && e.code === "KeyT") {
            e.preventDefault();
            e.stopPropagation();
            void restoreLastClosedScratchTab();
            return;
          }
        }
      }

      // Browser-panel reserved shortcuts (Ctrl+T/Ctrl+W/Ctrl+Tab/Ctrl+R/F5/
      // Ctrl+Shift+0/Ctrl+Shift+R) are now intercepted at the Tauri
      // runtime layer when the browser panel is visible — see
      // src-tauri/src/browser.rs::RESERVED_SHORTCUTS. That avoids the
      // cross-origin iframe focus trap so they fire even when the user is
      // clicking around inside the embedded SPA. The previous JS-level
      // hijack on this hook was redundant once OS-level capture took
      // over.

      if (paletteOpen) return;

      // Modal-open guard: when any dialog is mounted, the modal owns its
      // keyboard space (Ctrl+1/2/3 for action buttons, Ctrl+Enter to
      // confirm, etc.). Without this, Ctrl+Digit shortcuts pass through
      // to global session-switching even while the user is in the Ship
      // modal. Modals attach their own scoped listeners that handle the
      // keys; we just step out of the way.
      const dialogOpen = !!document.querySelector('[data-slot="dialog-content"]');
      if (dialogOpen) return;

      // Zen-open guard for alt+arrow: ZenMode's own listener also runs at
      // window capture, but mount order means this hook is registered first
      // and stopImmediatePropagation in Zen can't help. Bail here so the
      // background panels don't flash via focusZone()'s zone-flash class
      // and so alt+↑/↓ doesn't slide hidden session/panel cursors. Esc and
      // every other shortcut still flows normally.
      if (zenOpen && e.altKey && !e.ctrlKey && !e.shiftKey && !e.metaKey && (
        e.code === "ArrowLeft" || e.code === "ArrowRight" ||
        e.code === "ArrowUp" || e.code === "ArrowDown"
      )) {
        return;
      }

      // Let CodeMirror handle Ctrl+S when editor is focused
      const target = e.target as HTMLElement | null;
      const inEditor = target?.closest(".cm-editor") != null;
      // Ctrl+Enter is a submit gesture in non-terminal text fields (stash
      // message, commit message, PR comment) — those scoped handlers win.
      const inNonTerminalField =
        ((target?.tagName === "INPUT" || target?.tagName === "TEXTAREA"
          || inEditor || target?.getAttribute("contenteditable") === "true")
          // The worktree gate owns Ctrl+Enter (commit edit) while focused, so
          // the global fullscreen must not also fire from a gate button/row.
          || !!target?.closest("[data-focus-zone='worktree-gate']"))
        && !target?.closest("[data-focus-zone='terminal']");

      // Ctrl+} — dual matching because the `}` glyph is layout-dependent:
      // `e.key` covers layouts that type it via AltGr (on Linux AltGr sets
      // neither ctrlKey nor altKey, so real Ctrl is what matches), `e.code`
      // covers US-physical Shift+BracketRight per the event.code convention.
      // `!e.altKey` keeps layouts where AltGr reports as Ctrl+Alt from
      // toggling on a plain `}` keystroke.
      // Skip the dual-match when the user has rebound toggle-quake — its
      // override is a normal combo resolved by the registry loop below.
      if (
        !overrides["toggle-quake"]
        && e.ctrlKey && !e.altKey
        && (e.key === "}" || (e.shiftKey && e.code === "BracketRight"))
      ) {
        e.preventDefault();
        e.stopPropagation();
        toggleQuake();
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
          if (inEditor && action.id === "save-file") return;
          if (inNonTerminalField && action.id === "fullscreen-terminal") return;
          e.preventDefault();
          e.stopPropagation();
          action.handler();
          return;
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [registry, overrides, paletteOpen, setPaletteOpen, zenOpen]);
}
