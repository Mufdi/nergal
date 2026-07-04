import { describe, it, expect, vi } from "vitest";

// shortcuts.ts pulls in terminalService/quake/tauri-window transitively; mock
// the invoke/listen surface the same way deepLinkRouter.test.ts does so the
// module graph loads without a live Tauri runtime.
vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(),
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "id") => `${prefix}-test`,
}));

import { shortcutRegistryAtom } from "@/stores/shortcuts";
import { appStore } from "@/stores/jotaiStore";
import { comboSignature } from "@/lib/keymap";

describe("shortcut registry conflict detection", () => {
  it("has no duplicate code+modifier combo across the whole registry", () => {
    // This is the exact class of bug the historical BUG entries trace to: two
    // rows binding the same physical code+modifier combo, one silently
    // shadowing the other at dispatch time. `comboSignature` already
    // namespaces leader continuations (`L:...`) apart from global combos, so a
    // flat equality scan is correct across the whole registry, not just within
    // one category.
    const registry = appStore.get(shortcutRegistryAtom);
    const ownerBySignature = new Map<string, string>();
    const collisions: string[] = [];

    for (const action of registry) {
      const sig = comboSignature(action.keys);
      // null = unparseable by the generic parser (palette-only "" combos, or
      // dedicated-matcher bindings like quake's layout-dependent "ctrl+}") —
      // out of band for this scan, same as validateCombo's own findClash.
      if (sig === null) continue;

      const owner = ownerBySignature.get(sig);
      if (owner) {
        collisions.push(`"${action.id}" (${action.keys}) collides with "${owner}" on signature ${sig}`);
      } else {
        ownerBySignature.set(sig, action.id);
      }
    }

    expect(collisions).toEqual([]);
  });

  it("every registry entry with a non-empty keys string has a stable id", () => {
    // Guards the invariant the conflict scan above depends on: ids must be
    // unique, or a real collision could be masked by two rows sharing an id.
    const registry = appStore.get(shortcutRegistryAtom);
    const ids = registry.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
