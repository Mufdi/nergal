import { describe, expect, it } from "vitest";
import { mergeAfterConfirm, prViewerKeyAction } from "./prViewerKeys";

type Ev = {
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat: boolean;
};
type Ctx = { pickerOpen: boolean; hunkCount: number; fileCount: number; canMerge: boolean };

function ev(code: string, o: Partial<Ev> = {}): Ev {
  return { code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, ...o };
}

function ctx(o: Partial<Ctx> = {}): Ctx {
  return { pickerOpen: false, hunkCount: 3, fileCount: 3, canMerge: true, ...o };
}

describe("merge key", () => {
  it("maps M and Shift+M to merge when canMerge", () => {
    expect(prViewerKeyAction(ev("KeyM"), ctx())).toBe("merge");
    expect(prViewerKeyAction(ev("KeyM", { shiftKey: true }), ctx())).toBe("merge");
  });

  it("is null when canMerge is false", () => {
    expect(prViewerKeyAction(ev("KeyM"), ctx({ canMerge: false }))).toBeNull();
  });

  it("is null on a held key (repeat)", () => {
    expect(prViewerKeyAction(ev("KeyM", { repeat: true }), ctx())).toBeNull();
  });

  it.each(["ctrlKey", "altKey", "metaKey"] as const)("is null with %s", (mod) => {
    expect(prViewerKeyAction(ev("KeyM", { [mod]: true }), ctx())).toBeNull();
  });

  it("is null while the file picker is open", () => {
    expect(prViewerKeyAction(ev("KeyM"), ctx({ pickerOpen: true }))).toBeNull();
  });
});

describe("Enter never merges", () => {
  it.each(["ctrlKey", "metaKey"] as const)("%s+Enter is null", (mod) => {
    expect(prViewerKeyAction(ev("Enter", { [mod]: true }), ctx())).toBeNull();
  });
});

describe("annotate", () => {
  it("maps KeyA to annotate with hunks", () => {
    expect(prViewerKeyAction(ev("KeyA"), ctx())).toBe("annotate");
  });

  it("is null without hunks", () => {
    expect(prViewerKeyAction(ev("KeyA"), ctx({ hunkCount: 0 }))).toBeNull();
  });
});

describe("hunk navigation", () => {
  it.each([
    ["KeyJ", "hunk-next"],
    ["ArrowDown", "hunk-next"],
    ["KeyK", "hunk-prev"],
    ["ArrowUp", "hunk-prev"],
  ])("%s maps to %s with hunks", (code, action) => {
    expect(prViewerKeyAction(ev(code), ctx())).toBe(action);
  });

  it.each(["KeyJ", "KeyK", "ArrowDown", "ArrowUp"])("%s is null without hunks", (code) => {
    expect(prViewerKeyAction(ev(code), ctx({ hunkCount: 0 }))).toBeNull();
  });
});

describe("toggle collapse", () => {
  it("maps Space to toggle-collapse with hunks", () => {
    expect(prViewerKeyAction(ev("Space"), ctx())).toBe("toggle-collapse");
  });

  it("is null without hunks", () => {
    expect(prViewerKeyAction(ev("Space"), ctx({ hunkCount: 0 }))).toBeNull();
  });

  it.each(["ctrlKey", "metaKey", "altKey", "shiftKey"] as const)("is null with %s", (mod) => {
    expect(prViewerKeyAction(ev("Space", { [mod]: true }), ctx())).toBeNull();
  });
});

describe("file navigation", () => {
  it("Ctrl+Left and Ctrl+Right move between files with 2+ files", () => {
    expect(prViewerKeyAction(ev("ArrowLeft", { ctrlKey: true }), ctx({ fileCount: 2 }))).toBe("file-prev");
    expect(prViewerKeyAction(ev("ArrowRight", { ctrlKey: true }), ctx({ fileCount: 2 }))).toBe("file-next");
  });

  it("is null with fewer than 2 files", () => {
    expect(prViewerKeyAction(ev("ArrowLeft", { ctrlKey: true }), ctx({ fileCount: 1 }))).toBeNull();
    expect(prViewerKeyAction(ev("ArrowRight", { ctrlKey: true }), ctx({ fileCount: 0 }))).toBeNull();
  });
});

describe("file picker open", () => {
  const open = (o: Partial<Ctx> = {}) => ctx({ pickerOpen: true, ...o });

  it("Escape closes the picker", () => {
    expect(prViewerKeyAction(ev("Escape"), open())).toBe("picker-close");
  });

  it("j/k move through files", () => {
    expect(prViewerKeyAction(ev("KeyJ"), open())).toBe("picker-next");
    expect(prViewerKeyAction(ev("KeyK"), open())).toBe("picker-prev");
  });

  it("j/k are null without files", () => {
    expect(prViewerKeyAction(ev("KeyJ"), open({ fileCount: 0 }))).toBeNull();
    expect(prViewerKeyAction(ev("KeyK"), open({ fileCount: 0 }))).toBeNull();
  });

  it("Enter picks", () => {
    expect(prViewerKeyAction(ev("Enter"), open())).toBe("picker-pick");
  });
});

describe("mergeAfterConfirm", () => {
  const captured = { workspaceId: "ws-1", prNumber: 7 };
  const run = (
    confirmed: boolean,
    current: Partial<{ workspaceId: string; prNumber: number; state: string; merging: boolean }> = {},
  ) =>
    mergeAfterConfirm({
      confirmed,
      captured,
      current: { ...captured, state: "OPEN", merging: false, ...current },
    });

  it("merges when confirmed, same PR, open and idle", () => {
    expect(run(true)).toBe(true);
  });

  it("does not merge on cancel", () => {
    expect(run(false)).toBe(false);
  });

  it("does not merge when the PR changed during the dialog", () => {
    expect(run(true, { prNumber: 8 })).toBe(false);
  });

  it("does not merge when the workspace changed during the dialog", () => {
    expect(run(true, { workspaceId: "ws-2" })).toBe(false);
  });

  it("does not merge a PR that is no longer open", () => {
    expect(run(true, { state: "MERGED" })).toBe(false);
  });

  it("does not merge while another merge is running", () => {
    expect(run(true, { merging: true })).toBe(false);
  });
});
