import { useState, useEffect, useRef } from "react";
import { Check } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { AvailableAgent } from "@/lib/types";
import type { LaunchOptions, PermissionPreset } from "@/stores/workspace";
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
  onPick: (agentId: string, launchOptions: LaunchOptions | null) => void;
}

const LOGO_BY_ID: Record<string, string> = {
  "claude-code": claudeLogo,
  codex: codexLogo,
  opencode: opencodeLogo,
  pi: piLogo,
};

const PRESET_LABELS: Record<string, string> = {
  plan: "Plan mode",
  "accept-edits": "Auto-accept edits",
  auto: "Auto mode",
  bypass: "Skip permissions",
};

/// Preset rows behave as a radio: one permission mode per session. CC's
/// `--dangerously-skip-permissions` is the documented equivalent of
/// `--permission-mode bypassPermissions`, so "Skip permissions" is a mode
/// like the rest — checking it unchecks plan/accept (they don't combine;
/// bypass silently wins if both flags are passed). The one composable
/// option is "allow" (CC `--allow-dangerously-skip-permissions`): adds
/// bypass to the Shift+Tab cycle without starting in it.
type OptRow = { kind: "preset"; value: PermissionPreset } | { kind: "allow" };

export function AgentPickerModal({
  open,
  onOpenChange,
  agents,
  sessionName,
  preselectedId,
  onPick,
}: Props) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  // -1 = agent row focused; 0..rows-1 = option rows; rows.length = the
  // startup-command row (its input takes real DOM focus).
  const [optIdx, setOptIdx] = useState(-1);
  const [preset, setPreset] = useState<PermissionPreset>("default");
  const [allowSkip, setAllowSkip] = useState(false);
  const [startupCmd, setStartupCmd] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const startupInputRef = useRef<HTMLInputElement>(null);

  const agent = agents[selectedIdx];
  // "default" is the unchecked state, not a row — checking nothing = default.
  const rows: OptRow[] = [
    ...((agent?.permission_presets ?? [])
      .filter((p) => p !== "default")
      .map((p) => ({ kind: "preset", value: p as PermissionPreset }) as OptRow)),
    ...(agent?.allow_skip_cycle_supported ? [{ kind: "allow" } as OptRow] : []),
  ];
  const startupRowIdx = rows.length;

  useEffect(() => {
    if (!open) return;
    const idx = preselectedId ? agents.findIndex((a) => a.id === preselectedId) : 0;
    setSelectedIdx(idx >= 0 ? idx : 0);
    setOptIdx(-1);
    setPreset("default");
    setAllowSkip(false);
    setStartupCmd("");
    requestAnimationFrame(() => listRef.current?.focus());
  }, [open, preselectedId, agents]);

  // Scroll the active card into view on selection change so the rightmost
  // entries don't get clipped when the row overflows.
  useEffect(() => {
    if (!open) return;
    const el = itemRefs.current[selectedIdx];
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedIdx, open]);

  // The startup row owns real DOM focus (it's a text input); every other row
  // keeps focus on the container so its keydown handler drives navigation.
  useEffect(() => {
    if (!open) return;
    if (optIdx === startupRowIdx) startupInputRef.current?.focus();
    else listRef.current?.focus();
  }, [optIdx, startupRowIdx, open]);

  function switchAgent(idx: number) {
    setSelectedIdx(idx);
    // Options the new agent doesn't support must not survive the switch.
    const next = agents[idx];
    const supported = next?.permission_presets ?? [];
    setPreset((p) => (supported.includes(p) ? p : "default"));
    if (!next?.allow_skip_cycle_supported) setAllowSkip(false);
  }

  function toggleRow(row: number) {
    const r = rows[row];
    if (!r) return;
    if (r.kind === "allow") {
      // Redundant when already starting in bypass — keep it un-toggleable.
      if (preset !== "bypass") setAllowSkip((v) => !v);
      return;
    }
    setPreset((prev) => {
      const next = prev === r.value ? "default" : r.value;
      if (next === "bypass") setAllowSkip(false);
      return next;
    });
  }

  function commit(idx: number) {
    const a = agents[idx];
    if (!a || !a.installed) return;
    const cmd = startupCmd.trim();
    const launchOptions: LaunchOptions | null =
      preset === "default" && !allowSkip && !cmd
        ? null
        : { permission_preset: preset, allow_skip_in_cycle: allowSkip, startup_command: cmd || null };
    onPick(a.id, launchOptions);
    onOpenChange(false);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (agents.length === 0) return;
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      if (optIdx !== -1) return; // horizontal nav only applies to the agent row
      e.preventDefault();
      e.stopPropagation();
      const delta = e.key === "ArrowRight" ? 1 : -1;
      switchAgent((selectedIdx + delta + agents.length) % agents.length);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setOptIdx((i) => Math.min(i + 1, startupRowIdx));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setOptIdx((i) => Math.max(i - 1, -1));
    } else if (e.key === " " && optIdx >= 0 && optIdx < startupRowIdx) {
      e.preventDefault();
      e.stopPropagation();
      toggleRow(optIdx);
    } else if (e.key === "Enter") {
      // BUG-16: bubbling would click the hovered sidebar row.
      e.preventDefault();
      e.stopPropagation();
      commit(selectedIdx);
    } else if (/^[1-9]$/.test(e.key) && optIdx === -1) {
      const i = parseInt(e.key, 10) - 1;
      if (i < agents.length) {
        e.preventDefault();
        e.stopPropagation();
        commit(i);
      }
    }
  }

  function renderCheckRow(r: OptRow, row: number) {
    const danger = r.kind === "preset" && r.value === "bypass";
    const checked = r.kind === "allow" ? allowSkip : preset === r.value;
    const focused = optIdx === row;
    // The allow toggle is moot once the session already starts in bypass.
    const dimmed = r.kind === "allow" && preset === "bypass";
    const label =
      r.kind === "allow"
        ? "Allow skip in Shift+Tab cycle"
        : (PRESET_LABELS[r.value] ?? r.value);
    return (
      <button
        key={r.kind === "allow" ? "allow" : r.value}
        onClick={() => {
          setOptIdx(row);
          toggleRow(row);
        }}
        className={`flex items-center gap-2 px-2.5 py-1 text-left text-[11px] transition-colors ${
          focused ? "bg-secondary" : "hover:bg-secondary/50"
        } ${
          dimmed
            ? "text-muted-foreground/40"
            : danger
              ? "text-red-400"
              : checked
                ? "text-foreground"
                : "text-muted-foreground"
        }`}
      >
        <span
          className={`flex size-3 shrink-0 items-center justify-center rounded-[3px] border ${
            checked
              ? danger
                ? "border-red-500 bg-red-500/20"
                : "border-orange-500 bg-orange-500/20"
              : "border-border"
          }`}
        >
          {checked && <Check className="size-2.5" />}
        </span>
        {label}
        {danger && <span className="text-[9px] text-red-400/60">dangerous</span>}
      </button>
    );
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
          className="flex w-[28rem] max-w-full flex-col gap-2 outline-none rounded p-0.5"
        >
          <div className="flex flex-row items-stretch gap-2 overflow-x-auto">
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
                  onClick={() => switchAgent(i)}
                  onDoubleClick={() => commit(i)}
                  disabled={disabled}
                  className={`relative flex min-w-24 flex-1 flex-col items-center gap-2 rounded-lg border px-3 py-3 text-center transition-colors ${
                    disabled
                      ? "border-border/40 bg-card/40 text-muted-foreground/50 cursor-not-allowed"
                      : selected
                        ? optIdx === -1
                          ? "border-orange-500 bg-orange-500/10 text-foreground"
                          : "border-orange-500/40 bg-orange-500/5 text-foreground"
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

          {agents.length > 0 && (
            <div className="flex flex-col rounded-md border border-border/60 bg-card/40">
              <span className="px-2.5 pt-1.5 pb-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                Launch options
              </span>
              {rows.map((r, row) => renderCheckRow(r, row))}
              {rows.length === 0 && (
                <span className="px-2.5 py-1 text-[10px] text-muted-foreground/60">
                  This agent exposes no permission flags.
                </span>
              )}
              <div
                className={`flex items-center gap-2 px-2.5 py-1 ${
                  optIdx === startupRowIdx ? "bg-secondary" : ""
                }`}
              >
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground/70">$</span>
                <input
                  ref={startupInputRef}
                  type="text"
                  value={startupCmd}
                  onChange={(e) => setStartupCmd(e.target.value)}
                  onFocus={() => setOptIdx(startupRowIdx)}
                  onKeyDown={(e) => {
                    if (e.key === "ArrowUp") {
                      e.preventDefault();
                      e.stopPropagation();
                      setOptIdx(startupRowIdx - 1 >= 0 ? startupRowIdx - 1 : -1);
                    } else if (e.key === "Enter") {
                      e.preventDefault();
                      e.stopPropagation();
                      commit(selectedIdx);
                    }
                  }}
                  placeholder="Startup command (nvm use, source .env…)"
                  className="w-full bg-transparent font-mono text-[10px] text-foreground outline-none placeholder:text-muted-foreground/40"
                />
              </div>
            </div>
          )}
        </div>
        <p className="text-[10px] text-muted-foreground/60">
          ← → agent • ↑ ↓ options • Space toggle • 1–9 jump • Enter create • Esc cancel
        </p>
      </DialogContent>
    </Dialog>
  );
}
