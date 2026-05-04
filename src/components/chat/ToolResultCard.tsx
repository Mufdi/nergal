import type { OpenCodeMessagePart } from "@/stores/opencode";

interface Props {
  part: OpenCodeMessagePart;
}

export function ToolResultCard({ part }: Props) {
  const result = part.result;
  const text =
    typeof result === "string"
      ? result
      : result !== undefined
        ? JSON.stringify(result, null, 2)
        : "";

  return (
    <div className="mt-1 rounded border border-zinc-800 bg-zinc-950/50 px-2 py-1.5 font-mono text-xs">
      <div className="text-zinc-500">↳ result</div>
      {text ? (
        <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-all text-zinc-400">
          {text}
        </pre>
      ) : null}
    </div>
  );
}
