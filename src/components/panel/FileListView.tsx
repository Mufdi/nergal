import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionFilesAtom } from "@/stores/files";
import { openTabAction } from "@/stores/rightPanel";
import { FileCode } from "lucide-react";

export function FileListView() {
  const files = useAtomValue(activeSessionFilesAtom);
  const openTab = useSetAtom(openTabAction);

  function handleClick(path: string, filename: string) {
    openTab({
      tab: { id: `file-${path}`, type: "file", label: filename, data: { path } },
      isPinned: false,
    });
  }

  function handleDoubleClick(path: string, filename: string) {
    openTab({
      tab: { id: `file-${path}`, type: "file", label: filename, data: { path } },
      isPinned: true,
    });
  }

  if (files.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No modified files</span>
      </div>
    );
  }

  return (
    <div className="flex flex-col py-1">
      <div className="px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Modified Files
        </span>
      </div>
      {files.map((file) => {
        const filename = file.path.split("/").pop() ?? file.path;
        const dir = file.path.split("/").slice(0, -1).join("/");
        const timeStr = new Date(file.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
        return (
          <button
            key={file.path}
            onClick={() => handleClick(file.path, filename)}
            onDoubleClick={() => handleDoubleClick(file.path, filename)}
            className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-secondary/50"
            aria-label={filename}
          >
            <FileCode size={13} className="shrink-0 text-muted-foreground" />
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[11px] text-foreground">{filename}</span>
              <span className="truncate text-[10px] text-muted-foreground">{dir}</span>
            </div>
            <div className="flex shrink-0 flex-col items-end">
              <span className="text-[10px] text-muted-foreground">{file.tool}</span>
              <span className="text-[10px] text-muted-foreground/60">{timeStr}</span>
            </div>
          </button>
        );
      })}
    </div>
  );
}
