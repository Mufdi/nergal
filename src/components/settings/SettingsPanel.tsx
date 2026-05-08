import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { configAtom } from "@/stores/config";
import { availableAgentsAtom, type AgentDetection } from "@/stores/agent";
import { invoke } from "@/lib/tauri";
import type { AvailableAgent, Config, PathValidation } from "@/lib/types";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { CheckCircle2, AlertTriangle, XCircle, Info, FolderTree, Bot, Pencil, Palette, Terminal, NotebookText, RefreshCw, Check } from "lucide-react";
import { scratchpadPathAtom, reloadTabsFromBackend } from "@/stores/scratchpad";
import { VISIBLE_THEMES, normalizeThemeId, type Theme } from "@/lib/themes";

type PathKind = "dir" | "file" | "executable";

function ValidatedPathField({
  configKey,
  label,
  placeholder,
  kind,
  help,
  value,
  onChange,
}: {
  configKey: string;
  label: string;
  placeholder: string;
  kind: PathKind;
  help?: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const [validation, setValidation] = useState<PathValidation | null>(null);
  const [validating, setValidating] = useState(false);

  const runValidation = useCallback(async (path: string) => {
    if (!path.trim()) {
      setValidation(null);
      return;
    }
    setValidating(true);
    try {
      const result = await invoke<PathValidation>("validate_path", { path, kind });
      setValidation(result);
    } catch (err) {
      setValidation({
        exists: false,
        is_dir: false,
        is_file: false,
        is_executable: false,
        resolved_path: null,
        error: String(err),
      });
    } finally {
      setValidating(false);
    }
  }, [kind]);

  useEffect(() => {
    const handle = setTimeout(() => runValidation(value), 250);
    return () => clearTimeout(handle);
  }, [value, runValidation]);

  const status = (() => {
    // Show "Checking…" while validating OR while we don't have a result
    // yet for a non-empty path. Reserves the row's vertical space from
    // the first paint so the modal doesn't visibly grow when the async
    // invoke resolves (~300ms after open). Empty paths still get null
    // (no row) so the layout stays tight on unconfigured fields.
    const hasPendingValidation = !validation && value.trim().length > 0;
    if (validating || hasPendingValidation) {
      return { icon: <Info size={12} className="text-muted-foreground animate-pulse" />, text: "Checking…", tone: "muted" as const };
    }
    if (!validation) return null;
    if (validation.error || !validation.exists) {
      return { icon: <XCircle size={12} className="text-destructive" />, text: validation.error ?? "Not found", tone: "destructive" as const };
    }
    if (kind === "dir" && !validation.is_dir) {
      return { icon: <AlertTriangle size={12} className="text-amber-500" />, text: "Path exists but is not a directory", tone: "warning" as const };
    }
    if (kind === "executable" && !validation.is_executable) {
      return { icon: <AlertTriangle size={12} className="text-amber-500" />, text: "Path exists but is not executable", tone: "warning" as const };
    }
    if (kind === "file" && !validation.is_file) {
      return { icon: <AlertTriangle size={12} className="text-amber-500" />, text: "Path exists but is not a file", tone: "warning" as const };
    }
    const detail = validation.resolved_path && validation.resolved_path !== value
      ? `OK · ${validation.resolved_path}`
      : "OK";
    return { icon: <CheckCircle2 size={12} className="text-emerald-500" />, text: detail, tone: "success" as const };
  })();

  return (
    <div className="grid gap-1.5">
      <Label htmlFor={`setting-${configKey}`}>{label}</Label>
      <Input
        id={`setting-${configKey}`}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
      />
      {status && (
        <div className="flex items-center gap-1.5 text-xs">
          {status.icon}
          <span
            className={
              status.tone === "destructive"
                ? "text-destructive"
                : status.tone === "warning"
                ? "text-amber-500"
                : "text-muted-foreground"
            }
          >
            {status.text}
          </span>
        </div>
      )}
      {help && <p className="text-xs text-muted-foreground">{help}</p>}
    </div>
  );
}

function ScratchpadPathField() {
  const [path, setPath] = useAtom(scratchpadPathAtom);
  const [draft, setDraft] = useState(path);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setDraft(path);
  }, [path]);

  async function handleApply() {
    if (!draft || draft === path) return;
    setBusy(true);
    try {
      const canonical = await invoke<string>("scratchpad_set_path", { newPath: draft });
      setPath(canonical);
      await reloadTabsFromBackend();
    } catch (err) {
      console.error("[settings] scratchpad_set_path failed:", err);
    } finally {
      setBusy(false);
    }
  }

  async function handleReveal() {
    try {
      await invoke("scratchpad_reveal_in_file_manager");
    } catch (err) {
      console.error("[settings] reveal failed:", err);
    }
  }

  async function handleResetDefault() {
    setBusy(true);
    try {
      const def = await invoke<string>("scratchpad_default_path");
      const canonical = await invoke<string>("scratchpad_set_path", { newPath: def });
      setPath(canonical);
      setDraft(canonical);
      await reloadTabsFromBackend();
    } catch (err) {
      console.error("[settings] reset default failed:", err);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="grid gap-2">
      <Label htmlFor="setting-scratchpad-path">Scratchpad Directory</Label>
      <div className="flex gap-2">
        <Input
          id="setting-scratchpad-path"
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="~/.config/cluihud/scratchpad/"
          className="flex-1"
        />
        <Button
          variant="outline"
          size="sm"
          onClick={handleApply}
          disabled={busy || draft === path || !draft}
        >
          Apply
        </Button>
      </div>
      <div className="flex gap-2">
        <Button variant="ghost" size="xs" onClick={handleReveal}>
          Reveal in file manager
        </Button>
        <Button variant="ghost" size="xs" onClick={handleResetDefault} disabled={busy}>
          Reset to default
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        Path where scratchpad notes (.md) live. Changing it closes open tabs and reloads from the new path.
      </p>
    </div>
  );
}

