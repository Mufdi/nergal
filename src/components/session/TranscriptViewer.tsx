import { useState, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { invoke } from "@/lib/tauri";

interface TranscriptEntry {
  role: "human" | "assistant";
  content: string;
  timestamp?: number;
}

interface TranscriptViewerProps {
  sessionId: string;
}

export function TranscriptViewer({ sessionId }: TranscriptViewerProps) {
  const [entries, setEntries] = useState<TranscriptEntry[] | null>(null);

  useEffect(() => {
    setEntries(null);
    invoke<TranscriptEntry[]>("get_transcript", { sessionId })
      .then(setEntries)
      .catch(() => setEntries([]));
  }, [sessionId]);

  if (entries === null) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">Loading...</span>
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No transcript found</span>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto p-3">
      {entries.map((entry, i) => (
        <div
          key={i}
          className={`flex ${entry.role === "human" ? "justify-end" : "justify-start"}`}
        >
          <div
            className={`max-w-[85%] rounded-lg px-3 py-2 text-[11px] leading-relaxed ${
              entry.role === "human"
                ? "bg-orange-500/10 text-foreground"
                : "bg-card text-foreground/90"
            }`}
          >
            {entry.role === "assistant" ? (
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
                  code: ({ children }) => (
                    <code className="rounded bg-background/50 px-1 py-0.5 text-[10px] font-mono">
                      {children}
                    </code>
                  ),
                  pre: ({ children }) => (
                    <pre className="mb-2 overflow-x-auto rounded bg-background/50 p-2 text-[10px] last:mb-0">
                      {children}
                    </pre>
                  ),
                }}
              >
                {entry.content}
              </ReactMarkdown>
            ) : (
              <span className="whitespace-pre-wrap">{entry.content}</span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
