import type { OpenCodeMessagePart } from "@/stores/opencode";

interface Props {
  part: OpenCodeMessagePart;
}

export function ToolUseCard({ part }: Props) {
  const name = part.tool?.name ?? "tool";
  const input = part.tool?.input;
  const inputJson = input !== undefined ? JSON.stringify(input, null, 2) : null;

  return (
    <div className="mt-2 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 font-mono text-xs">
      <div className="flex items-center gap-2 text-zinc-400">
        <span className="text-amber-400">▶</span>
        <span>{name}</span>
        {part.state ? <span className="text-zinc-600">({part.state})</span> : null}
      </div>
      {inputJson ? (
        <pre className="mt-1 max-h-48 overflow-auto whitespace-pre-wrap break-all text-zinc-500">
          {inputJson}
        </pre>
      ) : null}
    </div>
  );
}
