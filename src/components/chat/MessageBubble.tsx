import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { OpenCodeMessage } from "@/stores/opencode";
import { ToolUseCard } from "./ToolUseCard";
import { ToolResultCard } from "./ToolResultCard";

interface Props {
  message: OpenCodeMessage;
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === "user";
  const parts = message.parts ?? [];
  const textParts = parts.filter((p) => p.type === "text" && p.text);

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"} mb-3`}>
      <div
        className={`max-w-[85%] rounded-lg px-3 py-2 text-sm ${
          isUser
            ? "bg-zinc-800 text-zinc-100"
            : "bg-zinc-900 border border-zinc-800 text-zinc-200"
        }`}
      >
        {textParts.length === 0 && parts.length === 0 ? (
          <span className="italic text-zinc-500">…</span>
        ) : null}

        {textParts.map((p, i) => (
          <div key={p.id ?? i} className="prose prose-invert prose-sm max-w-none">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{p.text ?? ""}</ReactMarkdown>
          </div>
        ))}

        {parts
          .filter((p) => p.type === "tool" && p.tool)
          .map((p, i) => (
            <ToolUseCard key={p.id ?? `tool-${i}`} part={p} />
          ))}

        {parts
          .filter((p) => p.type === "tool_result" || p.type === "tool-result")
          .map((p, i) => (
            <ToolResultCard key={p.id ?? `result-${i}`} part={p} />
          ))}

        {message.cost !== undefined && message.cost > 0 ? (
          <div className="mt-1 text-[10px] text-zinc-500">
            ${message.cost.toFixed(4)} · {message.tokens?.total ?? 0} tok
          </div>
        ) : null}
      </div>
    </div>
  );
}
