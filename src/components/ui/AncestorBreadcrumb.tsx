import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

/// Breadcrumb of ancestor task/issue names shown before a subtask's title in
/// the detail header (tab + modal). `ancestors` is ordered root → immediate
/// parent. The two closest ancestors render inline; any older ones collapse
/// into a "…" carrying an immediate tooltip with the full chain, so deep
/// nesting stays legible without eating the title's width.
const VISIBLE = 2;

export function AncestorBreadcrumb({ ancestors }: { ancestors: string[] }) {
  if (ancestors.length === 0) return null;
  const collapsed =
    ancestors.length > VISIBLE ? ancestors.slice(0, ancestors.length - VISIBLE) : [];
  const shown = ancestors.slice(-VISIBLE);

  return (
    <span className="flex min-w-0 shrink items-center gap-1 text-[11px] text-muted-foreground/70">
      {collapsed.length > 0 && (
        <>
          <TooltipProvider delay={0}>
            <Tooltip>
              <TooltipTrigger
                render={
                  <span
                    tabIndex={-1}
                    className="cursor-default rounded px-0.5 leading-none text-muted-foreground/60 outline-none hover:text-foreground"
                  />
                }
              >
                …
              </TooltipTrigger>
              <TooltipContent side="bottom" className="text-[10px]">
                {ancestors.join(" / ")}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
          <span className="text-muted-foreground/40">/</span>
        </>
      )}
      {shown.map((name, i) => (
        <span key={i} className="flex min-w-0 items-center gap-1">
          <span className="max-w-[8rem] truncate" title={name}>
            {name}
          </span>
          <span className="text-muted-foreground/40">/</span>
        </span>
      ))}
    </span>
  );
}
