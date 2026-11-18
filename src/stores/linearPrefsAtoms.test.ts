import { beforeEach, describe, expect, it, vi } from "vitest";
import { createStore } from "jotai";

const invoke = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@/lib/tauri", () => ({ invoke, listen: vi.fn() }));

import { configAtom } from "./config";
import {
  linearCollapsedGroupsAtom,
  linearHiddenStatesAtom,
  linearShowCompletedAtom,
} from "./linear";

describe("linear panel prefs persist through Nergal config", () => {
  beforeEach(() => invoke.mockClear());

  it("reads the initial value from config", () => {
    const store = createStore();
    store.set(configAtom, { ...store.get(configAtom), linear_hidden_states: ["done"] });
    expect([...store.get(linearHiddenStatesAtom)]).toEqual(["done"]);
  });

  it("writes only the changed key to save_config and updates configAtom", () => {
    const store = createStore();
    store.set(linearHiddenStatesAtom, (prev) => new Set([...prev, "backlog"]));
    expect(invoke).toHaveBeenCalledWith("save_config", {
      config: { linear_hidden_states: ["backlog"] },
    });
    expect(store.get(configAtom).linear_hidden_states).toEqual(["backlog"]);
  });

  it("bounds collapsed groups to the most recent 500", () => {
    const store = createStore();
    store.set(linearCollapsedGroupsAtom, new Set(Array.from({ length: 520 }, (_, i) => `k${i}`)));
    const saved = store.get(configAtom).linear_collapsed_groups ?? [];
    expect(saved).toHaveLength(500);
    expect(saved[499]).toBe("k519");
  });

  it("show-completed defaults to true and reads from config", () => {
    const store = createStore();
    expect(store.get(linearShowCompletedAtom)).toBe(true);
    store.set(configAtom, { ...store.get(configAtom), linear_show_completed: false });
    expect(store.get(linearShowCompletedAtom)).toBe(false);
  });

  it("show-completed writes only its key to save_config", () => {
    const store = createStore();
    store.set(linearShowCompletedAtom, false);
    expect(invoke).toHaveBeenCalledWith("save_config", {
      config: { linear_show_completed: false },
    });
    expect(store.get(configAtom).linear_show_completed).toBe(false);
  });
});
