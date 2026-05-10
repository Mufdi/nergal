import { useState, useEffect, useRef } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { AvailableAgent } from "@/lib/types";
import claudeLogo from "@/assets/agents/claude.svg";
import codexLogo from "@/assets/agents/codex.svg";
import opencodeLogo from "@/assets/agents/opencode.svg";
import piLogo from "@/assets/agents/pi.svg";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  agents: AvailableAgent[];
  sessionName: string;
  preselectedId?: string | null;
  onPick: (agentId: string) => void;
}

const LOGO_BY_ID: Record<string, string> = {
  "claude-code": claudeLogo,
  codex: codexLogo,
  opencode: opencodeLogo,
  pi: piLogo,
};

export function AgentPickerModal({
  open,
  onOpenChange,
  agents,
  sessionName,
  preselectedId,
  onPick,
}: Props) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    if (!open) return;
    const idx = preselectedId ? agents.findIndex((a) => a.id === preselectedId) : 0;
    setSelectedIdx(idx >= 0 ? idx : 0);
    requestAnimationFrame(() => listRef.current?.focus());
  }, [open, preselectedId, agents]);

  // Scroll the active card into view on selection change so the rightmost
  // entries don't get clipped when the row overflows.
  useEffect(() => {
    if (!open) return;
    const el = itemRefs.current[selectedIdx];
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedIdx, open]);

  function commit(idx: number) {
    const agent = agents[idx];
    if (!agent || !agent.installed) return;
    onPick(agent.id);
    onOpenChange(false);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (agents.length === 0) return;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIdx((i) => (i + 1) % agents.length);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIdx((i) => (i - 1 + agents.length) % agents.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      commit(selectedIdx);
    } else if (/^[1-9]$/.test(e.key)) {
      const i = parseInt(e.key, 10) - 1;
      if (i < agents.length) {
        e.preventDefault();
        commit(i);
      }
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false} className="!max-w-fit">
        <DialogHeader>
          <DialogTitle className="text-[13px]">
            Choose agent <span className="text-muted-foreground/70">— "{sessionName}"</span>
          </DialogTitle>
        </DialogHeader>
        <div
          ref={listRef}
          tabIndex={0}
          onKeyDown={handleKeyDown}
          className="flex flex-row items-start gap-2 overflow-x-auto outline-none rounded p-0.5"
        >
          {agents.map((a, i) => {
            const disabled = !a.installed;
            const selected = i === selectedIdx;
            const logo = LOGO_BY_ID[a.id];
            return (
              <button
                key={a.id}
                ref={(el) => {
                  itemRefs.current[i] = el;
                }}
                onClick={() => commit(i)}
                disabled={disabled}
                className={`relative flex w-32 shrink-0 flex-col items-center gap-2 rounded-lg border px-3 py-3 text-center transition-colors ${
                  disabled
                    ? "border-border/40 bg-card/40 text-muted-foreground/50 cursor-not-allowed"
                    : selected
                      ? "border-orange-500 bg-orange-500/10 text-foreground"
                      : "border-border bg-card text-foreground/80 hover:bg-secondary hover:text-foreground"
                }`}
              >
                {logo ? (
                  <img
                    src={logo}
                    alt=""
                    aria-hidden
                    className={`size-10 ${disabled ? "opacity-40" : ""}`}
                  />
                ) : (
                  <div className="size-10 rounded bg-muted/40" />
                )}
                <div className="flex flex-col gap-0.5">
                  <span className="text-[12px] font-medium leading-none">{a.display_name}</span>
                  {a.version ? (
                    <span className="text-[9px] tabular-nums text-muted-foreground/70 leading-none">
                      {a.version}
                    </span>
                  ) : !a.installed ? (
                    <span className="text-[9px] text-muted-foreground/70 leading-none">not installed</span>
                  ) : null}
                </div>
                {i < 9 && (
                  <span className="absolute -top-1.5 -left-1.5 flex size-4 items-center justify-center rounded-full bg-muted/80 text-[9px] font-medium tabular-nums text-muted-foreground">
                    {i + 1}
                  </span>
                )}
              </button>
            );
          })}
          {agents.length === 0 && (
            <p className="text-[11px] text-muted-foreground py-4 text-center w-full">No agents detected.</p>
          )}
        </div>
        <p className="text-[10px] text-muted-foreground/60">← → navigate • 1–9 jump • Enter to pick • Esc to cancel</p>
      </DialogContent>
    </Dialog>
  );
}
