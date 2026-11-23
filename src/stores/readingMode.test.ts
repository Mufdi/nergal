import { beforeEach, describe, expect, it, vi } from "vitest";
import { createStore } from "jotai";

const invoke = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@/lib/tauri", () => ({
  invoke,
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "id") => `${prefix}-test`,
}));

const toasts = vi.hoisted(() => vi.fn());
vi.mock("@/stores/toast", async () => {
  const { atom: a } = await import("jotai");
  return { toastsAtom: a(null, (_get: unknown, _set: unknown, toast: unknown) => toasts(toast)) };
});

import type { Config } from "@/lib/types";
import { configAtom, configLoadedAtom } from "@/stores/config";
import {
  readingFullWidthTablesAtom,
  readingMarkdownAtom,
  readingModeTarget,
  readingTerminalAtom,
  toggleReadingMode,
} from "@/stores/readingMode";
import { shortcutRegistryAtom, type FocusZone } from "@/stores/shortcuts";
import { appStore } from "@/stores/jotaiStore";


function loadedStore() {
  const store = createStore();
  store.set(configLoadedAtom, true);
  return store;
}

describe("reading flags persist through Nergal config", () => {
  beforeEach(() => {
    invoke.mockClear();
    toasts.mockClear();
  });

  it("both flags read false from a config without the keys", () => {
    const store = createStore();
    expect(store.get(readingTerminalAtom)).toBe(false);
    expect(store.get(readingMarkdownAtom)).toBe(false);
  });

  it("the terminal flag saves only reading_mode_terminal", () => {
    const store = createStore();
    store.set(readingTerminalAtom, true);
    expect(invoke).toHaveBeenCalledWith("save_config", {
      config: { reading_mode_terminal: true },
    });
    expect(store.get(configAtom).reading_mode_terminal).toBe(true);
    expect(store.get(readingMarkdownAtom)).toBe(false);
  });

  it("the markdown flag saves only reading_mode_markdown", () => {
    const store = createStore();
    store.set(readingMarkdownAtom, true);
    expect(invoke).toHaveBeenCalledWith("save_config", {
      config: { reading_mode_markdown: true },
    });
    expect(store.get(configAtom).reading_mode_markdown).toBe(true);
    expect(store.get(readingTerminalAtom)).toBe(false);
  });
});

describe("full-width tables flag", () => {
  it("reads true when the config lacks the key", () => {
    const store = createStore();
    const config = { ...store.get(configAtom) } as Record<string, unknown>;
    delete config.reading_mode_full_width_tables;
    store.set(configAtom, config as unknown as Config);
    expect(store.get(readingFullWidthTablesAtom)).toBe(true);
  });

  it("reads false when the config holds false", () => {
    const store = createStore();
    store.set(configAtom, { ...store.get(configAtom), reading_mode_full_width_tables: false });
    expect(store.get(readingFullWidthTablesAtom)).toBe(false);
  });
});

describe("reading mode follows the focused surface", () => {
  beforeEach(() => {
    invoke.mockClear();
    toasts.mockClear();
  });

  it.each<[FocusZone, "terminal" | "markdown" | null]>([
    ["terminal", "terminal"],
    ["panel", "markdown"],
    ["sidebar", null],
    ["quake", null],
  ])("zone %s targets %s", (zone, target) => {
    expect(readingModeTarget(zone)).toBe(target);
  });

  it("terminal zone flips only the terminal flag and toasts on, then off", () => {
    const store = loadedStore();
    toggleReadingMode(store, "terminal");
    expect(store.get(readingTerminalAtom)).toBe(true);
    expect(store.get(readingMarkdownAtom)).toBe(false);
    expect(toasts).toHaveBeenLastCalledWith({ message: "Reading mode: terminal on", type: "info" });

    toggleReadingMode(store, "terminal");
    expect(store.get(readingTerminalAtom)).toBe(false);
    expect(toasts).toHaveBeenLastCalledWith({ message: "Reading mode: terminal off", type: "info" });
  });

  it("panel zone flips only the markdown flag", () => {
    const store = loadedStore();
    toggleReadingMode(store, "panel");
    expect(store.get(readingMarkdownAtom)).toBe(true);
    expect(store.get(readingTerminalAtom)).toBe(false);
    expect(toasts).toHaveBeenLastCalledWith({ message: "Reading mode: markdown on", type: "info" });
  });

  it.each<FocusZone>(["sidebar", "quake"])("zone %s changes nothing and shows no toast", (zone) => {
    const store = loadedStore();
    toggleReadingMode(store, zone);
    expect(store.get(readingTerminalAtom)).toBe(false);
    expect(store.get(readingMarkdownAtom)).toBe(false);
    expect(toasts).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it.each<FocusZone>(["terminal", "panel", "sidebar", "quake"])(
    "before the config loads zone %s changes nothing",
    (zone) => {
      const store = createStore();
      toggleReadingMode(store, zone);
      expect(store.get(readingTerminalAtom)).toBe(false);
      expect(store.get(readingMarkdownAtom)).toBe(false);
      expect(invoke).not.toHaveBeenCalled();
    },
  );
});

describe("reading mode shortcut registration", () => {
  it("toggle-reading-mode owns leader f in the surfaces group", () => {
    const registry = appStore.get(shortcutRegistryAtom);
    const owners = registry.filter((a) => a.keys === "leader f");
    expect(owners.map((a) => a.id)).toEqual(["toggle-reading-mode"]);
    expect(owners[0].group).toBe("surfaces");
  });
});
