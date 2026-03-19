import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionFilesAtom } from "@/stores/files";
import { openTabAction } from "@/stores/rightPanel";
import { activeSessionIdAtom } from "@/stores/workspace";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

function fileIcon(tool: string) {
  switch (tool) {
    case "Write":
    case "Create":
      return "+";
    case "Edit":
    case "MultiEdit":
      return "~";
    default:
      return "*";
  }
}

function iconColor(tool: string) {
  switch (tool) {
    case "Write":
    case "Create":
      return "text-green-500";
    case "Edit":
    case "MultiEdit":
      return "text-orange-500";
    default:
      return "text-muted-foreground";
  }
}

export function ModifiedFiles() {
  const files = useAtomValue(activeSessionFilesAtom);
  const openTab = useSetAtom(openTabAction);
  const sessionId = useAtomValue(activeSessionIdAtom);

  function handleFileClick(path: string) {
    const filename = path.split("/").pop() ?? path;
    openTab({
      tab: { id: `diff-${path}`, type: "diff", label: filename, data: { path, sessionId } },
      isPinned: false,
    });
  }

  if (files.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-xs text-muted-foreground">No files modified yet</span>
      </div>
    );
  }

  return (
    <ScrollArea className="h-full">
      <div className="p-2 space-y-0.5">
        {files.map((file) => {
          const filename = file.path.split("/").pop() ?? file.path;
          const dir = file.path.split("/").slice(0, -1).join("/");

          return (
            <Tooltip key={file.path}>
              <TooltipTrigger
                className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-xs hover:bg-muted/50 cursor-pointer"
                onClick={() => handleFileClick(file.path)}
              >
                <span className={`font-mono font-bold ${iconColor(file.tool)}`}>
                  {fileIcon(file.tool)}
                </span>
                <span className="truncate text-foreground">{filename}</span>
                <span className="ml-auto shrink-0 text-muted-foreground opacity-60 text-[10px]">
                  {file.tool}
                </span>
              </TooltipTrigger>
              <TooltipContent side="right" className="max-w-xs">
                <p className="font-mono text-xs">{dir}/</p>
                <p className="font-mono text-xs font-bold">{filename}</p>
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </ScrollArea>
  );
}
