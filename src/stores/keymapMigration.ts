import { atom } from "jotai";
import type { DroppedOverride } from "@/lib/keymapMigration";

/// Populated once at startup (App.tsx) when the keymap-overrides migration
/// drops entries. Session-lived, not persisted — the migration itself is
/// idempotent (a clean second startup finds nothing to drop), so there is
/// nothing to remember across restarts.
export const droppedKeymapOverridesAtom = atom<DroppedOverride[]>([]);

/// Dismissal for the Settings → Keymap notice listing `droppedKeymapOverridesAtom`.
export const droppedKeymapNoticeDismissedAtom = atom(false);
