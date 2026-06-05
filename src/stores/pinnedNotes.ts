import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { openTabAction } from "./rightPanel";
import { invoke } from "@/lib/tauri";

function noteName(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

function noteTab(path: string) {
  return { id: `obsidiannote:${path}`, type: "obsidiannote" as const, label: noteName(path), data: { path } };
}

/// Context-injection tier wire strings (mirror of Rust `ContextInjection`).
export type ContextInjectionTier =
  | "append_system_prompt_file"
  | "append_system_prompt"
  | "prompt_preamble"
  | "unsupported";

/// session_id -> absolute vault-note paths pinned to it (single source for
/// #3/#H and the #P panel).
export const pinnedNotesMapAtom = atom<Record<string, string[]>>({});

/// session_id -> the active adapter's injection tier, for the chip's honesty
/// tooltip. Loaded lazily alongside the pins.
export const injectionTierMapAtom = atom<Record<string, ContextInjectionTier>>({});

export const activeSessionPinnedNotesAtom = atom<string[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  return get(pinnedNotesMapAtom)[id] ?? [];
});

export const activeSessionInjectionTierAtom = atom<ContextInjectionTier>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return "unsupported";
  return get(injectionTierMapAtom)[id] ?? "unsupported";
});

export const loadPinnedNotesAtom = atom(null, async (_get, set, sessionId: string) => {
  try {
    const [paths, tier] = await Promise.all([
      invoke<string[]>("list_pinned_notes", { sessionId }),
      invoke<ContextInjectionTier>("get_context_injection_tier", { sessionId }),
    ]);
    set(pinnedNotesMapAtom, (prev) => ({ ...prev, [sessionId]: paths }));
    set(injectionTierMapAtom, (prev) => ({ ...prev, [sessionId]: tier }));
    // Pinned notes live as tabs; restore them (in the background, so activating
    // a session doesn't yank focus onto a note) when the tab state is fresh.
    for (const path of paths) {
      set(openTabAction, { tab: noteTab(path), activate: false });
    }
  } catch (err) {
    console.warn("[pinnedNotes] load failed:", err);
  }
});

export const pinNoteAtom = atom(
  null,
  async (_get, set, args: { sessionId: string; path: string }) => {
    const paths = await invoke<string[]>("pin_vault_note", args);
    set(pinnedNotesMapAtom, (prev) => ({ ...prev, [args.sessionId]: paths }));
    set(openTabAction, { tab: noteTab(args.path) });
    // Deliver the note to the live agent now (labeled as pinned context);
    // persisted pins also seed the system prompt at the next spawn/resume.
    // No-op when the session has no live PTY.
    void invoke("reinject_pinned_note", args).catch(() => {});
    return paths;
  },
);

export const unpinNoteAtom = atom(
  null,
  async (_get, set, args: { sessionId: string; path: string }) => {
    const paths = await invoke<string[]>("unpin_vault_note", args);
    set(pinnedNotesMapAtom, (prev) => ({ ...prev, [args.sessionId]: paths }));
    return paths;
  },
);
