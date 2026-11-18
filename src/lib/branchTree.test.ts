import { describe, expect, it } from "vitest";
import {
  branchKey,
  branchTreeItems,
  branchTreeKeyAction,
  changedBranches,
  cursorIndexFor,
  openSessionConfirm,
  type BranchTreeItem,
  type BranchTreeState,
} from "./branchTree";
import type { LocalBranchRow } from "./localBranches";

function row(overrides: Partial<LocalBranchRow> = {}): LocalBranchRow {
  return {
    name: "feat/a",
    sha: "aaa111",
    pending: 1,
    last_commit_unix: 1700000000,
    last_subject: "subject",
    session: null,
    in_use: false,
    missing_worktree: null,
    ...overrides,
  };
}

function state(overrides: Partial<BranchTreeState> = {}): BranchTreeState {
  return {
    expandedBranch: null,
    expandedCommit: null,
    branchCommits: {},
    commitFiles: {},
    ...overrides,
  };
}

const commits = [
  { hash: "h1", message: "first" },
  { hash: "h2", message: "second" },
];

describe("branchKey", () => {
  it("prefixes the branch name with b:", () => {
    expect(branchKey("feat/a")).toBe("b:feat/a");
  });
});

describe("branchTreeItems", () => {
  const rows = [row({ name: "feat/a" }), row({ name: "feat/b" })];

  it("lists only branch rows when nothing is expanded", () => {
    const items = branchTreeItems(rows, state());
    expect(items.map((i) => i.key)).toEqual(["b:feat/a", "b:feat/b"]);
    expect(items.every((i) => i.kind === "branch")).toBe(true);
  });

  it("places the commits of the expanded branch right after it in given order", () => {
    const items = branchTreeItems(
      rows,
      state({ expandedBranch: "feat/a", branchCommits: { "feat/a": commits } }),
    );
    expect(items.map((i) => i.key)).toEqual([
      "b:feat/a",
      "c:feat/a:h1",
      "c:feat/a:h2",
      "b:feat/b",
    ]);
  });

  it("carries hash and message on commit items", () => {
    const items = branchTreeItems(
      rows,
      state({ expandedBranch: "feat/a", branchCommits: { "feat/a": commits } }),
    );
    expect(items[1]).toEqual({
      kind: "commit",
      key: "c:feat/a:h1",
      branch: "feat/a",
      hash: "h1",
      message: "first",
    });
  });

  it("shows no commits for an expanded branch without loaded commits", () => {
    const items = branchTreeItems(rows, state({ expandedBranch: "feat/a" }));
    expect(items.map((i) => i.key)).toEqual(["b:feat/a", "b:feat/b"]);
  });

  it("shows no commits for a collapsed branch even when commits are loaded", () => {
    const items = branchTreeItems(
      rows,
      state({ expandedBranch: null, branchCommits: { "feat/a": commits } }),
    );
    expect(items).toHaveLength(2);
  });

  it("places files of the expanded commit right after that commit", () => {
    const items = branchTreeItems(
      rows,
      state({
        expandedBranch: "feat/a",
        expandedCommit: "h1",
        branchCommits: { "feat/a": commits },
        commitFiles: { h1: ["src/x.ts", "README.md"] },
      }),
    );
    expect(items.map((i) => i.key)).toEqual([
      "b:feat/a",
      "c:feat/a:h1",
      "f:feat/a:h1:src/x.ts",
      "f:feat/a:h1:README.md",
      "c:feat/a:h2",
      "b:feat/b",
    ]);
  });

  it("carries branch, hash and path on file items", () => {
    const items = branchTreeItems(
      rows,
      state({
        expandedBranch: "feat/a",
        expandedCommit: "h1",
        branchCommits: { "feat/a": commits },
        commitFiles: { h1: ["src/x.ts"] },
      }),
    );
    expect(items[2]).toEqual({
      kind: "file",
      key: "f:feat/a:h1:src/x.ts",
      branch: "feat/a",
      hash: "h1",
      path: "src/x.ts",
    });
  });

  it("shows no files when the expanded commit has no loaded files", () => {
    const items = branchTreeItems(
      rows,
      state({
        expandedBranch: "feat/a",
        expandedCommit: "h1",
        branchCommits: { "feat/a": commits },
      }),
    );
    expect(items.some((i) => i.kind === "file")).toBe(false);
  });

  it("shows files only under the expanded branch's matching commit", () => {
    const items = branchTreeItems(
      rows,
      state({
        expandedBranch: "feat/b",
        expandedCommit: "h1",
        branchCommits: { "feat/a": commits, "feat/b": [{ hash: "h9", message: "other" }] },
        commitFiles: { h1: ["src/x.ts"] },
      }),
    );
    expect(items.map((i) => i.key)).toEqual(["b:feat/a", "b:feat/b", "c:feat/b:h9"]);
  });
});

