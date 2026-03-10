import { useEffect, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import { useTerminal } from "./useTerminal";

interface TerminalPanelProps {
  cwd?: string;
}

export function TerminalPanel({ cwd }: TerminalPanelProps) {
  const { terminalRef, fit } = useTerminal({ cwd });
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const observer = new ResizeObserver(() => {
      requestAnimationFrame(fit);
    });
    observer.observe(container);

    return () => observer.disconnect();
  }, [fit]);

  return (
    <div ref={containerRef} className="flex h-full w-full flex-col overflow-hidden bg-[#1a1a2e]">
      <div ref={terminalRef} className="h-full w-full" />
    </div>
  );
}
