import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { Prec, type Extension } from "@codemirror/state";
import { EditorView, keymap, type ViewUpdate } from "@codemirror/view";
import { invoke } from "@/lib/tauri";
import { obsidianEnabledAtom, vaultSearchScopeAtom, obsidianConfigAtom } from "@/stores/obsidian";
import { activeWorkspaceAtom, activeSessionIdAtom } from "@/stores/workspace";
import { pinNoteAtom } from "@/stores/pinnedNotes";
import type { SearchHit } from "@/stores/search";
import { buildCitation, findActiveMention, type MentionToken } from "@/lib/mentionPicker";
import { MentionPickerOverlay } from "@/components/floating/MentionPickerOverlay";

const DEBOUNCE_MS = 50;
const MAX_RESULTS = 8;
const ROW_HEIGHT_PX = 32;

function noteWikiName(hit: SearchHit): string {
  const base = hit.path.split("/").pop() ?? hit.path;
  return base.replace(/\.md$/i, "");
}

/// Inline `@@` vault-note picker for a CodeMirror 6 editor (the scratchpad).
/// The textarea hook can't drive CM — it has no `value`/`selectionStart` and
/// React-controlled writes don't apply. This returns a CM `extension` the
/// editor must include (an update listener + a high-precedence keymap) plus the
/// overlay node to render. No-op unless Obsidian is configured.
export function useCodeMirrorMentionPicker(): { extension: Extension; overlay: ReactNode } {
  const enabled = useAtomValue(obsidianEnabledAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const scopeMode = useAtomValue(vaultSearchScopeAtom);
  const setScopeMode = useSetAtom(vaultSearchScopeAtom);
  const cfg = useAtomValue(obsidianConfigAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const pinNote = useSetAtom(pinNoteAtom);
  const sessionIdRef = useRef<string | null>(null);
  sessionIdRef.current = activeSessionId;

  const [items, setItems] = useState<SearchHit[]>([]);
  const [open, setOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [position, setPosition] = useState({ left: 0, top: 0 });

  const openRef = useRef(false);
  const itemsRef = useRef<SearchHit[]>([]);
  const selectedRef = useRef(0);
  const tokenRef = useRef<MentionToken | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const seqRef = useRef(0);
  const dismissedKeyRef = useRef<string | null>(null);
  const queriedKeyRef = useRef<string | null>(null);

  openRef.current = open;
  itemsRef.current = items;
  selectedRef.current = selectedIndex;

  const enabledRef = useRef(enabled);
  enabledRef.current = enabled;
  const workspaceIdRef = useRef<string | null>(null);
  workspaceIdRef.current = workspace?.id ?? null;
  const subdirAvailRef = useRef<string | null>(null);
  subdirAvailRef.current = cfg?.search_subdir?.trim() || null;
  const scopeModeRef = useRef(scopeMode);
  scopeModeRef.current = scopeMode;
  const vaultSubdirRef = useRef<string | null>(null);
  vaultSubdirRef.current = scopeMode === "subdir" ? subdirAvailRef.current : null;

  const close = useCallback(() => {
    tokenRef.current = null;
    queriedKeyRef.current = null;
    setOpen(false);
    setItems([]);
  }, []);

  const insertCitation = useCallback((hit: SearchHit) => {
    const view = viewRef.current;
    const token = tokenRef.current;
    if (!view || !token) return;
    const insert = buildCitation(noteWikiName(hit));
    view.dispatch({
      changes: { from: token.start, to: token.end, insert },
      selection: { anchor: token.start + insert.length },
    });
    view.focus();
    close();
  }, [close]);

  // Shift+Enter pins to the active session instead of citing inline.
  const pinHit = useCallback((hit: SearchHit) => {
    const sid = sessionIdRef.current;
    if (sid) void pinNote({ sessionId: sid, path: hit.path });
    viewRef.current?.focus();
    close();
  }, [close, pinNote]);

  const recompute = useCallback(
    (view: EditorView) => {
      if (!enabledRef.current) return;
      const caret = view.state.selection.main.head;
      const text = view.state.doc.toString();
      const token = findActiveMention(text, caret);
      if (!token) {
        tokenRef.current = null;
        dismissedKeyRef.current = null;
        queriedKeyRef.current = null;
        setOpen(false);
        return;
      }
      const key = `${token.start}:${token.query}`;
      if (dismissedKeyRef.current === key) {
        tokenRef.current = null;
        setOpen(false);
        return;
      }
      dismissedKeyRef.current = null;
      tokenRef.current = token;
      // Same query already on screen (caret nav, not a text change) — keep
      // items + selection instead of re-querying back to 0.
      if (key === queriedKeyRef.current && openRef.current) return;
      queriedKeyRef.current = key;

      const coords = view.coordsAtPos(token.start);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      const seq = ++seqRef.current;
      debounceRef.current = setTimeout(async () => {
        try {
          const hits = await invoke<SearchHit[]>("search", {
            query: {
              text: token.query,
              scopes: [{ kind: "vault" }],
              titlesOnly: true,
              maxResults: MAX_RESULTS,
            },
            activeWorkspaceId: workspaceIdRef.current,
            vaultSubdir: vaultSubdirRef.current,
          });
          if (seq !== seqRef.current) return;
          if (coords) {
            const estHeight = Math.min(hits.length, MAX_RESULTS) * ROW_HEIGHT_PX + 8;
            const below = coords.bottom + 4;
            const flip = below + estHeight > window.innerHeight;
            setPosition({
              left: coords.left,
              top: flip ? Math.max(8, coords.top - estHeight - 4) : below,
            });
          }
          setItems(hits);
          setSelectedIndex(0);
          setOpen(hits.length > 0);
        } catch {
          // Picker stays closed on backend error — incidental, no toast.
        }
      }, DEBOUNCE_MS);
    },
    [],
  );

  const extension = useMemo<Extension>(() => {
    const listener = EditorView.updateListener.of((update: ViewUpdate) => {
      viewRef.current = update.view;
      if (update.docChanged || update.selectionSet) {
        recompute(update.view);
      }
    });

    // Highest precedence so the picker consumes nav/commit keys before CM's
    // default keymap (Enter newline, Tab indent, arrows move caret).
    const navKeys = Prec.highest(
      keymap.of([
        {
          key: "Escape",
          run: () => {
            const t = tokenRef.current;
            if (!t) return false;
            dismissedKeyRef.current = `${t.start}:${t.query}`;
            close();
            return true;
          },
        },
        {
          key: "Mod-d",
          run: () => {
            const avail = subdirAvailRef.current;
            if (!tokenRef.current || !avail) return false;
            const next = scopeModeRef.current === "subdir" ? "whole" : "subdir";
            setScopeMode(next);
            scopeModeRef.current = next;
            vaultSubdirRef.current = next === "subdir" ? avail : null;
            queriedKeyRef.current = null;
            if (viewRef.current) recompute(viewRef.current);
            return true;
          },
        },
        {
          key: "ArrowDown",
          run: () => {
            if (!openRef.current) return false;
            setSelectedIndex((p) => Math.min(p + 1, itemsRef.current.length - 1));
            return true;
          },
        },
        {
          key: "ArrowUp",
          run: () => {
            if (!openRef.current) return false;
            setSelectedIndex((p) => Math.max(p - 1, 0));
            return true;
          },
        },
        {
          key: "Shift-Enter",
          run: () => {
            if (!openRef.current) return false;
            const hit = itemsRef.current[selectedRef.current];
            if (!hit) return false;
            pinHit(hit);
            return true;
          },
        },
        {
          key: "Enter",
          run: () => {
            if (!openRef.current) return false;
            const hit = itemsRef.current[selectedRef.current];
            if (!hit) return false;
            insertCitation(hit);
            return true;
          },
        },
        {
          key: "Tab",
          run: () => {
            if (!openRef.current) return false;
            const hit = itemsRef.current[selectedRef.current];
            if (!hit) return false;
            insertCitation(hit);
            return true;
          },
        },
      ]),
    );

    return [listener, navKeys];
  }, [recompute, close, insertCitation, pinHit, setScopeMode]);

  const overlay =
    enabled && open ? (
      <MentionPickerOverlay
        items={items.map((h) => ({
          key: h.path,
          label: h.title ?? noteWikiName(h),
          sublabel: h.lineText || undefined,
        }))}
        selectedIndex={selectedIndex}
        position={position}
        onSelect={(idx) => {
          const hit = items[idx];
          if (hit) insertCitation(hit);
        }}
        onHover={setSelectedIndex}
        hint={
          subdirAvailRef.current
            ? `Ctrl+D: ${scopeMode === "subdir" ? subdirAvailRef.current : "whole vault"} · Shift+Enter: pin`
            : "Shift+Enter: pin to session"
        }
      />
    ) : null;

  return { extension, overlay };
}
