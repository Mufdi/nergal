import { describe, expect, it } from "vitest";
import {
  branchActionsAllowed,
  branchDeleteToast,
  deleteBranchConfirm,
  orphanBranchCount,
  type LocalBranchRow,
} from "./localBranches";

function row(overrides: Partial<LocalBranchRow> = {}): LocalBranchRow {
  return {
    name: "nergal/a-1",
    sha: "abc1234",
    pending: 3,
    last_commit_unix: 1700000000,
    last_subject: "subject",
    session: null,
    in_use: false,
    missing_worktree: null,
    ...overrides,
  };
}

describe("branchActionsAllowed", () => {
  it.each([
    { label: "no session and not in use", overrides: {}, allowed: true },
    {
      label: "has a session",
      overrides: { session: { id: "s1", name: "Session" } },
      allowed: false,
    },
    { label: "in use", overrides: { in_use: true }, allowed: false },
  ])("$label -> $allowed", ({ overrides, allowed }) => {
    expect(branchActionsAllowed(row(overrides))).toBe(allowed);
  });
});

describe("orphanBranchCount", () => {
  it("counts only the rows with no session", () => {
    const rows = [
      row({ name: "nergal/a" }),
      row({ name: "nergal/b", session: { id: "s1", name: "S" } }),
      row({ name: "nergal/c" }),
    ];
    expect(orphanBranchCount(rows)).toBe(2);
  });
});

describe("deleteBranchConfirm", () => {
  it("escapes the branch name and states the count and local-only warning", () => {
    const opts = deleteBranchConfirm(row({ name: "nergal/<b>x", pending: 7 }));
    expect(opts.title).toBe("Delete branch?");
    expect(opts.body).toContain("nergal/&lt;b&gt;x");
    expect(opts.body).not.toContain("<b>");
    expect(opts.body).toContain("7");
    expect(opts.body).toContain("exist only on this machine");
    expect(opts.confirmLabel).toBe("Delete");
    expect(opts.destructive).toBe(true);
  });

  it("shows ? when the pending count is unknown", () => {
    const opts = deleteBranchConfirm(row({ pending: null }));
    expect(opts.body).toContain("?");
  });
});

describe("branchDeleteToast", () => {
  it("a deleted branch gives the restore command with the repo quoted", () => {
    const toast = branchDeleteToast("nergal/a-1", {
      kind: "deleted",
      sha: "abc1234",
      repo: "/home/u/my repo",
    });
    expect(toast.message).toBe("Branch deleted");
    expect(toast.description).toContain(
      "git -C '/home/u/my repo' branch nergal/a-1 abc1234",
    );
    expect(toast.type).toBe("info");
  });

  it("a missing branch reports it was already deleted", () => {
    const toast = branchDeleteToast("nergal/a-1", { kind: "missing" });
    expect(toast.message).toBe("Branch already deleted");
    expect(toast.type).toBe("info");
  });

  it("a kept branch surfaces the reason as an error", () => {
    const toast = branchDeleteToast("nergal/a-1", { kind: "kept", message: "branch moved" });
    expect(toast.message).toBe("Branch kept");
    expect(toast.description).toBe("branch moved");
    expect(toast.type).toBe("error");
  });
});
