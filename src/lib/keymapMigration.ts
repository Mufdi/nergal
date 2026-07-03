/// Pure startup cleanup for `keymap_overrides`, run once when config loads
/// (see App.tsx). Kept side-effect free so it's testable without mocking
/// Jotai/Tauri: callers own persisting `cleaned` and surfacing `dropped`.
import { comboSignature } from "@/lib/keymap";

export interface DroppedOverride {
  id: string;
  keys: string;
  /// "removed-id": the override targets a shortcut id no longer in the
  /// registry (e.g. pre-restructure `focused-session-*`).
  /// "collision": the override's combo now collides (same signature) with a
  /// NEW default binding on a different id — under first-match-wins dispatch
  /// it would silently shadow that shortcut.
  reason: "removed-id" | "collision";
}

export interface KeymapMigrationResult {
  cleaned: Record<string, string>;
  dropped: DroppedOverride[];
}

/// Drops overrides whose id no longer exists in `registry`, then drops
/// surviving overrides whose signature collides with any OTHER registry
/// entry's default keys. Chord signatures are "L:"-namespaced (see
/// `comboSignature`), so a pre-restructure global override can only ever
/// collide with a global default — the flat signature-equality check below
/// is correct without extra namespace bookkeeping.
export function migrateKeymapOverrides(
  overrides: Record<string, string>,
  registry: { id: string; keys: string }[],
): KeymapMigrationResult {
  const registryIds = new Set(registry.map((entry) => entry.id));
  const cleaned: Record<string, string> = {};
  const dropped: DroppedOverride[] = [];

  for (const [id, keys] of Object.entries(overrides)) {
    if (!registryIds.has(id)) {
      dropped.push({ id, keys, reason: "removed-id" });
      continue;
    }
    const sig = comboSignature(keys);
    const collision = sig
      ? registry.find((entry) => entry.id !== id && comboSignature(entry.keys) === sig)
      : undefined;
    if (collision) {
      dropped.push({ id, keys, reason: "collision" });
      continue;
    }
    cleaned[id] = keys;
  }

  return { cleaned, dropped };
}
