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
import { CheckCircle2, AlertTriangle, XCircle, Info, FolderTree, Bot, Pencil, Palette, Terminal, NotebookText, RefreshCw } from "lucide-react";
import { scratchpadPathAtom, reloadTabsFromBackend } from "@/stores/scratchpad";

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
    if (validating) return { icon: <Info size={12} className="text-muted-foreground animate-pulse" />, text: "Checking…", tone: "muted" as const };
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

const THEME_OPTIONS: { value: string; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
];

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
  // and Tab walks into the form fields naturally.
  useEffect(() => {
    if (!open || !navRef.current) return;
    const activeBtn = navRef.current.querySelector<HTMLButtonElement>('button[data-active="true"]');
    activeBtn?.focus({ preventScroll: true });
  }, [open, activeSection]);

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

  const availableEditors = editors.filter((e) => e.available);
  const installedAgents = agents.filter((a) => a.installed);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl w-full [animation:none!important] [&[data-open]]:opacity-100">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure paths and preferences. Press <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Alt+1</kbd>–<kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Alt+{SECTIONS.length}</kbd> to jump between sections, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Tab</kbd> to enter the form, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Ctrl+,</kbd> to toggle.
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-[180px_1fr] gap-5 min-h-[420px]">
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
                  <Label htmlFor="setting-theme_mode">Theme</Label>
                  <Select
                    id="setting-theme_mode"
                    value={config.theme_mode}
                    onValueChange={(v) => handleTextChange("theme_mode", v)}
                    options={THEME_OPTIONS}
                  />
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

        <DialogFooter>
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
