import { useState, useEffect, useRef } from "react";
import { Check, Plus, X } from "lucide-react";
import { KeyHints } from "@/components/ui/KeyHints";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { AvailableAgent } from "@/lib/types";
import type { EnvShellDef, LaunchOptions, PermissionPreset } from "@/stores/workspace";
import { invoke } from "@/lib/tauri";
import claudeLogo from "@/assets/agents/claude.svg";
import codexLogo from "@/assets/agents/codex.png";
import opencodeLogo from "@/assets/agents/opencode.svg";
import piLogo from "@/assets/agents/pi.svg";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  agents: AvailableAgent[];
  sessionName: string;
  /// Resolves the workspace's environment-shell suggestion library.
  workspaceId?: string | null;
  /// Workspace root — base for validating relative env-shell cwd paths.
  repoPath?: string | null;
  preselectedId?: string | null;
  onPick: (
    agentId: string,
    launchOptions: LaunchOptions | null,
    envShells: EnvShellDef[],
  ) => void;
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
  workspaceId,
  repoPath,
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
  const [envShells, setEnvShells] = useState<EnvShellDef[]>([]);
  const [suggestions, setSuggestions] = useState<EnvShellDef[]>([]);
  /// Per-row cwd validation: true ok, false missing dir, undefined pending.
  const [cwdValid, setCwdValid] = useState<Record<number, boolean>>({});
  const [cwdBlocked, setCwdBlocked] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const startupInputRef = useRef<HTMLInputElement>(null);
  const envCmdRefs = useRef<Array<HTMLInputElement | null>>([]);
  const envLabelRefs = useRef<Array<HTMLInputElement | null>>([]);
  const envCwdRefs = useRef<Array<HTMLInputElement | null>>([]);
  const envDelRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const sugRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const agent = agents[selectedIdx];
  // "default" is the unchecked state, not a row — checking nothing = default.
  const rows: OptRow[] = [
    ...((agent?.permission_presets ?? [])
      .filter((p) => p !== "default")
      .map((p) => ({ kind: "preset", value: p as PermissionPreset }) as OptRow)),
    ...(agent?.allow_skip_cycle_supported ? [{ kind: "allow" } as OptRow] : []),
  ];
  // Vertical index space: -1 agent row · 0..rows-1 option rows · startup
  // input · suggestion chips (when any) · one row per env shell · the
  // add-shell row. Horizontal movement inside a row uses DOM focus (chips,
  // label/command/✕) — caret-boundary jumps keep text editing natural.
  const startupRowIdx = rows.length;
  const hasSuggestions = suggestions.length > 0;
  const suggRowIdx = hasSuggestions ? startupRowIdx + 1 : -99;
  const envRowStart = startupRowIdx + 1 + (hasSuggestions ? 1 : 0);
  const addRowIdx = envRowStart + envShells.length;

  useEffect(() => {
    if (!open) return;
    const idx = preselectedId ? agents.findIndex((a) => a.id === preselectedId) : 0;
    setSelectedIdx(idx >= 0 ? idx : 0);
    setOptIdx(-1);
    setPreset("default");
    setAllowSkip(false);
    setStartupCmd("");
    setEnvShells([]);
    setCwdValid({});
    setCwdBlocked(false);
    requestAnimationFrame(() => listRef.current?.focus());
  }, [open, preselectedId, agents]);

  useEffect(() => {
    if (!open || !workspaceId) {
      setSuggestions([]);
      return;
    }
    let cancelled = false;
    invoke<EnvShellDef[]>("get_workspace_env_shell_suggestions", { workspaceId })
      .then((defs) => {
        if (!cancelled) setSuggestions(defs);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [open, workspaceId]);

  // Live-validate every non-empty cwd (debounced) so a typo'd directory is
  // flagged before the session is created. Mirrors the resolution the spawn
  // applies: `~` expands, relative resolves against the workspace root.
  const cwdKey = envShells.map((sh) => sh.cwd ?? "").join("\n");
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const t = setTimeout(() => {
      // Rebuild from scratch — row removal shifts indexes and a stale
      // verdict must not stick to the wrong row.
      setCwdValid({});
      envShells.forEach((sh, i) => {
        const raw = sh.cwd?.trim();
        if (!raw) return;
        const path =
          raw.startsWith("/") || raw.startsWith("~")
            ? raw
            : `${repoPath ?? ""}/${raw}`;
        invoke<{ exists: boolean; is_dir: boolean }>("validate_path", {
          path,
          kind: "dir",
          caseInsensitive: true,
        })
          .then((v) => {
            if (cancelled) return;
            setCwdValid((prev) => ({ ...prev, [i]: v.exists && v.is_dir }));
          })
          .catch(() => {});
      });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [open, cwdKey, repoPath]);

  // Scroll the active card into view on selection change so the rightmost
  // entries don't get clipped when the row overflows.
  useEffect(() => {
    if (!open) return;
    const el = itemRefs.current[selectedIdx];
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedIdx, open]);

  // Input/chip rows own real DOM focus; every other row keeps focus on the
  // container so its keydown handler drives navigation.
  useEffect(() => {
    if (!open) return;
    if (optIdx === startupRowIdx) startupInputRef.current?.focus();
    else if (optIdx === suggRowIdx)
      sugRefs.current.find((el) => el && !el.disabled)?.focus();
    else if (optIdx >= envRowStart && optIdx < addRowIdx)
      envCmdRefs.current[optIdx - envRowStart]?.focus();
    else listRef.current?.focus();
  }, [optIdx, startupRowIdx, suggRowIdx, envRowStart, addRowIdx, open]);

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

  const hasCwdProblem = envShells.some(
    (sh, i) => sh.cwd?.trim() && cwdValid[i] !== true,
  );

  function commit(idx: number) {
    const a = agents[idx];
    if (!a || !a.installed) return;
    // Hard gate: a mistyped directory must be fixed or cleared, not
    // silently fall back at spawn.
    if (hasCwdProblem) {
      setCwdBlocked(true);
      return;
    }
    const cmd = startupCmd.trim();
    const launchOptions: LaunchOptions | null =
      preset === "default" && !allowSkip && !cmd
        ? null
        : { permission_preset: preset, allow_skip_in_cycle: allowSkip, startup_command: cmd || null };
    const shells = envShells
      .map((sh) => ({
        label: sh.label.trim(),
        command: sh.command.trim(),
        cwd: sh.cwd?.trim() || null,
      }))
      .filter((sh) => sh.command);
    onPick(a.id, launchOptions, shells);
    onOpenChange(false);
  }

  function updateEnvShell(i: number, patch: Partial<EnvShellDef>) {
    setEnvShells((prev) => prev.map((sh, idx) => (idx === i ? { ...sh, ...patch } : sh)));
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
      setOptIdx((i) => Math.min(i + 1, addRowIdx));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setOptIdx((i) => Math.max(i - 1, -1));
    } else if (e.key === " " && optIdx >= 0 && optIdx < startupRowIdx) {
      e.preventDefault();
      e.stopPropagation();
      toggleRow(optIdx);
    } else if ((e.key === " " || e.key === "Enter") && optIdx === addRowIdx) {
      e.preventDefault();
      e.stopPropagation();
      // Post-add, the old addRowIdx IS the new row's index — focus lands in
      // its command input via the focus effect.
      setEnvShells((prev) => [...prev, { label: "", command: "" }]);
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
                  placeholder="Prelude — must exit (nvm use, source .env…)"
                  title="Runs in the agent terminal before the agent, which inherits its env. A command that never exits blocks the agent — long-running commands go in Environment shells below."
                  className="w-full bg-transparent font-mono text-[10px] text-foreground outline-none placeholder:text-muted-foreground/40"
                />
              </div>
            </div>
          )}

          {agents.length > 0 && (
            <div className="flex flex-col rounded-md border border-border/60 bg-card/40">
              <span className="px-2.5 pt-1.5 pb-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                Environment shells
              </span>
              {hasSuggestions && (
                <div className="flex flex-wrap gap-1 px-2.5 pb-1">
                  {suggestions.map((sg, i) => {
                    const added = envShells.some(
                      (sh) => sh.command.trim() === sg.command.trim(),
                    );
                    return (
                      <button
                        key={i}
                        ref={(el) => {
                          sugRefs.current[i] = el;
                        }}
                        disabled={added}
                        title={sg.cwd ? `${sg.cwd} $ ${sg.command}` : sg.command}
                        onFocus={() => setOptIdx(suggRowIdx)}
                        onClick={() => {
                          setEnvShells((prev) => [...prev, { ...sg }]);
                          // The chip becomes disabled (added) and drops focus —
                          // hand it to the freshly autofilled shell row so
                          // keyboard nav continues.
                          setOptIdx(envRowStart + envShells.length);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                            e.preventDefault();
                            e.stopPropagation();
                            const dir = e.key === "ArrowRight" ? 1 : -1;
                            for (let j = i + dir; j >= 0 && j < suggestions.length; j += dir) {
                              const el = sugRefs.current[j];
                              if (el && !el.disabled) {
                                el.focus();
                                return;
                              }
                            }
                          } else if (e.key === " ") {
                            // Native button behavior triggers click on Space;
                            // just keep it from bubbling into the container's
                            // option-toggle handler.
                            e.stopPropagation();
                          } else if (e.key === "Enter") {
                            e.preventDefault();
                            e.stopPropagation();
                            setEnvShells((prev) => [...prev, { ...sg }]);
                            setOptIdx(envRowStart + envShells.length);
                          }
                        }}
                        className={`rounded-full border px-2 py-0.5 text-[10px] transition-colors outline-none focus:ring-1 focus:ring-orange-500/60 ${
                          added
                            ? "border-border/40 text-muted-foreground/40 cursor-default"
                            : "border-border text-muted-foreground hover:bg-secondary hover:text-foreground"
                        }`}
                      >
                        {sg.label.trim() || sg.command}
                      </button>
                    );
                  })}
                </div>
              )}
              {envShells.map((sh, i) => (
                <div
                  key={i}
                  className={`flex items-center gap-1.5 px-2.5 py-1 ${
                    optIdx === envRowStart + i ? "bg-secondary" : ""
                  }`}
                >
                  <input
                    ref={(el) => {
                      envLabelRefs.current[i] = el;
                    }}
                    type="text"
                    value={sh.label}
                    onChange={(e) => updateEnvShell(i, { label: e.target.value })}
                    onFocus={() => setOptIdx(envRowStart + i)}
                    onKeyDown={(e) => {
                      // Caret at the end + ArrowRight = hop to the next
                      // field; inside the text, arrows edit normally.
                      const input = e.currentTarget;
                      if (
                        e.key === "ArrowRight"
                        && input.selectionStart === input.value.length
                        && input.selectionEnd === input.value.length
                      ) {
                        e.preventDefault();
                        e.stopPropagation();
                        envCwdRefs.current[i]?.focus();
                      }
                    }}
                    placeholder="label"
                    className="w-20 shrink-0 rounded border border-border/60 bg-transparent px-1.5 py-0.5 text-[10px] text-foreground outline-none placeholder:text-muted-foreground/40 focus:border-orange-500/60"
                  />
                  <input
                    ref={(el) => {
                      envCwdRefs.current[i] = el;
                    }}
                    type="text"
                    value={sh.cwd ?? ""}
                    onChange={(e) => updateEnvShell(i, { cwd: e.target.value })}
                    onFocus={() => setOptIdx(envRowStart + i)}
                    onKeyDown={(e) => {
                      const input = e.currentTarget;
                      if (e.key === "Enter") {
                        e.preventDefault();
                        e.stopPropagation();
                        commit(selectedIdx);
                      } else if (
                        e.key === "ArrowLeft"
                        && input.selectionStart === 0
                        && input.selectionEnd === 0
                      ) {
                        e.preventDefault();
                        e.stopPropagation();
                        envLabelRefs.current[i]?.focus();
                      } else if (
                        e.key === "ArrowRight"
                        && input.selectionStart === input.value.length
                        && input.selectionEnd === input.value.length
                      ) {
                        e.preventDefault();
                        e.stopPropagation();
                        envCmdRefs.current[i]?.focus();
                      }
                    }}
                    placeholder="cwd"
                    title="Working directory — ~ expands, relative paths resolve against the workspace root. Empty = session cwd."
                    className={`w-24 shrink-0 rounded border bg-transparent px-1.5 py-0.5 font-mono text-[10px] text-foreground outline-none placeholder:text-muted-foreground/40 ${
                      sh.cwd?.trim() && cwdValid[i] === false
                        ? "border-red-500/70 focus:border-red-500"
                        : "border-border/60 focus:border-orange-500/60"
                    }`}
                  />
                  <span className="shrink-0 font-mono text-[10px] text-muted-foreground/70">$</span>
                  <input
                    ref={(el) => {
                      envCmdRefs.current[i] = el;
                    }}
                    type="text"
                    value={sh.command}
                    onChange={(e) => updateEnvShell(i, { command: e.target.value })}
                    onFocus={() => setOptIdx(envRowStart + i)}
                    onKeyDown={(e) => {
                      const input = e.currentTarget;
                      if (e.key === "Enter") {
                        e.preventDefault();
                        e.stopPropagation();
                        commit(selectedIdx);
                      } else if (
                        e.key === "ArrowLeft"
                        && input.selectionStart === 0
                        && input.selectionEnd === 0
                      ) {
                        e.preventDefault();
                        e.stopPropagation();
                        envCwdRefs.current[i]?.focus();
                      } else if (
                        e.key === "ArrowRight"
                        && input.selectionStart === input.value.length
                        && input.selectionEnd === input.value.length
                      ) {
                        e.preventDefault();
                        e.stopPropagation();
                        envDelRefs.current[i]?.focus();
                      }
                    }}
                    placeholder="pnpm dev, docker compose up…"
                    className="w-full rounded border border-border/60 bg-transparent px-1.5 py-0.5 font-mono text-[10px] text-foreground outline-none placeholder:text-muted-foreground/40 focus:border-orange-500/60"
                  />
                  <button
                    ref={(el) => {
                      envDelRefs.current[i] = el;
                    }}
                    aria-label="Remove environment shell"
                    onFocus={() => setOptIdx(envRowStart + i)}
                    onClick={() => {
                      setEnvShells((prev) => prev.filter((_, idx) => idx !== i));
                      setOptIdx((prev) => Math.min(prev, addRowIdx - 1));
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "ArrowLeft") {
                        e.preventDefault();
                        e.stopPropagation();
                        envCmdRefs.current[i]?.focus();
                      } else if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        e.stopPropagation();
                        setEnvShells((prev) => prev.filter((_, idx) => idx !== i));
                        setOptIdx((prev) => Math.min(prev, addRowIdx - 1));
                      }
                    }}
                    className="shrink-0 rounded text-muted-foreground/60 transition-colors outline-none hover:text-foreground focus:ring-1 focus:ring-orange-500/60 focus:text-foreground"
                  >
                    <X className="size-3" />
                  </button>
                </div>
              ))}
              <button
                onClick={() => setEnvShells((prev) => [...prev, { label: "", command: "" }])}
                className={`flex items-center gap-1.5 px-2.5 py-1 pb-1.5 text-left text-[10px] transition-colors hover:text-foreground ${
                  optIdx === addRowIdx ? "bg-secondary text-foreground" : "text-muted-foreground/70"
                }`}
              >
                <Plus className="size-3" />
                Add environment shell — long-running commands run in the quake terminal
              </button>
              {cwdBlocked && hasCwdProblem && (
                <span className="px-2.5 pb-1.5 text-[10px] text-red-400">
                  A working directory doesn't exist — fix or clear it to create the session.
                </span>
              )}
            </div>
          )}
        </div>
        <KeyHints
          hints={[
            { keys: "←→", label: "agent" },
            { keys: "↑↓", label: "options" },
            { keys: "Space", label: "toggle" },
            { keys: "1–9", label: "jump" },
            { keys: "Enter", label: "create" },
            { keys: "Esc", label: "cancel" },
          ]}
        />
      </DialogContent>
    </Dialog>
  );
}
