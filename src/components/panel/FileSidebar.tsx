import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionFilesAtom } from "@/stores/files";
import { openTabAction } from "@/stores/rightPanel";
import { FileCode } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

export function FileSidebar() {
  const files = useAtomValue(activeSessionFilesAtom);
  const openTab = useSetAtom(openTabAction);

  function handleClick(path: string, filename: string) {
    openTab({
      tab: { id: `file-${path}`, type: "file", label: filename, data: { path } },
    });
  }

  if (files.length === 0) {
    return (
      <div className="flex w-40 shrink-0 flex-col items-center justify-center border-l border-border/50">
        <span className="text-[10px] text-muted-foreground">No modified files</span>
      </div>
    );
  }

  return (
    <div className="flex w-40 shrink-0 flex-col border-l border-border/50 overflow-y-auto py-1">
      {files.map((file) => {
        const filename = file.path.split("/").pop() ?? file.path;
        return (
          <Tooltip key={file.path}>
            <TooltipTrigger
              render={
                <button
                  onClick={() => handleClick(file.path, filename)}
                  className="flex w-full items-center gap-1.5 px-2 py-1 text-left text-[11px] text-muted-foreground hover:bg-secondary/50 hover:text-foreground transition-colors"
                  aria-label={filename}
                />
              }
            >
              <FileCode size={12} className="shrink-0" />
              <span className="truncate">{filename}</span>
            </TooltipTrigger>
            <TooltipContent side="left" className="max-w-64 truncate">
              {file.path}
            </TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}
