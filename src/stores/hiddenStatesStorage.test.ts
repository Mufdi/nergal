import { describe, expect, it } from "vitest";
import {
  HIDDEN_STATES_KEY,
  parseHiddenStates,
  serializeHiddenStates,
  loadHiddenStates,
  saveHiddenStates,
  COLLAPSED_GROUPS_KEY,
  loadStringSet,
  saveStringSet,
} from "./hiddenStatesStorage";

function fakeStorage(initial: Record<string, string> = {}) {
  const data = { ...initial };
  return {
    data,
    getItem: (k: string) => data[k] ?? null,
    setItem: (k: string, v: string) => {
      data[k] = v;
    },
  };
}

describe("hidden states storage", () => {
  it("round-trips a set through a JSON array", () => {
    const raw = serializeHiddenStates(new Set(["done", "canceled"]));
    expect(JSON.parse(raw)).toEqual(["done", "canceled"]);
    expect(parseHiddenStates(raw)).toEqual(new Set(["done", "canceled"]));
  });

  it("tolerates garbage", () => {
    for (const raw of [null, "", "{", "42", '{"a":1}', "null"]) {
      expect(parseHiddenStates(raw)).toEqual(new Set());
    }
  });

  it("drops non-string entries", () => {
    expect(parseHiddenStates('["done", 3, null, "todo"]')).toEqual(new Set(["done", "todo"]));
  });

  it("loads and saves through the injected storage", () => {
    const st = fakeStorage();
    saveHiddenStates(new Set(["done"]), st);
    expect(st.data[HIDDEN_STATES_KEY]).toBe('["done"]');
    expect(loadHiddenStates(st)).toEqual(new Set(["done"]));
  });

  it("survives a throwing storage", () => {
    const boom = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(loadHiddenStates(boom)).toEqual(new Set());
    expect(() => saveHiddenStates(new Set(["x"]), boom)).not.toThrow();
  });
});

describe("keyed string-set storage", () => {
  it("round-trips under an arbitrary key", () => {
    const st = fakeStorage();
    saveStringSet("k", new Set(["a", "b"]), st);
    expect(st.data["k"]).toBe('["a","b"]');
    expect(loadStringSet("k", st)).toEqual(new Set(["a", "b"]));
  });

  it("keeps hidden states and collapsed groups apart", () => {
    const st = fakeStorage();
    saveHiddenStates(new Set(["done"]), st);
    saveStringSet(COLLAPSED_GROUPS_KEY, new Set(["state:abc"]), st);
    expect(loadHiddenStates(st)).toEqual(new Set(["done"]));
    expect(loadStringSet(COLLAPSED_GROUPS_KEY, st)).toEqual(new Set(["state:abc"]));
    expect(COLLAPSED_GROUPS_KEY).not.toBe(HIDDEN_STATES_KEY);
  });

  it("tolerates garbage and throwing storage", () => {
    expect(loadStringSet("k", fakeStorage({ k: "{" }))).toEqual(new Set());
    const boom = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(loadStringSet("k", boom)).toEqual(new Set());
    expect(() => saveStringSet("k", new Set(["x"]), boom)).not.toThrow();
  });
});
