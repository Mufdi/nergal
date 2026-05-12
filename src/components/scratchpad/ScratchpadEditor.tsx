import { useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { syntaxHighlighting } from "@codemirror/language";
import { currentHighlightStyle, useThemeName } from "@/lib/codemirrorHighlight";
import { searchKeymap } from "@codemirror/search";
import { indentWithTab } from "@codemirror/commands";
import {
  currentScratchpadSelectionAtom,
  loadTabContentIfNeeded,
  persistTabContent,
  scratchpadContentAtom,
  scratchpadCursorAtom,
  scratchpadDirtyAtom,
  scratchpadConflictAtom,
  scratchpadFocusSignalAtom,
} from "@/stores/scratchpad";
import { appStore } from "@/stores/jotaiStore";

const AUTOSAVE_DEBOUNCE_MS = 300;

const editorTheme = EditorView.theme({
  "&": {
    background: "transparent",
    color: "var(--foreground)",
    height: "100%",
  },
  ".cm-scroller": {
    fontFamily: "var(--font-sans)",
    fontSize: "13px",
    lineHeight: "1.55",
  },
  ".cm-content": {
    caretColor: "#f97316",
    padding: "10px 14px",
  },
  ".cm-cursor": { borderLeftColor: "#f97316" },
  // Match the higher-specificity focused default so the themed orange wins.
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
    backgroundColor: "rgba(249, 115, 22, 0.55)",
  },
  ".cm-selectionLayer .cm-selectionBackground": {
    backgroundColor: "rgba(249, 115, 22, 0.45)",
  },
  ".cm-content ::selection": {
    backgroundColor: "rgba(249, 115, 22, 0.55)",
    color: "inherit",
  },
  ".cm-activeLine": { backgroundColor: "transparent" },
  ".cm-gutters": { display: "none" },
});

interface ScratchpadEditorProps {
  tabId: string;
}

export function ScratchpadEditor({ tabId }: ScratchpadEditorProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const tabIdRef = useRef(tabId);
  const autosaveRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const setSelection = useSetAtom(currentScratchpadSelectionAtom);
  const conflictMap = useAtomValue(scratchpadConflictAtom);
  const conflict = conflictMap[tabId] ?? false;
  const focusSignal = useAtomValue(scratchpadFocusSignalAtom);
  const theme = useThemeName();

  // Keep an up-to-date ref so the autosave closure (created once) sees the
  // current tab — important when the user switches tabs without unmounting.
  tabIdRef.current = tabId;

  // (Re)mount editor whenever the tab id changes.
  useEffect(() => {
    if (!containerRef.current) return;

    let cancelled = false;
    const container = containerRef.current;

    (async () => {
      await loadTabContentIfNeeded(tabId);
      if (cancelled) return;
      const initial = appStore.get(scratchpadContentAtom)[tabId] ?? "";

      const updateListener = EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          appStore.set(scratchpadDirtyAtom, (prev) => ({ ...prev, [tabId]: true }));
          if (autosaveRef.current) clearTimeout(autosaveRef.current);
          autosaveRef.current = setTimeout(() => {
            const view = viewRef.current;
            if (!view) return;
            const content = view.state.doc.toString();
            void persistTabContent(tabIdRef.current, content);
          }, AUTOSAVE_DEBOUNCE_MS);
        }
        if (update.selectionSet || update.docChanged) {
          const sel = update.state.selection.main;
          const text = update.state.sliceDoc(sel.from, sel.to);
          setSelection(text);
        }
      });

      const state = EditorState.create({
        doc: initial,
        extensions: [
          basicSetup,
          editorTheme,
          markdown(),
          syntaxHighlighting(currentHighlightStyle()),
          keymap.of([indentWithTab, ...searchKeymap]),
          updateListener,
          EditorView.lineWrapping,
        ],
      });

      const view = new EditorView({ state, parent: container });
      viewRef.current = view;
      const savedCursor = appStore.get(scratchpadCursorAtom)[tabId];
      const docLen = view.state.doc.length;
      const target = savedCursor !== undefined ? Math.min(savedCursor, docLen) : docLen;
      view.dispatch({
        selection: { anchor: target, head: target },
        scrollIntoView: true,
      });
      view.focus();
    })();

    return () => {
      cancelled = true;
      if (autosaveRef.current) {
        clearTimeout(autosaveRef.current);
        autosaveRef.current = null;
      }
      if (viewRef.current) {
        // Flush any pending edits synchronously to avoid losing the last
        // few keystrokes when the user switches tab mid-typing or toggles
        // the theme (which forces a remount). We seed the in-memory buffer
        // immediately so the next mount can read the live doc without
        // racing the async persistTabContent disk round-trip.
        const view = viewRef.current;
        const content = view.state.doc.toString();
        const id = tabIdRef.current;
        const dirty = appStore.get(scratchpadDirtyAtom)[id] ?? false;
        if (dirty) {
          appStore.set(scratchpadContentAtom, (prev) => ({ ...prev, [id]: content }));
          void persistTabContent(id, content);
        }
        const cursor = view.state.selection.main.head;
        appStore.set(scratchpadCursorAtom, (prev) => ({ ...prev, [id]: cursor }));
        view.destroy();
        viewRef.current = null;
      }
      setSelection("");
    };
  }, [tabId, theme, setSelection]);

  // Re-focus the editor when the panel signals it (open, restored, etc.).
  // Skips the initial render — the mount effect already focused the view.
  useEffect(() => {
    if (focusSignal === 0) return;
    const view = viewRef.current;
    if (!view) return;
    view.focus();
  }, [focusSignal]);

  return (
    <div className="relative flex h-full flex-col">
      {conflict && (
        <div className="bg-yellow-500/10 border-b border-yellow-500/30 px-3 py-1 text-[10px] text-yellow-400 flex items-center justify-between">
          <span>External change detected — save will overwrite the disk version.</span>
        </div>
      )}
      <div ref={containerRef} className="flex-1 min-h-0 overflow-auto" />
    </div>
  );
}
