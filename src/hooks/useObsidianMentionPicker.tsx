import { useCallback, useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import { obsidianEnabledAtom, vaultSearchScopeAtom, obsidianConfigAtom } from "@/stores/obsidian";
import { activeWorkspaceAtom } from "@/stores/workspace";
import type { SearchHit } from "@/stores/search";
import {
  applyMention,
  buildCitation,
  findActiveMention,
  type MentionToken,
} from "@/lib/mentionPicker";
import { MentionPickerOverlay } from "@/components/floating/MentionPickerOverlay";

const DEBOUNCE_MS = 50;
const MAX_RESULTS = 8;
const ROW_HEIGHT_PX = 32;

function noteWikiName(hit: SearchHit): string {
  const base = hit.path.split("/").pop() ?? hit.path;
  return base.replace(/\.md$/i, "");
}

// React-controlled textareas ignore direct `.value` writes; set through the
// native prototype setter + dispatch `input` so the owning component's
// onChange runs and its state stays the source of truth.
function setControlledValue(el: HTMLTextAreaElement, value: string, caret: number) {
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
  setter?.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
  requestAnimationFrame(() => el.setSelectionRange(caret, caret));
}

/// Inline `@@` vault-note picker for a plain textarea. Returns the overlay
/// node (or null) for the caller to render. Decoupled from the textarea's
/// owner — it only needs the ref. No-op unless Obsidian is configured.
export function useObsidianMentionPicker(
  textareaRef: RefObject<HTMLTextAreaElement | null>,
  // Bump when the (possibly conditionally-rendered) textarea mounts/unmounts so
  // the listener effect re-binds to the live element. A stable RefObject alone
  // can't trigger that — `.current` mutation isn't reactive, so without this the
  // effect runs once against a null ref and listeners never attach.
  active = true,
): ReactNode {
  const enabled = useAtomValue(obsidianEnabledAtom);
  const workspace = useAtomValue(activeWorkspaceAtom);
  const scopeMode = useAtomValue(vaultSearchScopeAtom);
  const setScopeMode = useSetAtom(vaultSearchScopeAtom);
  const cfg = useAtomValue(obsidianConfigAtom);

  const [items, setItems] = useState<SearchHit[]>([]);
  const [open, setOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [position, setPosition] = useState({ left: 0, top: 0 });

  // Mirrored into refs so the keydown listener can stay mounted once instead
  // of re-binding on every keystroke result.
  const openRef = useRef(false);
  const itemsRef = useRef<SearchHit[]>([]);
  const selectedRef = useRef(0);
  const tokenRef = useRef<MentionToken | null>(null);
  const anchorRef = useRef<DOMRect | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const seqRef = useRef(0);
  // Set on Esc to the dismissed token's identity, so the Esc keyup (and any
  // later keystroke that leaves the token unchanged) doesn't reopen it.
  const dismissedKeyRef = useRef<string | null>(null);
  // Last token key we actually queried. Lets recompute skip a re-search (which
  // would reset the selection to 0) when the keyup came from nav keys, not a
  // query change.
  const queriedKeyRef = useRef<string | null>(null);

  openRef.current = open;
  itemsRef.current = items;
  selectedRef.current = selectedIndex;
  // Read through refs so the keydown listener + debounced search see the live
  // scope without re-binding the effect on every toggle.
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

  const insertCitation = useCallback(
    (hit: SearchHit) => {
      const el = textareaRef.current;
      const token = tokenRef.current;
      if (!el || !token) return;
      const { text, caret } = applyMention(el.value, token, buildCitation(noteWikiName(hit)));
      setControlledValue(el, text, caret);
      close();
    },
    [close, textareaRef],
  );

  useEffect(() => {
    const el = textareaRef.current;
    if (!el || !enabled || !active) return;

    function recompute() {
      if (!el) return;
      const value = el.value;
      const caret = el.selectionStart ?? value.length;
      const token = findActiveMention(value, caret);
      if (!token) {
        tokenRef.current = null;
        dismissedKeyRef.current = null;
        queriedKeyRef.current = null;
        setOpen(false);
        return;
      }
      const key = `${token.start}:${token.query}`;
      if (dismissedKeyRef.current === key) {
        // Stay dismissed: null token makes the keydown handler inert so Esc
        // falls through to the textarea owner (cancel comment).
        tokenRef.current = null;
        setOpen(false);
        return;
      }
      dismissedKeyRef.current = null;
      tokenRef.current = token;
      anchorRef.current = el.getBoundingClientRect();
      // Same query already on screen (keyup came from Arrow/Enter nav, not a
      // text change) — keep items + selection instead of re-querying to 0.
      if (key === queriedKeyRef.current && openRef.current) return;
      queriedKeyRef.current = key;
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
            activeWorkspaceId: workspace?.id ?? null,
            vaultSubdir: vaultSubdirRef.current,
          });
          if (seq !== seqRef.current) return;
          const rect = anchorRef.current;
          if (rect) {
            const estHeight = Math.min(hits.length, MAX_RESULTS) * ROW_HEIGHT_PX + 8;
            const below = rect.bottom + 4;
            const flip = below + estHeight > window.innerHeight;
            setPosition({ left: rect.left, top: flip ? Math.max(8, rect.top - estHeight - 4) : below });
          }
          setItems(hits);
          setSelectedIndex(0);
          setOpen(hits.length > 0);
        } catch {
          // Picker stays closed on backend error — no toast (it's incidental).
        }
      }, DEBOUNCE_MS);
    }

    function onKeyDown(e: KeyboardEvent) {
      if (!tokenRef.current) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        const t = tokenRef.current;
        dismissedKeyRef.current = t ? `${t.start}:${t.query}` : null;
        close();
        return;
      }
      // Ctrl+D mirrors the search modal's scope toggle (whole vault ⇄ subdir).
      // event.code (not key) for the WebKitGTK layout quirk.
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyD") {
        const avail = subdirAvailRef.current;
        if (!avail) return;
        e.preventDefault();
        e.stopPropagation();
        const next = scopeModeRef.current === "subdir" ? "whole" : "subdir";
        setScopeMode(next);
        scopeModeRef.current = next;
        vaultSubdirRef.current = next === "subdir" ? avail : null;
        queriedKeyRef.current = null; // force a re-query under the new scope
        recompute();
        return;
      }
      if (!openRef.current) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopPropagation();
        setSelectedIndex((p) => Math.min(p + 1, itemsRef.current.length - 1));
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        setSelectedIndex((p) => Math.max(p - 1, 0));
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        // Ctrl+Enter belongs to the textarea owner (submit comment).
        if (e.ctrlKey || e.metaKey) return;
        const hit = itemsRef.current[selectedRef.current];
        if (hit) {
          e.preventDefault();
          e.stopPropagation();
          insertCitation(hit);
        }
      }
    }

    function onBlur() {
      tokenRef.current = null;
      setOpen(false);
    }

    el.addEventListener("keyup", recompute);
    el.addEventListener("click", recompute);
    // Capture beats both the global window shortcut handler and the textarea's
    // own bubble-phase onKeyDown, so Esc/Enter drive the picker first.
    el.addEventListener("keydown", onKeyDown, true);
    el.addEventListener("blur", onBlur);
    return () => {
      el.removeEventListener("keyup", recompute);
      el.removeEventListener("click", recompute);
      el.removeEventListener("keydown", onKeyDown, true);
      el.removeEventListener("blur", onBlur);
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [enabled, workspace?.id, close, insertCitation, textareaRef, active]);

  if (!enabled || !open) return null;
  return (
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
          ? `Ctrl+D: ${scopeMode === "subdir" ? subdirAvailRef.current : "whole vault"}`
          : undefined
      }
    />
  );
}
