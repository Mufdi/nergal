import { useMemo, useState, useCallback } from "react";
import { useAtomValue } from "jotai";
import { activeActivityAtom } from "@/stores/activity";
import {
  ReactFlow,
  Background,
  type Node,
  type Edge,
  type NodeMouseHandler,
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
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  const toggleExpanded = useCallback((id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const onNodeClick: NodeMouseHandler = useCallback((_event, node) => {
    const entry = entries.find((e) => e.id === node.id);
    if (entry?.detail) {
      toggleExpanded(node.id);
    }
  }, [entries, toggleExpanded]);

  const { nodes, edges } = useMemo(() => {
    const ns: Node[] = [];
    const es: Edge[] = [];

    let yOffset = 0;

    for (let i = 0; i < entries.length; i++) {
      const entry = entries[i];
      const color = TYPE_COLORS[entry.type] ?? "#5c5c5f";
      const isExpanded = expandedIds.has(entry.id);
      const hasDetail = !!entry.detail;
      const nodeWidth = isExpanded ? 360 : 240;
      const nodeHeight = isExpanded ? "auto" : undefined;

      ns.push({
        id: entry.id,
        position: { x: 50, y: yOffset },
        data: {
          label: (
            <div style={{ maxWidth: isExpanded ? 340 : 220 }}>
              <div className="flex items-center gap-1.5">
                <span
                  className="inline-block size-2 rounded-full"
                  style={{ backgroundColor: color }}
                />
                <span className="text-[10px] font-medium text-foreground truncate">
                  {entry.message}
                </span>
              </div>
              {entry.detail && !isExpanded && (
                <p className="mt-0.5 truncate text-[9px] text-muted-foreground">{entry.detail}</p>
              )}
              {isExpanded && entry.detail && (
                <div className="mt-1 rounded bg-background/60 px-1.5 py-1">
                  <p className="whitespace-pre-wrap text-[9px] leading-relaxed text-muted-foreground">
                    {entry.detail}
                  </p>
                </div>
              )}
              {hasDetail && (
                <p className="mt-0.5 text-[8px] text-muted-foreground/60">
                  {isExpanded ? "click to collapse" : "click to expand"}
                </p>
              )}
            </div>
          ),
        },
        style: {
          background: "var(--card)",
          border: `1px solid ${color}40`,
          borderRadius: "6px",
          padding: "6px 10px",
          width: nodeWidth,
          height: nodeHeight,
          cursor: hasDetail ? "pointer" : "default",
        },
      });

      yOffset += isExpanded ? 140 : 80;

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
  }, [entries, expandedIds]);

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
        onNodeClick={onNodeClick}
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
