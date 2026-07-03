import { describe, it, expect } from "vitest";
import { migrateKeymapOverrides } from "@/lib/keymapMigration";

const registry = [
  { id: "ship-session", keys: "ctrl+shift+enter" },
  { id: "open-ide", keys: "leader e" },
  { id: "toggle-sidebar", keys: "ctrl+b" },
];

describe("migrateKeymapOverrides", () => {
  it("drops overrides for ids no longer in the registry", () => {
    const { cleaned, dropped } = migrateKeymapOverrides(
      { "focused-session-3": "ctrl+shift+3" },
      registry,
    );
    expect(cleaned).toEqual({});
    expect(dropped).toEqual([
      { id: "focused-session-3", keys: "ctrl+shift+3", reason: "removed-id" },
    ]);
  });

  it("drops surviving overrides that collide with a new default on another id", () => {
    // Pre-restructure override, valid at the time — now shadows ship-session's new default.
    const { cleaned, dropped } = migrateKeymapOverrides(
      { "open-ide": "ctrl+shift+enter" },
      registry,
    );
    expect(cleaned).toEqual({});
    expect(dropped).toEqual([
      { id: "open-ide", keys: "ctrl+shift+enter", reason: "collision" },
    ]);
  });

  it("keeps non-colliding overrides for surviving ids", () => {
    const { cleaned, dropped } = migrateKeymapOverrides(
      { "toggle-sidebar": "ctrl+alt+b" },
      registry,
    );
    expect(cleaned).toEqual({ "toggle-sidebar": "ctrl+alt+b" });
    expect(dropped).toEqual([]);
  });

  it("is a no-op on an already-clean config (idempotent)", () => {
    const { cleaned, dropped } = migrateKeymapOverrides({}, registry);
    expect(cleaned).toEqual({});
    expect(dropped).toEqual([]);
  });

  it("does not flag a leader-chord override against a global default sharing the same underlying key", () => {
    // "L:"-namespaced signature keeps this from colliding with toggle-sidebar's ctrl+b.
    const { cleaned, dropped } = migrateKeymapOverrides(
      { "open-ide": "leader b" },
      registry,
    );
    expect(cleaned).toEqual({ "open-ide": "leader b" });
    expect(dropped).toEqual([]);
  });
});
