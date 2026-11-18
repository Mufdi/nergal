export interface RowNavData {
  groupKey?: string;
  file?: string;
  detailId?: string;
}

export type RowKeyAction =
  | { kind: "toggle-group"; key: string }
  | { kind: "open-file"; path: string }
  | { kind: "toggle-detail"; id: string }
  | { kind: "none" };

// Enter on a row with both a file and a detail keeps opening the file (its
// pre-existing meaning); Space has no file meaning, so it reaches the detail.
export function rowKeyAction(key: "Enter" | "Space", row: RowNavData): RowKeyAction {
  if (row.groupKey) return { kind: "toggle-group", key: row.groupKey };
  if (key === "Enter" && row.file) return { kind: "open-file", path: row.file };
  if (row.detailId) return { kind: "toggle-detail", id: row.detailId };
  return { kind: "none" };
}
