import { useAtomValue } from "jotai";
import { planDiffAtom } from "@/stores/plan";

const LINE_STYLES = {
  addition: "bg-success/15 text-success",
  deletion: "bg-danger/15 text-danger",
  context: "text-text-muted",
} as const;

const LINE_PREFIX = {
  addition: "+",
  deletion: "-",
  context: " ",
} as const;

export function DiffView() {
  const diff = useAtomValue(planDiffAtom);

  if (diff.length === 0) {
    return (
      <p className="px-3 py-4 text-center text-xs text-text-muted">No changes to display</p>
    );
  }

  return (
    <div className="overflow-auto font-mono text-xs" role="region" aria-label="Plan diff">
      {diff.map((line, i) => (
        <div key={i} className={`px-3 py-px ${LINE_STYLES[line.type]}`}>
          <span className="mr-2 inline-block w-3 select-none text-right opacity-50">
            {LINE_PREFIX[line.type]}
          </span>
          {line.content}
        </div>
      ))}
    </div>
  );
}
