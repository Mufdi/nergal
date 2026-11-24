export type PrViewerKeyAction =
  | "picker-close"
  | "picker-next"
  | "picker-prev"
  | "picker-pick"
  | "hunk-next"
  | "hunk-prev"
  | "toggle-collapse"
  | "annotate"
  | "merge"
  | "file-prev"
  | "file-next";

export function prViewerKeyAction(
  e: {
    code: string;
    ctrlKey: boolean;
    metaKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    repeat: boolean;
  },
  ctx: { pickerOpen: boolean; hunkCount: number; fileCount: number; canMerge: boolean },
): PrViewerKeyAction | null {
  const mods = e.ctrlKey || e.metaKey || e.altKey;
  if (ctx.pickerOpen) {
    if (e.code === "Escape") return "picker-close";
    if (mods) return null;
    if (e.code === "Enter") return "picker-pick";
    if (ctx.fileCount === 0) return null;
    if (e.code === "KeyJ" || e.code === "ArrowDown") return "picker-next";
    if (e.code === "KeyK" || e.code === "ArrowUp") return "picker-prev";
    return null;
  }
  if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && ctx.fileCount >= 2) {
    if (e.code === "ArrowLeft") return "file-prev";
    if (e.code === "ArrowRight") return "file-next";
  }
  if (mods) return null;
  if (e.code === "KeyM") return ctx.canMerge && !e.repeat ? "merge" : null;
  if (ctx.hunkCount === 0) return null;
  if (e.code === "KeyA") return "annotate";
  if (e.code === "KeyJ" || e.code === "ArrowDown") return "hunk-next";
  if (e.code === "KeyK" || e.code === "ArrowUp") return "hunk-prev";
  if (e.code === "Space" && !e.shiftKey) return "toggle-collapse";
  return null;
}

export function mergeAfterConfirm(p: {
  confirmed: boolean;
  captured: { workspaceId: string; prNumber: number };
  current: { workspaceId: string; prNumber: number; state: string; merging: boolean };
}): boolean {
  const { confirmed, captured, current } = p;
  return (
    confirmed &&
    current.workspaceId === captured.workspaceId &&
    current.prNumber === captured.prNumber &&
    current.state === "OPEN" &&
    !current.merging
  );
}
