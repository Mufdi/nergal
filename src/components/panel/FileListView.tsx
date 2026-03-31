import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionFilesAtom } from "@/stores/files";
import { openTabAction } from "@/stores/rightPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import { FileCode } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

export function FileListView() {
  const files = useAtomValue(activeSessionFilesAtom);
  const openTab = useSetAtom(openTabAction);
  const sessionId = useAtomValue(activeSessionIdAtom);

  function handleClick(path: string, filename: string) {
    openTab({
      tab: { id: `diff-${path}`, type: "diff", label: filename, data: { path, sessionId } },
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
        const timeStr = new Date(file.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
        return (
          <Tooltip key={file.path}>
            <TooltipTrigger
              render={
                <button
                  data-nav-item
                  onClick={() => handleClick(file.path, filename)}
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-secondary/50"
                  aria-label={filename}
                />
              }
            >
              <FileCode size={13} className="shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate text-[11px] text-foreground">{filename}</span>
              <div className="flex shrink-0 flex-col items-end">
                <span className="text-[10px] text-muted-foreground">{file.tool}</span>
                <span className="text-[10px] text-muted-foreground/60">{timeStr}</span>
              </div>
            </TooltipTrigger>
            <TooltipContent side="left" className="max-w-xs">
              <p className="font-mono text-xs break-all">{file.path}</p>
            </TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}
