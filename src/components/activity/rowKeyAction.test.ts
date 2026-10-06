import { describe, expect, it } from "vitest";
import { rowKeyAction } from "./rowKeyAction";

describe("rowKeyAction", () => {
  it("toggles the detail on a detail row for Enter and Space", () => {
    expect(rowKeyAction("Enter", { detailId: "e1" })).toEqual({ kind: "toggle-detail", id: "e1" });
    expect(rowKeyAction("Space", { detailId: "e1" })).toEqual({ kind: "toggle-detail", id: "e1" });
  });

  it("opens the file on a file row for Enter, ignores Space", () => {
    expect(rowKeyAction("Enter", { file: "/a/b.ts" })).toEqual({ kind: "open-file", path: "/a/b.ts" });
    expect(rowKeyAction("Space", { file: "/a/b.ts" })).toEqual({ kind: "none" });
  });

  it("keeps Enter on the file when a row has both a file and a detail", () => {
    const row = { file: "/a/b.ts", detailId: "e1" };
    expect(rowKeyAction("Enter", row)).toEqual({ kind: "open-file", path: "/a/b.ts" });
    expect(rowKeyAction("Space", row)).toEqual({ kind: "toggle-detail", id: "e1" });
  });

  it("toggles the group on a group row", () => {
    expect(rowKeyAction("Enter", { groupKey: "g1" })).toEqual({ kind: "toggle-group", key: "g1" });
    expect(rowKeyAction("Space", { groupKey: "g1" })).toEqual({ kind: "toggle-group", key: "g1" });
  });

  it("does nothing on a plain row", () => {
    expect(rowKeyAction("Enter", {})).toEqual({ kind: "none" });
    expect(rowKeyAction("Space", {})).toEqual({ kind: "none" });
  });
});