function DetectedAgentsList({ onRescan, rescanning }: { onRescan: () => void; rescanning: boolean }) {
  const agents = useAtomValue(availableAgentsAtom);
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between">
        <Label>Detected Agents</Label>
        <Button variant="ghost" size="xs" onClick={onRescan} disabled={rescanning}>
          <RefreshCw size={12} className={rescanning ? "animate-spin" : ""} />
          {rescanning ? "Scanning…" : "Rescan"}
        </Button>
      </div>
      {agents.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {rescanning ? "Scanning filesystem…" : "No agents detected. Click Rescan to retry."}
        </p>
      ) : (
        <ul className="space-y-1 text-xs">
          {agents.map((a) => (
            <li
              key={a.id}
              className="flex items-center justify-between rounded border border-border/40 bg-muted/30 px-2 py-1"
            >
              <span className="font-mono">{a.id}</span>
              <span className={a.installed ? "text-foreground" : "text-muted-foreground"}>
                {a.installed ? a.version ?? "installed" : "not detected"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface SettingsProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

interface EditorInfo {
  id: string;
  name: string;
  command: string;
  available: boolean;
}

type StringConfigKey = {
  [K in keyof Config]: Config[K] extends string ? K : never;
}[keyof Config];

type BooleanConfigKey = {
  [K in keyof Config]: Config[K] extends boolean ? K : never;
}[keyof Config];

const TOGGLE_FIELDS: { key: BooleanConfigKey; label: string; help: string }[] = [
  {
    key: "terminal_kitty_keyboard",
    label: "Kitty keyboard protocol",
    help: "Distinct encoding for Ctrl+Backspace, Shift+Enter, Alt+letter.",
  },
];

function ThemePreviewCard({
  theme,
  selected,
  onSelect,
}: {
  theme: Theme;
  selected: boolean;
  onSelect: () => void;
}) {
  const { preview } = theme;
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      data-theme-card
      data-theme-id={theme.id}
      className={`group relative flex flex-col gap-2 rounded-md border p-2 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-2 focus-visible:ring-offset-background ${
        selected
          ? "border-orange-500 bg-orange-500/10"
          : "border-border bg-card hover:border-border/80 hover:bg-secondary/40"
      }`}
    >
      <div
        className="relative h-20 overflow-hidden rounded"
        style={{
          backgroundColor: preview.background,
          boxShadow: `inset 0 0 0 1px ${preview.border}`,
        }}
      >
        <div
          className="absolute rounded-sm"
          style={{
            left: 6,
            top: 6,
            bottom: 6,
            width: 22,
            backgroundColor: preview.card,
            boxShadow: `inset 0 0 0 1px ${preview.border}`,
          }}
        />
        <div
          className="absolute rounded-sm"
          style={{
            left: 34,
            right: 6,
            top: 6,
            height: 8,
            backgroundColor: preview.secondary,
          }}
        />
        <div
          className="absolute rounded-sm"
          style={{
            left: 34,
            top: 20,
            width: 40,
            height: 6,
            backgroundColor: preview.foreground,
            opacity: 0.85,
          }}
        />
        <div
          className="absolute rounded-sm"
          style={{
            left: 34,
            top: 30,
            width: 56,
            height: 4,
            backgroundColor: preview.mutedForeground,
            opacity: 0.7,
          }}
        />
        <div
          className="absolute rounded-sm"
          style={{
            left: 34,
            bottom: 6,
            width: 36,
            height: 12,
            backgroundColor: preview.primary,
          }}
        />
      </div>
      <div className="flex items-center justify-between px-0.5">
        <span className="text-xs font-medium text-foreground">{theme.label}</span>
        {selected && (
          <span className="flex size-4 items-center justify-center rounded-full bg-orange-500 text-white">
            <Check size={10} strokeWidth={3} />
          </span>
        )}
      </div>
      {/* Font sample row: each "Aa" uses one of the theme's fonts so the
          card surfaces the typographic personality (interface / markdown /
          terminal) at a glance. */}
      <div className="flex items-center gap-2 px-0.5 text-[11px] text-muted-foreground">
        <span style={{ fontFamily: theme.fonts.interface }} title="Interface font">Aa</span>
        <span style={{ fontFamily: theme.fonts.markdown }} title="Markdown font">Aa</span>
        <span style={{ fontFamily: theme.fonts.terminal }} title="Terminal font">Aa</span>
      </div>
    </button>
  );
}


type SectionId = "paths" | "agents" | "editor" | "appearance" | "terminal" | "scratchpad";

const SECTIONS: { id: SectionId; label: string; icon: typeof FolderTree }[] = [
  { id: "paths", label: "Paths", icon: FolderTree },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "editor", label: "Editor", icon: Pencil },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "terminal", label: "Terminal", icon: Terminal },
  { id: "scratchpad", label: "Scratchpad", icon: NotebookText },
];

export function SettingsPanel({ open, onOpenChange }: SettingsProps) {
  const [config, setConfig] = useAtom(configAtom);
  const [editors, setEditors] = useState<EditorInfo[]>([]);
  const [activeSection, setActiveSection] = useState<SectionId>("paths");
  const agents = useAtomValue(availableAgentsAtom);
  const setAvailableAgents = useSetAtom(availableAgentsAtom);
  const [rescanning, setRescanning] = useState(false);
  const navRef = useRef<HTMLElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);

  const FOCUSABLE_SELECTOR =
    'input:not([disabled]):not([aria-disabled="true"]), [role="combobox"]:not([disabled]):not([aria-disabled="true"]), textarea:not([disabled]):not([aria-disabled="true"]), button:not([disabled]):not([aria-disabled="true"])';

  const rescanAgents = useCallback(async () => {
    setRescanning(true);
    const startedAt = Date.now();
    try {
      const result = await invoke<AvailableAgent[]>("list_available_agents");
      const detections: AgentDetection[] = result.map((a) => ({
        id: a.id,
        installed: a.installed,
        binary_path: a.binary_path,
        config_path: a.config_path,
        version: a.version,
      }));
      setAvailableAgents(detections);
    } catch (err) {
      console.error("[settings] list_available_agents failed:", err);
    } finally {
      const elapsed = Date.now() - startedAt;
      if (elapsed < 450) await new Promise((r) => setTimeout(r, 450 - elapsed));
      setRescanning(false);
    }
  }, [setAvailableAgents]);

  useEffect(() => {
    if (open) {
      invoke<EditorInfo[]>("detect_editors").then(setEditors).catch(() => {});
      rescanAgents();
    }
  }, [open, rescanAgents]);

  useEffect(() => {
    if (!open) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
      const idx = parseInt(e.key, 10);
      if (!Number.isNaN(idx) && idx >= 1 && idx <= SECTIONS.length) {
        e.preventDefault();
        e.stopPropagation();
        setActiveSection(SECTIONS[idx - 1].id);
      }
    }
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [open]);

  // Focus the active nav button (not the first input) so Alt+N stays usable
  // and Tab walks into the form fields naturally. Double-rAF defers past
  // BaseUI Dialog's capture-phase focus trap, which would otherwise win on
  // reopen and leave focus on the close button (breaking arrow-key nav in
  // the appearance grid on the second open).
  useEffect(() => {
    if (!open) return;
    let raf2: number | null = null;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        const activeBtn = navRef.current?.querySelector<HTMLButtonElement>(
          'button[data-active="true"]',
        );
        activeBtn?.focus({ preventScroll: true });
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      if (raf2 !== null) cancelAnimationFrame(raf2);
    };
  }, [open, activeSection]);

  // Appearance section is a 2-col grid of buttons (no form fields), so the
  // nav-button-only focus leaves the user one Tab away from interaction with
  // poor discoverability. Hand off focus to the selected (or first) theme
  // card. Double-rAF for the same BaseUI override reason as above.
  useEffect(() => {
    if (!open || activeSection !== "appearance") return;
    let raf2: number | null = null;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        const cards = contentRef.current?.querySelectorAll<HTMLButtonElement>(
          "[data-theme-card]",
        );
        if (!cards || cards.length === 0) return;
        const selectedId = normalizeThemeId(config.theme_mode);
        const target =
          Array.from(cards).find((c) => c.dataset.themeId === selectedId) ?? cards[0];
        target.focus({ preventScroll: true });
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      if (raf2 !== null) cancelAnimationFrame(raf2);
    };
  }, [open, activeSection, config.theme_mode]);

  // Arrow-key navigation between theme cards. Only fires when focus is on a
  // theme card, so it never interferes with form fields in other sections.
  // Capture phase + stopPropagation prevents BaseUI Dialog from intercepting.
  useEffect(() => {
    if (!open) return;
    function handleArrows(e: KeyboardEvent) {
      if (
        e.key !== "ArrowRight" &&
        e.key !== "ArrowLeft" &&
        e.key !== "ArrowDown" &&
        e.key !== "ArrowUp"
      ) {
        return;
      }
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      const target = e.target as HTMLElement | null;
      if (!target || !target.closest("[data-theme-card]")) return;

      const cards = contentRef.current?.querySelectorAll<HTMLButtonElement>("[data-theme-card]");
      if (!cards || cards.length === 0) return;
      const list = Array.from(cards);
      const current = target.closest<HTMLButtonElement>("[data-theme-card]");
      const idx = current ? list.indexOf(current) : -1;
      if (idx === -1) return;

      e.preventDefault();
      e.stopPropagation();

      const forward = e.key === "ArrowRight" || e.key === "ArrowDown";
      const nextIdx = forward ? (idx + 1) % list.length : (idx - 1 + list.length) % list.length;
      list[nextIdx].focus();
    }
    window.addEventListener("keydown", handleArrows, true);
    return () => window.removeEventListener("keydown", handleArrows, true);
  }, [open]);

  // BaseUI Dialog runs a focus trap in capture phase that breaks the nav→content
  // hand-off and the loop back from the footer. We replace it with a manual,
  // ordered trap (active nav → content → footer → loop). stopImmediatePropagation
  // wins over BaseUI; events outside our managed regions (close button, select
  // popup portal) fall through to default browser/BaseUI handling.
  useEffect(() => {
    if (!open) return;
    function getOrderedFocusables(): HTMLElement[] {
      const list: HTMLElement[] = [];
      const navActive = navRef.current?.querySelector<HTMLElement>('button[data-active="true"]');
      if (navActive) list.push(navActive);
      if (contentRef.current) {
        list.push(...Array.from(contentRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)));
      }
      if (footerRef.current) {
        list.push(...Array.from(footerRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)));
      }
      return list;
    }
    function handleTab(e: KeyboardEvent) {
      if (e.key !== "Tab" || e.ctrlKey || e.altKey || e.metaKey) return;
      const target = e.target as HTMLElement | null;
      if (!target) return;

      const inManaged =
        navRef.current?.contains(target) ||
        contentRef.current?.contains(target) ||
        footerRef.current?.contains(target);
      if (!inManaged) return;

      const focusables = getOrderedFocusables();
      const idx = focusables.indexOf(target);
      if (idx === -1) return;

      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();

      const nextIdx = e.shiftKey
        ? (idx === 0 ? focusables.length - 1 : idx - 1)
        : (idx === focusables.length - 1 ? 0 : idx + 1);
      focusables[nextIdx].focus();
    }
    window.addEventListener("keydown", handleTab, true);
    return () => window.removeEventListener("keydown", handleTab, true);
  }, [open]);

  function handleTextChange(key: StringConfigKey, value: string) {
    setConfig((prev: Config) => ({ ...prev, [key]: value }));
  }

  function handleToggleChange(key: BooleanConfigKey, value: boolean) {
    setConfig((prev: Config) => ({ ...prev, [key]: value }));
  }

  function handleDefaultAgentChange(value: string) {
    setConfig((prev: Config) => ({ ...prev, default_agent: value || null }));
  }

  function handleSave() {
    invoke("save_config", { config }).catch(() => {});
    onOpenChange(false);
  }

  // Ctrl/Cmd+Enter saves from anywhere in the dialog. Skip when focus is in a
  // textarea so multi-line inputs (future-proof) keep their newline behavior.
  useEffect(() => {
    if (!open) return;
    function handleSaveShortcut(e: KeyboardEvent) {
      if (e.key !== "Enter") return;
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.altKey || e.shiftKey) return;
      const target = e.target as HTMLElement | null;
      if (target?.tagName === "TEXTAREA") return;
      e.preventDefault();
      e.stopPropagation();
      handleSave();
    }
    window.addEventListener("keydown", handleSaveShortcut, true);
    return () => window.removeEventListener("keydown", handleSaveShortcut, true);
  }, [open, config]);

  const availableEditors = editors.filter((e) => e.available);
  const installedAgents = agents.filter((a) => a.installed);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl h-[640px]">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure paths and preferences. Press <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Alt+1</kbd>–<kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Alt+{SECTIONS.length}</kbd> to jump between sections, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Tab</kbd> to enter the form, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Ctrl+Enter</kbd> to save, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Ctrl+,</kbd> to toggle.
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-[180px_1fr] gap-5 h-[460px]">
          <nav ref={navRef} className="flex flex-col gap-0.5 border-r border-border/40 pr-2">
            {SECTIONS.map((section, idx) => {
              const Icon = section.icon;
              const isActive = section.id === activeSection;
              return (
                <button
                  key={section.id}
                  type="button"
                  tabIndex={isActive ? 0 : -1}
                  data-active={isActive}
                  onClick={() => setActiveSection(section.id)}
                  className={`flex items-center justify-between rounded-md px-2 py-1.5 text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring ${
                    isActive
                      ? "bg-secondary text-foreground"
                      : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                  }`}
                >
                  <span className="flex items-center gap-2">
                    <Icon size={14} className="shrink-0" />
                    <span>{section.label}</span>
                  </span>
                  <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border border-border/40 text-muted-foreground">⌥{idx + 1}</kbd>
                </button>
              );
            })}
          </nav>

          <div ref={contentRef} className="overflow-y-auto pr-1 max-h-[60vh]">
            {activeSection === "paths" && (
              <div className="space-y-4">
                <ValidatedPathField
                  configKey="claude_binary"
                  label="Claude Binary"
                  placeholder="claude"
                  kind="executable"
                  help="Binary used to spawn Claude Code sessions. Plain name uses PATH lookup."
                  value={config.claude_binary}
                  onChange={(v) => handleTextChange("claude_binary", v)}
                />
                <ValidatedPathField
                  configKey="plans_directory"
                  label="Plans Directory"
                  placeholder="~/.claude/plans"
                  kind="dir"
                  help="Where plan files (.md) are watched and saved."
                  value={config.plans_directory}
                  onChange={(v) => handleTextChange("plans_directory", v)}
                />
                <ValidatedPathField
                  configKey="transcripts_directory"
                  label="Transcripts Directory"
                  placeholder="~/.claude/projects"
                  kind="dir"
                  help="Where Claude Code stores per-session .jsonl transcripts."
                  value={config.transcripts_directory}
                  onChange={(v) => handleTextChange("transcripts_directory", v)}
                />
                <ValidatedPathField
                  configKey="default_shell"
                  label="Default Shell"
                  placeholder="/bin/bash"
                  kind="executable"
                  help="Shell used when launching the PTY for sessions."
                  value={config.default_shell}
                  onChange={(v) => handleTextChange("default_shell", v)}
                />
                <div className="grid gap-1.5">
                  <Label htmlFor="setting-hook_socket_path" className="flex items-center gap-1.5">
                    Hook Socket Path
                    <span className="text-[10px] uppercase tracking-wide text-muted-foreground">read-only</span>
                  </Label>
                  <Input
                    id="setting-hook_socket_path"
                    type="text"
                    value={config.hook_socket_path}
                    disabled
                    className="opacity-70"
                  />
                  <p className="text-xs text-muted-foreground">
                    Unix socket where hooks send events. Edit via config file if you need to change it.
                  </p>
                </div>
              </div>
            )}

            {activeSection === "agents" && (
              <div className="space-y-4">
                <div className="grid gap-2">
                  <Label htmlFor="setting-default_agent">Default Agent</Label>
                  <Select
                    id="setting-default_agent"
                    value={config.default_agent ?? ""}
                    onValueChange={handleDefaultAgentChange}
                    options={[
                      { value: "", label: "Auto (priority list)" },
                      ...installedAgents.map((agent) => ({ value: agent.id, label: agent.id })),
                    ]}
                  />
                  <p className="text-xs text-muted-foreground">
                    Agent used when creating new sessions. "Auto" falls back to the registry priority list (CC &gt; Codex &gt; OpenCode &gt; Pi).
                  </p>
                </div>
                <DetectedAgentsList onRescan={rescanAgents} rescanning={rescanning} />
              </div>
            )}

            {activeSection === "editor" && (
              <div className="space-y-4">
                <div className="grid gap-2">
                  <Label htmlFor="setting-preferred_editor">Preferred Editor</Label>
                  <Select
                    id="setting-preferred_editor"
                    value={config.preferred_editor}
                    onValueChange={(v) => handleTextChange("preferred_editor", v)}
                    options={[
                      { value: "", label: "Auto-detect" },
                      ...availableEditors.map((editor) => ({ value: editor.id, label: editor.name })),
                    ]}
                  />
                  <p className="text-xs text-muted-foreground">
                    Used by <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Ctrl+Shift+E</kbd> to open files. Auto picks the first available from the registered list.
                  </p>
                </div>
              </div>
            )}

            {activeSection === "appearance" && (
              <div className="space-y-4">
                <div className="grid gap-2">
                  <Label>Theme</Label>
                  <div className="grid grid-cols-2 gap-2">
                    {VISIBLE_THEMES.map((theme) => (
                      <ThemePreviewCard
                        key={theme.id}
                        theme={theme}
                        selected={normalizeThemeId(config.theme_mode) === theme.id}
                        onSelect={() => handleTextChange("theme_mode", theme.id)}
                      />
                    ))}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Applied immediately. Click <strong>Save</strong> to persist across sessions.
                  </p>
                </div>
              </div>
            )}

            {activeSection === "terminal" && (
              <div className="space-y-4">
                {TOGGLE_FIELDS.map(({ key, label, help }) => (
                  <label key={key} className="flex items-start gap-3 text-sm">
                    <input
                      id={`setting-${key}`}
                      type="checkbox"
                      checked={config[key]}
                      onChange={(e) => handleToggleChange(key, e.target.checked)}
                      className="mt-0.5 h-4 w-4"
                    />
                    <span className="flex flex-col gap-0.5">
                      <span className="font-medium">{label}</span>
                      <span className="text-xs text-muted-foreground">{help}</span>
                    </span>
                  </label>
                ))}
              </div>
            )}

            {activeSection === "scratchpad" && <ScratchpadPathField />}
          </div>
        </div>

        <DialogFooter className="-mx-4 -mb-4 bg-transparent border-t-0 px-4 pt-2 pb-4">
          <div ref={footerRef} className="contents">
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button onClick={handleSave}>
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
