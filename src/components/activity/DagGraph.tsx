import { useMemo } from "react";
import { useAtomValue } from "jotai";
import { activeActivityAtom } from "@/stores/activity";
import {
  ReactFlow,
  Background,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { ActivityEntry } from "@/lib/types";

const TYPE_COLORS: Record<ActivityEntry["type"], string> = {
  tool_use: "#3b82f6",
  session: "#f97316",
  task: "#22c55e",
  plan: "#f97316",
  error: "#ef4444",
  file_modified: "#eab308",
};

export function DagGraph() {
  const entries = useAtomValue(activeActivityAtom);

  const { nodes, edges } = useMemo(() => {
    const ns: Node[] = [];
    const es: Edge[] = [];

    for (let i = 0; i < entries.length; i++) {
      const entry = entries[i];
      const color = TYPE_COLORS[entry.type] ?? "#5c5c5f";

      ns.push({
        id: entry.id,
        position: { x: 50, y: i * 80 },
        data: {
          label: (
            <div className="max-w-52">
              <div className="flex items-center gap-1.5">
                <span
                  className="inline-block size-2 rounded-full"
                  style={{ backgroundColor: color }}
                />
                <span className="text-[10px] font-medium text-foreground truncate">
                  {entry.message}
                </span>
              </div>
              {entry.detail && (
                <p className="mt-0.5 truncate text-[9px] text-muted-foreground">{entry.detail}</p>
              )}
            </div>
          ),
        },
        style: {
          background: "#1c1c1e",
          border: `1px solid ${color}40`,
          borderRadius: "6px",
          padding: "6px 10px",
          width: 240,
        },
      });

      if (i > 0) {
        es.push({
          id: `e-${entries[i - 1].id}-${entry.id}`,
          source: entries[i - 1].id,
          target: entry.id,
          style: { stroke: "#5c5c5f40" },
        });
      }
    }

    return { nodes: ns, edges: es };
  }, [entries]);

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">No activity to visualize</p>
      </div>
    );
  }

  return (
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        fitView
        panOnDrag
        zoomOnScroll
        nodesDraggable={false}
        nodesConnectable={false}
        proOptions={{ hideAttribution: true }}
      >
        <Background color="#5c5c5f20" gap={20} />
      </ReactFlow>
    </div>
  );
}
