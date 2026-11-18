import { describe, expect, it } from "vitest";
import { cursorIndexFor } from "./branchTree";
import { historyTreeItems, historyTreeKeyAction, type HistoryTreeItem } from "./historyTree";

const commits = [{ hash: "aaa" }, { hash: "bbb" }, { hash: "ccc" }];

describe("historyTreeItems", () => {
  it("lists commit rows in order when nothing is expanded", () => {
    expect(historyTreeItems(commits, null, {})).toEqual([
      { kind: "commit", key: "c:aaa", hash: "aaa" },
      { kind: "commit", key: "c:bbb", hash: "bbb" },
      { kind: "commit", key: "c:ccc", hash: "ccc" },
    ]);
  });

  it("places the files of the expanded commit right after it", () => {
    const items = historyTreeItems(commits, "bbb", { bbb: ["src/a.ts", "src/b.ts"] });
    expect(items).toEqual([
      { kind: "commit", key: "c:aaa", hash: "aaa" },
      { kind: "commit", key: "c:bbb", hash: "bbb" },
      { kind: "file", key: "f:bbb:src/a.ts", hash: "bbb", path: "src/a.ts" },
      { kind: "file", key: "f:bbb:src/b.ts", hash: "bbb", path: "src/b.ts" },
      { kind: "commit", key: "c:ccc", hash: "ccc" },
    ]);
  });

  it("omits files of a collapsed commit even when they are loaded", () => {
    const items = historyTreeItems(commits, null, { bbb: ["src/a.ts"] });
    expect(items.map((i) => i.kind)).toEqual(["commit", "commit", "commit"]);
  });

  it("omits files when the expanded commit has none loaded", () => {
    const items = historyTreeItems(commits, "bbb", { aaa: ["src/a.ts"] });
    expect(items.map((i) => i.key)).toEqual(["c:aaa", "c:bbb", "c:ccc"]);
  });

  it("returns an empty list for no commits", () => {
    expect(historyTreeItems([], null, {})).toEqual([]);
  });
});

describe("historyTreeKeyAction", () => {
  const commit: HistoryTreeItem = { kind: "commit", key: "c:aaa", hash: "aaa" };
  const file: HistoryTreeItem = { kind: "file", key: "f:aaa:src/a.ts", hash: "aaa", path: "src/a.ts" };

  it("toggles a commit on Space and Enter", () => {
    expect(historyTreeKeyAction(commit, "Space")).toEqual({ type: "toggle-commit", hash: "aaa" });
    expect(historyTreeKeyAction(commit, "Enter")).toEqual({ type: "toggle-commit", hash: "aaa" });
  });

  it("opens a file on Enter", () => {
    expect(historyTreeKeyAction(file, "Enter")).toEqual({ type: "open-file", hash: "aaa", path: "src/a.ts" });
  });

  it("ignores Space on a file", () => {
    expect(historyTreeKeyAction(file, "Space")).toBeNull();
  });

  it.each(["KeyO", "KeyD", "KeyX"])("ignores %s on both kinds", (code) => {
    expect(historyTreeKeyAction(commit, code)).toBeNull();
    expect(historyTreeKeyAction(file, code)).toBeNull();
  });

  it("returns null without an item", () => {
    expect(historyTreeKeyAction(undefined, "Enter")).toBeNull();
  });
});

describe("cursorIndexFor on history items", () => {
  const items = historyTreeItems(commits, "bbb", { bbb: ["src/a.ts"] });

  it("finds a file key", () => {
    expect(cursorIndexFor(items, "f:bbb:src/a.ts", 0)).toBe(2);
  });

  it("falls back clamped for a missing key", () => {
    expect(cursorIndexFor(items, "f:zzz:gone.ts", 99)).toBe(items.length - 1);
  });
});