describe("cursorIndexFor", () => {
  const items = branchTreeItems(
    [row({ name: "a" }), row({ name: "b" }), row({ name: "c" })],
    state(),
  );

  it("returns the index of the key when present", () => {
    expect(cursorIndexFor(items, "b:c", 0)).toBe(2);
  });

  it("falls back to the clamped fallback when the key is missing", () => {
    expect(cursorIndexFor(items, "b:gone", 1)).toBe(1);
  });

  it("clamps a fallback past the end to the last index", () => {
    expect(cursorIndexFor(items, "b:gone", 9)).toBe(2);
  });

  it("clamps a negative fallback to 0", () => {
    expect(cursorIndexFor(items, "b:gone", -4)).toBe(0);
  });

  it("returns 0 for an empty list", () => {
    expect(cursorIndexFor([], "b:a", 3)).toBe(0);
  });

  it("uses the clamped fallback for a null key", () => {
    expect(cursorIndexFor(items, null, 7)).toBe(2);
  });
});

describe("branchTreeKeyAction", () => {
  const allowed = row({ name: "feat/a" });
  const branch = (r: LocalBranchRow): BranchTreeItem => ({ kind: "branch", key: branchKey(r.name), row: r });
  const commit: BranchTreeItem = { kind: "commit", key: "c:feat/a:h1", branch: "feat/a", hash: "h1", message: "m" };
  const file: BranchTreeItem = { kind: "file", key: "f:feat/a:h1:p.ts", branch: "feat/a", hash: "h1", path: "p.ts" };

  it.each(["Space", "Enter"])("%s on a branch toggles the branch", (code) => {
    expect(branchTreeKeyAction(branch(allowed), code)).toEqual({ type: "toggle-branch", branch: "feat/a" });
  });

  it.each(["Space", "Enter"])("%s on a commit toggles the commit", (code) => {
    expect(branchTreeKeyAction(commit, code)).toEqual({ type: "toggle-commit", hash: "h1" });
  });

  it("Enter on a file opens it with hash and path", () => {
    expect(branchTreeKeyAction(file, "Enter")).toEqual({ type: "open-file", hash: "h1", path: "p.ts" });
  });

  it("Space on a file does nothing", () => {
    expect(branchTreeKeyAction(file, "Space")).toBeNull();
  });

  it("KeyO on an allowed branch opens a session", () => {
    expect(branchTreeKeyAction(branch(allowed), "KeyO")).toEqual({ type: "open-session", row: allowed });
  });

  it("KeyD on an allowed branch deletes it", () => {
    expect(branchTreeKeyAction(branch(allowed), "KeyD")).toEqual({ type: "delete", row: allowed });
  });

  it.each(["KeyO", "KeyD"])("%s on a branch with a session does nothing", (code) => {
    const r = row({ session: { id: "s1", name: "sess" } });
    expect(branchTreeKeyAction(branch(r), code)).toBeNull();
  });

  it.each(["KeyO", "KeyD"])("%s on an in-use branch does nothing", (code) => {
    expect(branchTreeKeyAction(branch(row({ in_use: true })), code)).toBeNull();
  });

  it.each(["KeyO", "KeyD"])("%s on a commit row does nothing", (code) => {
    expect(branchTreeKeyAction(commit, code)).toBeNull();
  });

  it.each(["KeyO", "KeyD"])("%s on a file row does nothing", (code) => {
    expect(branchTreeKeyAction(file, code)).toBeNull();
  });

  it("returns null when there is no item", () => {
    expect(branchTreeKeyAction(undefined, "Enter")).toBeNull();
  });

  it("returns null for unrelated codes", () => {
    expect(branchTreeKeyAction(branch(allowed), "KeyX")).toBeNull();
  });
});

describe("changedBranches", () => {
  it("returns names removed from next", () => {
    expect(changedBranches([row({ name: "a" }), row({ name: "b" })], [row({ name: "a" })])).toEqual(["b"]);
  });

  it("returns names whose sha differs", () => {
    const prev = [row({ name: "a", sha: "1" })];
    const next = [row({ name: "a", sha: "2" })];
    expect(changedBranches(prev, next)).toEqual(["a"]);
  });

  it("ignores unchanged branches", () => {
    const rows = [row({ name: "a", sha: "1" })];
    expect(changedBranches(rows, [row({ name: "a", sha: "1" })])).toEqual([]);
  });

  it("ignores newly added branches", () => {
    expect(changedBranches([row({ name: "a" })], [row({ name: "a" }), row({ name: "new" })])).toEqual([]);
  });
});

describe("openSessionConfirm", () => {
  it("escapes the branch name in the body", () => {
    const { body } = openSessionConfirm(row({ name: "nergal/<b>x" }));
    expect(body).toContain("nergal/&lt;b&gt;x");
    expect(body).not.toContain("<b>");
  });

  it("mentions the new Claude Code session", () => {
    expect(openSessionConfirm(row()).body).toContain("new Claude Code session");
  });

  it("uses the open-session labels and a non-destructive question kind", () => {
    const opts = openSessionConfirm(row());
    expect(opts.title).toBe("Open session?");
    expect(opts.confirmLabel).toBe("Open session");
    expect(opts.cancelLabel).toBe("Cancel");
    expect(opts.kind).toBe("question");
    expect(opts.destructive).toBe(false);
  });
});
