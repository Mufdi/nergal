import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { configAtom, settingsRequestedSectionAtom } from "@/stores/config";
import { getVersion } from "@tauri-apps/api/app";
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
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import { Switch } from "@/components/ui/switch";
import { Select } from "@/components/ui/select";
import { CheckCircle2, AlertTriangle, XCircle, Info, FolderTree, Bot, Pencil, Palette, Terminal, NotebookText, RefreshCw, Check, ArrowLeft, Trash2, Sliders, Download, ExternalLink, FolderOpen, ClipboardCopy, Bug, Keyboard, Network } from "lucide-react";
import { ClickUpIcon } from "@/components/icons/ClickUpIcon";
import { LinearIcon } from "@/components/icons/LinearIcon";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { toastsAtom } from "@/stores/toast";
import { check as checkUpdater } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { HexColorPicker } from "react-colorful";
import { scratchpadPathAtom, reloadTabsFromBackend } from "@/stores/scratchpad";
import {
  obsidianApplyBusyAtom,
  obsidianDefaultConfig,
  obsidianDraftAtom,
  obsidianDraftDirtyAtom,
  obsidianSelectedWorkspaceIdAtom,
  obsidianSettingsResolvedAtom,
  resetObsidianDraftAtom,
  saveObsidianConfigAtom,
} from "@/stores/obsidian";
import { activeWorkspaceAtom, workspacesAtom, activeSessionIdAtom, selectedWorkspaceIdAtom, openspecDirDraftAtom, type Workspace, type EnvShellDef } from "@/stores/workspace";
import {
  clickupSyncStatusAtom,
  clickupTokenOnDiskAtom,
  type ClickUpSyncStatus,
} from "@/stores/clickup";
import {
  linearSyncStatusAtom,
  linearKeyOnDiskAtom,
  linearTeamsAtom,
  refreshLinearMirror,
  type LinearWorkspace,
  type SyncStatus as LinearSyncStatus,
} from "@/stores/linear";
import { appStore } from "@/stores/jotaiStore";
import type { ResolvedObsidianConfig } from "@/lib/types";
import { ObsidianIcon } from "@/components/icons/ObsidianIcon";
import { KeymapSection } from "@/components/settings/KeymapSection";
import { McpSection } from "@/components/settings/McpSection";
import { keymapCaptureActiveAtom } from "@/stores/shortcuts";
import type { ObsidianConfig } from "@/lib/types";
import {
  VISIBLE_THEMES,
  normalizeThemeId,
  type Theme,
  type FontOption,
  ACCENT_PRESETS,
  INTERFACE_FONTS,
  TERMINAL_FONTS,
  MARKDOWN_FONTS,
  resolveCustomTheme,
  forkBuiltinTheme,
} from "@/lib/themes";
import type { CustomTheme } from "@/lib/types";
import { confirm as swalConfirm } from "@/lib/swal";

type PathKind = "dir" | "file" | "executable";

function ValidatedPathField({
  configKey,
  label,
  placeholder,
  kind,
  help,
  value,
  onChange,
  caseInsensitive = false,
}: {
  configKey: string;
  label: string;
  placeholder: string;
  kind: PathKind;
  help?: string;
  value: string;
  onChange: (v: string) => void;
  caseInsensitive?: boolean;
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
      const result = await invoke<PathValidation>("validate_path", { path, kind, caseInsensitive });
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
  }, [kind, caseInsensitive]);

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
          placeholder="~/.config/nergal/scratchpad/"
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

/// Per-workspace OpenSpec directory override. Lets specs live outside the code
/// repo so the repo stays clean. Validates live (case-insensitive, like the
/// Obsidian paths) and persists on the global Settings Save — no Apply button.
function OpenSpecPathField() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selectedWorkspaceId = useAtomValue(selectedWorkspaceIdAtom);
  const effective = activeWorkspace ?? workspaces.find((w) => w.id === selectedId) ?? workspaces.find((w) => w.id === selectedWorkspaceId) ?? workspaces[0] ?? null;
  const [draft, setDraft] = useAtom(openspecDirDraftAtom);

  useEffect(() => {
    if (!effective) {
      setDraft(null);
      return;
    }
    let cancelled = false;
    invoke<{ configured: string | null; default_dir: string }>("get_workspace_openspec_dir", {
      workspaceId: effective.id,
    })
      .then((info) => {
        if (cancelled) return;
        // Empty = "use the default" (shown as the field's placeholder). Prefilling
        // the default's text made a cleared path indistinguishable from a custom
        // one, so a clear looked like it "didn't take" on reopen.
        const initial = info.configured ?? "";
        setDraft({
          workspaceId: effective.id,
          value: initial,
          defaultDir: info.default_dir,
          baseline: initial,
        });
      })
      .catch((err) => console.error("[settings] get_workspace_openspec_dir failed:", err));
    return () => {
      cancelled = true;
    };
  }, [effective?.id, setDraft]);

  if (!effective) {
    return (
      <div className="grid gap-1.5">
        <Label>OpenSpec Directory</Label>
        <p className="text-xs text-muted-foreground">Add a workspace to configure its specs path.</p>
      </div>
    );
  }

  return (
    <div className="grid gap-2">
      {workspaces.length > 1 && !activeWorkspace && (
        <Select
          id="setting-openspec-workspace"
          value={effective.id}
          onValueChange={setSelectedId}
          options={workspaces.map((w) => ({ value: w.id, label: w.name }))}
        />
      )}
      <ValidatedPathField
        configKey="openspec_dir"
        label="OpenSpec Directory"
        placeholder={draft?.defaultDir ?? "<repo>/openspec"}
        kind="dir"
        caseInsensitive
        help="Where this workspace's openspec/ lives. Set a path outside the repo to keep specs separate from code; clear (or match the default) to use <repo>/openspec. Saved with Settings."
        value={draft?.value ?? ""}
        onChange={(v) => setDraft((prev) => (prev ? { ...prev, value: v } : prev))}
      />
    </div>
  );
}

/// Per-workspace library of suggested environment shells, quick-picked in
/// the new-session modal. Persists on add/remove and on blur of text edits —
/// list mutations are discrete actions, unlike path typing, so no Apply or
/// Save round-trip is needed.
function EnvShellSuggestionsField() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selectedWorkspaceId = useAtomValue(selectedWorkspaceIdAtom);
  const effective =
    activeWorkspace ?? workspaces.find((w) => w.id === selectedId) ?? workspaces.find((w) => w.id === selectedWorkspaceId) ?? workspaces[0] ?? null;
  const [items, setItems] = useState<EnvShellDef[]>([]);

  useEffect(() => {
    if (!effective) {
      setItems([]);
      return;
    }
    let cancelled = false;
    invoke<EnvShellDef[]>("get_workspace_env_shell_suggestions", { workspaceId: effective.id })
      .then((defs) => {
        if (!cancelled) setItems(defs);
      })
      .catch((err) => console.error("[settings] get_workspace_env_shell_suggestions failed:", err));
    return () => {
      cancelled = true;
    };
  }, [effective?.id]);

  function persist(next: EnvShellDef[]) {
    if (!effective) return;
    invoke("set_workspace_env_shell_suggestions", {
      workspaceId: effective.id,
      suggestions: next.filter((s) => s.command.trim()),
    }).catch((err) =>
      console.error("[settings] set_workspace_env_shell_suggestions failed:", err),
    );
  }

  function update(i: number, patch: Partial<EnvShellDef>) {
    setItems((prev) => prev.map((s, idx) => (idx === i ? { ...s, ...patch } : s)));
  }

  if (!effective) return null;

  return (
    <div className="grid gap-2">
      <Label>Environment Shell Suggestions</Label>
      {workspaces.length > 1 && !activeWorkspace && (
        <Select
          id="setting-envshell-workspace"
          value={effective.id}
          onValueChange={setSelectedId}
          options={workspaces.map((w) => ({ value: w.id, label: w.name }))}
        />
      )}
      {items.map((s, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <Input
            type="text"
            value={s.label}
            onChange={(e) => update(i, { label: e.target.value })}
            onBlur={() => persist(items)}
            placeholder="label"
            className="w-24 shrink-0"
          />
          <Input
            type="text"
            value={s.cwd ?? ""}
            onChange={(e) => update(i, { cwd: e.target.value || null })}
            onBlur={() => persist(items)}
            placeholder="cwd"
            title="Working directory — ~ expands, relative paths resolve against the workspace root. Empty = session cwd."
            className="w-28 shrink-0 font-mono"
          />
          <Input
            type="text"
            value={s.command}
            onChange={(e) => update(i, { command: e.target.value })}
            onBlur={() => persist(items)}
            placeholder="pnpm dev, docker compose up…"
            className="flex-1 font-mono"
          />
          <Button
            variant="ghost"
            size="xs"
            aria-label="Remove suggestion"
            onClick={() => {
              const next = items.filter((_, idx) => idx !== i);
              setItems(next);
              persist(next);
            }}
          >
            ✕
          </Button>
        </div>
      ))}
      <div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setItems((prev) => [...prev, { label: "", command: "" }])}
        >
          Add suggestion
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        Per-workspace presets for environment shells (long-running commands like dev servers).
        Offered as quick-picks when creating a session in {effective.name}.
      </p>
    </div>
  );
}

function useEffectiveObsidianWorkspace(): {
  workspaces: Workspace[];
  activeWorkspace: Workspace | null;
  effective: Workspace | null;
  selectedId: string | null;
  setSelectedId: (id: string | null) => void;
} {
  const workspaces = useAtomValue(workspacesAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const [selectedId, setSelectedId] = useAtom(obsidianSelectedWorkspaceIdAtom);
  const selectedWorkspaceId = useAtomValue(selectedWorkspaceIdAtom);

  const effective = activeWorkspace
    ? activeWorkspace
    : workspaces.find((w) => w.id === selectedId)
      ?? workspaces.find((w) => w.id === selectedWorkspaceId)
      ?? workspaces[0] ?? null;

  return { workspaces, activeWorkspace, effective, selectedId, setSelectedId };
}

function ObsidianSection() {
  const { workspaces, activeWorkspace, effective, setSelectedId } =
    useEffectiveObsidianWorkspace();
  const [resolved, setResolved] = useAtom(obsidianSettingsResolvedAtom);
  const [draft, setDraft] = useAtom(obsidianDraftAtom);
  const dirty = useAtomValue(obsidianDraftDirtyAtom);

  useEffect(() => {
    if (!effective) {
      setResolved(null);
      setDraft(obsidianDefaultConfig);
      return;
    }
    let cancelled = false;
    invoke<ResolvedObsidianConfig>("get_obsidian_config", { workspaceId: effective.id })
      .then(async (cfg) => {
        if (cancelled) return;
        // A workspace with no own vault inherits the SHARED vault root from a
        // sibling (same vault) — channels stay blank. Prefill it so the field
        // isn't empty for a brand-new workspace.
        let merged = cfg;
        if (!cfg.vault_root) {
          try {
            const pb = await invoke<{ vault_root: string; inherited: boolean } | null>(
              "obsidian_pre_bootstrap",
              { workspaceId: effective.id },
            );
            if (!cancelled && pb?.vault_root) merged = { ...cfg, vault_root: pb.vault_root };
          } catch {
            /* no donor vault → stay blank */
          }
        }
        if (cancelled) return;
        setResolved(merged);
        setDraft(merged);
      })
      .catch((err) => {
        console.warn("[obsidian-settings] load failed:", err);
        if (cancelled) return;
        setResolved(obsidianDefaultConfig);
        setDraft(obsidianDefaultConfig);
      });
    return () => {
      cancelled = true;
    };
  }, [effective?.id, setResolved, setDraft]);

  useEffect(() => {
    if (resolved) setDraft(resolved);
  }, [resolved, setDraft]);

  function setField<K extends keyof ObsidianConfig>(key: K, value: ObsidianConfig[K]) {
    setDraft((prev) => ({ ...prev, [key]: value }));
  }

  function nullableString(s: string): string | null {
    // Keep the raw value (incl. interior/trailing spaces) so the user can type
    // a path like "Resti Manager"; trimming per-keystroke ate the space before
    // the next char. Backend normalize_file_channels trims on save.
    return s.trim().length === 0 ? null : s;
  }

  if (workspaces.length === 0) {
    return (
      <div className="grid gap-3">
        <p className="text-xs text-muted-foreground">
          Obsidian settings are stored per workspace. Create one from the sidebar first.
        </p>
      </div>
    );
  }

  const selectorVisible = workspaces.length > 1 || !activeWorkspace;
  const workspaceOptions = workspaces.map((w) => ({ value: w.id, label: w.name }));

  return (
    <div className="grid gap-4">
      <p className="text-xs text-muted-foreground">
        Bridge to your Obsidian vault. Set <code>vault_root</code> to enable any feature; each channel is independently opt-in.
      </p>

      {selectorVisible && (
        <div className="grid gap-1.5">
          <Label htmlFor="obsidian-workspace-selector">Workspace</Label>
          <Select
            id="obsidian-workspace-selector"
            value={effective?.id ?? ""}
            onValueChange={(v) => setSelectedId(v)}
            options={workspaceOptions}
            disabled={!!activeWorkspace}
          />
          <p className="text-[11px] text-muted-foreground">
            {activeWorkspace
              ? "Showing the active workspace. Editing another workspace requires no active session in it."
              : "No active session. Pick the workspace whose Obsidian config you want to edit."}
          </p>
        </div>
      )}

      <ValidatedPathField
        configKey="obsidian_vault_root"
        label="Vault root"
        placeholder="/path/to/vault"
        kind="dir"
        caseInsensitive
        help="Master switch. When unset, every Obsidian feature stays invisible."
        value={draft.vault_root ?? ""}
        onChange={(v) => setField("vault_root", nullableString(v))}
      />

      {resolved?.vault_root && (
        <Button
          variant="outline"
          type="button"
          className="w-fit"
          onClick={() => {
            const vault =
              resolved.vault_name?.trim() ||
              resolved.vault_root?.split("/").filter(Boolean).pop() ||
              "";
            if (!vault) return;
            const uri = `obsidian://open?vault=${encodeURIComponent(vault)}`;
            invoke("obsidian_open_uri", { uri }).catch((err) =>
              console.warn("[settings] open vault failed:", err),
            );
          }}
        >
          Open vault in Obsidian
        </Button>
      )}

      <div className="grid gap-1.5">
        <Label htmlFor="obsidian-vault-name">Vault name (override)</Label>
        <Input
          id="obsidian-vault-name"
          type="text"
          value={draft.vault_name ?? ""}
          onChange={(e) => setField("vault_name", nullableString(e.target.value))}
          placeholder="MyVault"
        />
        <p className="text-xs text-muted-foreground">
          Used in <code>obsidian://open?vault=…</code> URIs. Defaults to the basename of <code>vault_root</code>.
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="obsidian-session-log-path">Session log channel</Label>
        <Input
          id="obsidian-session-log-path"
          type="text"
          value={draft.session_log_path ?? ""}
          onChange={(e) => setField("session_log_path", nullableString(e.target.value))}
          placeholder="/path/to/vault/log.md"
        />
        <p className="text-xs text-muted-foreground">
          File where hook events get appended live. Empty = disabled.
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="obsidian-quick-capture-path">Quick capture channel</Label>
        <Input
          id="obsidian-quick-capture-path"
          type="text"
          value={draft.quick_capture_path ?? ""}
          onChange={(e) => setField("quick_capture_path", nullableString(e.target.value))}
          placeholder="/path/to/vault/Inbox.md"
        />
        <p className="text-xs text-muted-foreground">
          Target file for <kbd className="px-1 py-0.5 text-[10px] rounded bg-secondary">Ctrl+Alt+Q</kbd> captures. If you omit the <code>.md</code> extension it's added on save. Empty = shortcut shows a hint instead.
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="obsidian-moc-path">MOC channel (directory)</Label>
        <Input
          id="obsidian-moc-path"
          type="text"
          value={draft.moc_path ?? ""}
          onChange={(e) => setField("moc_path", nullableString(e.target.value))}
          placeholder="/path/to/vault/MOCs/"
        />
        <p className="text-xs text-muted-foreground">
          Where per-session MOCs (Maps of Content — structured snapshot of files touched, tasks, decisions) land at session end. Empty = no MOCs are written.
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="obsidian-templates-path">Templates channel (directory)</Label>
        <Input
          id="obsidian-templates-path"
          type="text"
          value={draft.templates_path ?? ""}
          onChange={(e) => setField("templates_path", nullableString(e.target.value))}
          placeholder="/path/to/vault/Templates/"
        />
        <p className="text-xs text-muted-foreground">
          Directory of <code>template-*.md</code> files. Each appears in the command palette as "Send template: …".
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="obsidian-search-subdir">Search subdir (vault-relative)</Label>
        <Input
          id="obsidian-search-subdir"
          type="text"
          value={draft.search_subdir ?? ""}
          onChange={(e) => setField("search_subdir", nullableString(e.target.value))}
          placeholder="Projects/"
        />
        <p className="text-xs text-muted-foreground">
          Optional folder (relative to vault root) to scope vault search and the <code>@@</code> picker. Toggle whole-vault ⇄ this folder with <code>Ctrl+D</code> in the search modal. Empty = always whole vault.
        </p>
      </div>

      <div className="grid gap-3 rounded-md border border-border/40 p-3">
        <label className="flex items-start gap-3">
          <Switch
            checked={draft.render_wikilinks}
            onCheckedChange={(v) => setField("render_wikilinks", v)}
          />
          <div className="grid gap-0.5">
            <span className="text-sm font-medium">Render wikilinks</span>
            <span className="text-xs text-muted-foreground">
              Convert <code>[[Note]]</code> in transcripts and plans to clickable <code>obsidian://</code> links.
            </span>
          </div>
        </label>
        <label className="flex items-start gap-3">
          <Switch
            checked={draft.backlinks_enabled}
            onCheckedChange={(v) => setField("backlinks_enabled", v)}
          />
          <div className="grid gap-0.5">
            <span className="text-sm font-medium">Reverse backlinks</span>
            <span className="text-xs text-muted-foreground">
              When a MOC is generated, write a backlink section into every referenced vault note.
            </span>
          </div>
        </label>
      </div>

      {dirty && (
        <p className="text-xs text-amber-500">Unsaved changes — click Apply in the footer.</p>
      )}
    </div>
  );
}

function ObsidianApplyButton() {
  const draft = useAtomValue(obsidianDraftAtom);
  const dirty = useAtomValue(obsidianDraftDirtyAtom);
  const { effective } = useEffectiveObsidianWorkspace();
  const saveCfg = useSetAtom(saveObsidianConfigAtom);
  const [busy, setBusy] = useAtom(obsidianApplyBusyAtom);

  async function handleApply() {
    if (!dirty || !effective || busy) return;
    setBusy(true);
    try {
      await saveCfg({ workspaceId: effective.id, cfg: draft });
    } catch (err) {
      console.error("[settings] save_obsidian_config failed:", err);
    } finally {
      setBusy(false);
    }
  }

  if (!effective) return null;

  return (
    <Button onClick={handleApply} disabled={!dirty || busy}>
      {busy ? "Applying…" : "Apply"}
    </Button>
  );
}

function syncStateLabel(status: ClickUpSyncStatus): string {
  switch (status.state) {
    case "no_token":
      return "No token configured";
    case "needs_team":
      return "Waiting for workspace selection";
    case "syncing":
      return "Syncing…";
    case "ok":
      return status.last_sync != null
        ? `Synced ${new Date(status.last_sync * 1000).toLocaleTimeString()}`
        : "Synced";
    case "error":
      return "Sync error";
    default:
      return "Idle";
  }
}

const CLICKUP_VIEW_OPTIONS = [
  { value: "mine", label: "My tasks" },
  { value: "status", label: "Status" },
  { value: "list", label: "List" },
  { value: "assignee", label: "Assignee" },
];

function ClickUpSection() {
  const [syncStatus, setSyncStatus] = useAtom(clickupSyncStatusAtom);
  const [tokenOnDisk, setTokenOnDisk] = useAtom(clickupTokenOnDiskAtom);
  const [config, setConfig] = useAtom(configAtom);
  const pushToast = useSetAtom(toastsAtom);
  const [tokenInput, setTokenInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [resolvedUser, setResolvedUser] = useState<{ username: string; email: string } | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);

  const configured = syncStatus !== null && syncStatus.state !== "no_token";

  async function refreshStatus() {
    try {
      setSyncStatus(await invoke<ClickUpSyncStatus>("clickup_sync_status"));
    } catch {
      // The poller also pushes clickup:sync-status; a missed pull is fine.
    }
  }

  async function handleSetToken() {
    if (tokenInput.trim().length === 0 || busy) return;
    setBusy(true);
    setValidationError(null);
    try {
      const status = await invoke<{ token_on_disk: boolean }>("clickup_set_token", {
        token: tokenInput.trim(),
      });
      setTokenOnDisk(status.token_on_disk);
      setTokenInput("");
      await handleValidate();
      await refreshStatus();
    } catch (err) {
      setValidationError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleValidate() {
    setValidationError(null);
    try {
      const user = await invoke<{ id: number; username: string; email: string }>(
        "clickup_validate_token",
      );
      setResolvedUser({ username: user.username, email: user.email });
    } catch (err) {
      setResolvedUser(null);
      setValidationError(String(err));
    }
  }

  async function handleClearToken() {
    if (busy) return;
    setBusy(true);
    try {
      await invoke("clickup_clear_token");
      setResolvedUser(null);
      setValidationError(null);
      setTokenOnDisk(false);
      await refreshStatus();
      pushToast({ message: "ClickUp", description: "Token cleared", type: "info" });
    } catch (err) {
      pushToast({ message: "ClickUp", description: String(err), type: "error" });
    } finally {
      setBusy(false);
    }
  }

  async function handleSelectTeam(teamId: string) {
    if (!teamId) return;
    try {
      await invoke("clickup_select_team", { teamId });
      await refreshStatus();
    } catch (err) {
      pushToast({ message: "ClickUp", description: String(err), type: "error" });
    }
  }

  const teams = syncStatus?.teams ?? [];

  return (
    <div className="grid gap-4">
      <p className="text-xs text-muted-foreground">
        Read-only mirror of your ClickUp workspace. Paste a Personal API token
        (ClickUp → Settings → Apps) to enable the panel.
      </p>

      <div className="grid gap-1.5">
        <Label htmlFor="clickup-token">Personal API token</Label>
        <div className="flex items-center gap-2">
          <Input
            id="clickup-token"
            type="password"
            value={tokenInput}
            onChange={(e) => setTokenInput(e.target.value)}
            placeholder={configured ? "Token configured — paste to replace" : "pk_…"}
            autoComplete="off"
          />
          <Button
            variant="outline"
            size="sm"
            onClick={() => void handleSetToken()}
            disabled={busy || tokenInput.trim().length === 0}
          >
            {busy ? "Saving…" : "Set token"}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Stored in the OS keyring when available. The token never leaves this machine.
        </p>
      </div>

      {configured && (
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void handleValidate()} disabled={busy}>
            Validate token
          </Button>
          <Button variant="destructive" size="sm" onClick={() => void handleClearToken()} disabled={busy}>
            Clear token
          </Button>
        </div>
      )}

      {resolvedUser && (
        <p className="flex items-center gap-1.5 text-xs text-green-500">
          <CheckCircle2 size={13} />
          Token valid — {resolvedUser.username} ({resolvedUser.email})
        </p>
      )}
      {validationError && (
        <p className="flex items-center gap-1.5 text-xs text-red-400">
          <XCircle size={13} className="shrink-0" />
          <span className="min-w-0 truncate" title={validationError}>{validationError}</span>
        </p>
      )}

      {tokenOnDisk && (
        <p className="flex items-start gap-1.5 rounded-md border border-yellow-500/30 bg-yellow-500/10 px-2 py-1.5 text-xs text-yellow-400">
          <AlertTriangle size={13} className="mt-0.5 shrink-0" />
          No OS keyring available — the token was written to
          ~/.config/nergal/clickup.toml (mode 0600). Any process running as
          your user can read it.
        </p>
      )}

      {teams.length > 1 && (
        <div className="grid gap-1.5">
          <Label htmlFor="clickup-team">Workspace to sync</Label>
          <Select
            id="clickup-team"
            value={syncStatus?.team_id ?? ""}
            onValueChange={(v) => void handleSelectTeam(v)}
            options={teams.map((t) => ({ value: t.id, label: t.name }))}
            placeholder="Pick a workspace…"
          />
          <p className="text-xs text-muted-foreground">
            Multiple ClickUp workspaces detected — nothing syncs until one is picked.
          </p>
        </div>
      )}

      <div className="grid gap-1.5">
        <Label htmlFor="clickup-default-view">Default view</Label>
        <Select
          id="clickup-default-view"
          value={config.clickup_default_view ?? "mine"}
          onValueChange={(v) => setConfig((prev) => ({ ...prev, clickup_default_view: v }))}
          options={CLICKUP_VIEW_OPTIONS}
        />
        <p className="text-xs text-muted-foreground">
          The view the ClickUp panel opens on. Applies on next launch. Save to persist.
        </p>
      </div>

      {syncStatus && (
        <p className="text-xs text-muted-foreground">
          Status: {syncStateLabel(syncStatus)}
          {syncStatus.state === "error" && syncStatus.error ? ` — ${syncStatus.error}` : ""}
        </p>
      )}
    </div>
  );
}

function linearSyncStateLabel(s: LinearSyncStatus): string {
  switch (s.state) {
    case "idle": return "Idle";
    case "no_key": return "No API key";
    case "needs_team": return "Select teams in team picker";
    case "syncing": return s.baselineDone ? "Syncing…" : "Baseline sync in progress…";
    case "ok": return s.lastSync ? `OK — last sync ${new Date(s.lastSync * 1000).toLocaleTimeString()}` : "OK";
    case "error": return "Error";
    default: return "Idle";
  }
}

const LINEAR_VIEW_OPTIONS = [
  { value: "mine", label: "My issues" },
  { value: "state", label: "State" },
  { value: "project", label: "Project" },
  { value: "assignee", label: "Assignee" },
  { value: "cycle", label: "Cycle" },
];

function LinearSection() {
  const [syncStatus, setSyncStatus] = useAtom(linearSyncStatusAtom);
  const keyOnDisk = useAtomValue(linearKeyOnDiskAtom);
  const [config, setConfig] = useAtom(configAtom);
  const pushToast = useSetAtom(toastsAtom);
  const [keyInput, setKeyInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [workspaces, setWorkspaces] = useState<LinearWorkspace[]>([]);
  const [validationError, setValidationError] = useState<string | null>(null);
  const teams = useAtomValue(linearTeamsAtom);
  const selectedTeamIds = syncStatus?.selectedTeamIds ?? [];

  async function refreshStatus() {
    try {
      setSyncStatus(await invoke<LinearSyncStatus>("linear_sync_status"));
    } catch {
      // poller also pushes linear:sync-status; a missed pull is fine
    }
  }

  async function refreshWorkspaces() {
    try {
      setWorkspaces(await invoke<LinearWorkspace[]>("linear_list_workspaces"));
    } catch {
      // non-fatal; the list just stays empty
    }
  }

  useEffect(() => {
    void refreshWorkspaces();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handleAddWorkspace() {
    if (keyInput.trim().length === 0 || busy) return;
    setBusy(true);
    setValidationError(null);
    try {
      const ws = await invoke<LinearWorkspace>("linear_add_workspace", { key: keyInput.trim() });
      setKeyInput("");
      await refreshWorkspaces();
      await refreshStatus();
      await refreshLinearMirror(appStore);
      pushToast({ message: "Linear", description: `Workspace added: ${ws.name}`, type: "success" });
    } catch (err) {
      setValidationError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleSetActive(orgId: string) {
    if (busy) return;
    setBusy(true);
    try {
      await invoke("linear_set_active_workspace", { orgId });
      await refreshWorkspaces();
      await refreshStatus();
      await refreshLinearMirror(appStore);
    } catch (err) {
      pushToast({ message: "Linear", description: String(err), type: "error" });
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveWorkspace(orgId: string, name: string) {
    if (busy) return;
    const ok = await swalConfirm({
      title: "Remove workspace?",
      body: `<strong>${name}</strong>'s API key will be deleted. If it's the active workspace its mirror is wiped.`,
      confirmLabel: "Remove",
      kind: "warning",
      destructive: true,
    });
    if (!ok) return;
    setBusy(true);
    try {
      await invoke("linear_remove_workspace", { orgId });
      await refreshWorkspaces();
      await refreshStatus();
      await refreshLinearMirror(appStore);
    } catch (err) {
      pushToast({ message: "Linear", description: String(err), type: "error" });
    } finally {
      setBusy(false);
    }
  }

  async function handleToggleTeam(teamId: string, checked: boolean) {
    const next = checked
      ? [...selectedTeamIds.filter((id) => id !== teamId), teamId]
      : selectedTeamIds.filter((id) => id !== teamId);
    try {
      await invoke("linear_select_teams", { teamIds: next });
      await refreshStatus();
      await refreshLinearMirror(appStore);
    } catch (err) {
      pushToast({ message: "Linear", description: String(err), type: "error" });
    }
  }

  return (
    <div className="grid gap-4">
      <p className="text-xs text-muted-foreground">
        Read-only mirror of your Linear workspaces. A Personal API key (Linear →
        Settings → API) is workspace-scoped — add one per workspace, pick the
        active one to mirror.
      </p>

      {workspaces.length > 0 && (
        <div className="grid gap-1.5">
          <Label>Workspaces</Label>
          <div className="flex flex-col gap-1">
            {workspaces.map((ws) => (
              <div key={ws.orgId} className="flex items-center gap-2 text-xs">
                <label className="flex flex-1 items-center gap-2">
                  <input
                    type="radio"
                    name="linear-active-workspace"
                    checked={ws.active}
                    onChange={() => void handleSetActive(ws.orgId)}
                    disabled={busy}
                    className="border border-border"
                  />
                  <span className={ws.active ? "font-medium text-foreground" : "text-foreground/80"}>
                    {ws.name}
                  </span>
                  {ws.active && <span className="text-[10px] text-primary">active</span>}
                </label>
                <Button
                  variant="ghost"
                  size="xs"
                  onClick={() => void handleRemoveWorkspace(ws.orgId, ws.name)}
                  disabled={busy}
                >
                  <XCircle size={13} />
                </Button>
              </div>
            ))}
          </div>
          <p className="text-xs text-muted-foreground">
            Switching the active workspace wipes the mirror and re-syncs.
          </p>
        </div>
      )}

      <div className="grid gap-1.5">
        <Label htmlFor="linear-key">Add workspace</Label>
        <div className="flex items-center gap-2">
          <Input
            id="linear-key"
            type="password"
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            placeholder="lin_api_…"
            autoComplete="off"
          />
          <Button
            variant="outline"
            size="sm"
            onClick={() => void handleAddWorkspace()}
            disabled={busy || keyInput.trim().length === 0}
          >
            {busy ? "Adding…" : "Add"}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          The key is validated, its workspace resolved, and stored in the OS
          keyring when available. The key never leaves this machine.
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="linear-default-view">Default view</Label>
        <Select
          id="linear-default-view"
          value={config.linear_default_view ?? "mine"}
          onValueChange={(v) => setConfig((prev) => ({ ...prev, linear_default_view: v }))}
          options={LINEAR_VIEW_OPTIONS}
        />
        <p className="text-xs text-muted-foreground">
          The view the Linear panel opens on. Applies on next launch. Save to persist.
        </p>
      </div>

      {validationError && (
        <p className="flex items-center gap-1.5 text-xs text-red-400">
          <XCircle size={13} className="shrink-0" />
          <span className="min-w-0 truncate" title={validationError}>{validationError}</span>
        </p>
      )}

      {keyOnDisk && (
        <p className="flex items-start gap-1.5 rounded-md border border-yellow-500/30 bg-yellow-500/10 px-2 py-1.5 text-xs text-yellow-400">
          <AlertTriangle size={13} className="mt-0.5 shrink-0" />
          No OS keyring available — keys were written to
          ~/.config/nergal/linear-*.toml (mode 0600). Any process running as
          your user can read them.
        </p>
      )}

      {teams.length > 0 && (
        <div className="grid gap-1.5">
          <Label>Teams to sync</Label>
          <div className="flex flex-col gap-1.5">
            {teams.map((team) => (
              <label key={team.id} className="flex items-center gap-2 text-xs">
                <input
                  type="checkbox"
                  checked={selectedTeamIds.includes(team.id)}
                  onChange={(e) => void handleToggleTeam(team.id, e.target.checked)}
                  className="rounded border border-border"
                />
                <span className="font-mono text-foreground/70">{team.key}</span>
                <span>{team.name}</span>
              </label>
            ))}
          </div>
          <p className="text-xs text-muted-foreground">
            Only issues from selected teams are mirrored.
          </p>
        </div>
      )}

      {syncStatus && (
        <p className="text-xs text-muted-foreground">
          Status: {linearSyncStateLabel(syncStatus)}
          {syncStatus.state === "error" && syncStatus.error ? ` — ${syncStatus.error}` : ""}
        </p>
      )}
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

// `-?` strips the optional modifier so optional Config fields (e.g. the
// default-view selects) don't leak `undefined` into the key union.
type StringConfigKey = {
  [K in keyof Config]-?: Config[K] extends string ? K : never;
}[keyof Config];

type BooleanConfigKey = {
  [K in keyof Config]-?: Config[K] extends boolean ? K : never;
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
  isCustom,
  onSelect,
  onCustomize,
  onEdit,
  onDelete,
}: {
  theme: Theme;
  selected: boolean;
  isCustom?: boolean;
  onSelect: () => void;
  onCustomize?: () => void;
  onEdit?: () => void;
  onDelete?: () => void;
}) {
  const { preview } = theme;
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      data-theme-card
      data-theme-id={theme.id}
      className={`group relative flex flex-col gap-2 rounded-md border p-2 text-left outline-none transition-colors focus:ring-2 focus:ring-primary focus:ring-offset-2 focus:ring-offset-background focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2 focus-visible:ring-offset-background ${
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
        <span className="text-xs font-medium text-foreground truncate">{theme.label}</span>
        {selected && (
          <span className="flex size-4 shrink-0 items-center justify-center rounded-full bg-orange-500 text-white">
            <Check size={10} strokeWidth={3} />
          </span>
        )}
      </div>
      {/* Font sample row: each "Aa" uses one of the theme's fonts so the
          card surfaces the typographic personality (interface / markdown /
          terminal) at a glance. */}
      <div className="flex items-center justify-between px-0.5">
        <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
          <span style={{ fontFamily: theme.fonts.interface }} title="Interface font">Aa</span>
          <span style={{ fontFamily: theme.fonts.markdown }} title="Markdown font">Aa</span>
          <span style={{ fontFamily: theme.fonts.terminal }} title="Terminal font">Aa</span>
        </div>
        <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus:opacity-100 group-focus-within:opacity-100">
          {/* Keyboard hints: "E" enters the editor for the focused card;
              "D" deletes a custom (only for customs). Visible only on
              keyboard focus so the chips don't compete with the icons
              shown on hover. */}
          <span className="hidden rounded border border-border/60 bg-secondary px-1 font-mono text-[9px] text-muted-foreground group-focus:inline-block group-focus-within:inline-block">
            E
          </span>
          {isCustom && (
            <span className="hidden rounded border border-border/60 bg-secondary px-1 font-mono text-[9px] text-muted-foreground group-focus:inline-block group-focus-within:inline-block">
              D
            </span>
          )}
          {isCustom ? (
            <>
              {onEdit && (
                <span
                  role="button"
                  tabIndex={0}
                  aria-label="Edit custom theme"
                  onClick={(e) => { e.stopPropagation(); onEdit(); }}
                  onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); e.preventDefault(); onEdit(); } }}
                  className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
                >
                  <Sliders size={11} />
                </span>
              )}
              {onDelete && (
                <span
                  role="button"
                  tabIndex={0}
                  aria-label="Delete custom theme"
                  onClick={(e) => { e.stopPropagation(); onDelete(); }}
                  onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); e.preventDefault(); onDelete(); } }}
                  className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-destructive"
                >
                  <Trash2 size={11} />
                </span>
              )}
            </>
          ) : (
            onCustomize && (
              <Tooltip>
                <TooltipTrigger
                  render={
                    <span
                      role="button"
                      tabIndex={0}
                      aria-label="Customize theme"
                      onClick={(e) => { e.stopPropagation(); onCustomize(); }}
                      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); e.preventDefault(); onCustomize(); } }}
                      className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
                    />
                  }
                >
                  <Sliders size={11} />
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">Customize</TooltipContent>
              </Tooltip>
            )
          )}
        </div>
      </div>
    </button>
  );
}

function ThemeEditor({
  custom,
  onChange,
  onClose,
  onDelete,
}: {
  custom: CustomTheme;
  onChange: (next: CustomTheme) => void;
  onClose: () => void;
  onDelete: () => void;
}) {
  const editorRef = useRef<HTMLDivElement>(null);
  const interfaceFontId =
    INTERFACE_FONTS.find((f) => f.stack === custom.fonts.interface)?.id ?? "geist";
  const terminalFontId =
    TERMINAL_FONTS.find((f) => f.stack === custom.fonts.terminal)?.id ?? "jetbrains-mono";
  const markdownFontId =
    MARKDOWN_FONTS.find((f) => f.stack === custom.fonts.markdown)?.id ?? "geist";

  // Land focus on the Back button so arrow keys navigate from the top.
  useEffect(() => {
    const root = editorRef.current;
    if (!root) return;
    const back = root.querySelector<HTMLElement>("button");
    requestAnimationFrame(() => back?.focus({ preventScroll: true }));
  }, []);

  // Esc returns to the grid. Window-level capture phase + targeted
  // popup-state detection so an open Select popup absorbs Esc to close
  // itself first (without leaking to the parent Dialog and closing the
  // whole modal). Esc only — ArrowLeft would conflict with swatch
  // navigation, so it's reserved exclusively for left/right movement.
  useEffect(() => {
    function findOpenSelectTrigger(): HTMLElement | null {
      return document.querySelector<HTMLElement>(
        '[aria-haspopup="listbox"][aria-expanded="true"]',
      );
    }
    function onKeyDown(e: KeyboardEvent) {
      const isEsc = e.key === "Escape";
      const isBackspace = e.key === "Backspace";
      if (!isEsc && !isBackspace) return;
      const trigger = findOpenSelectTrigger();
      if (trigger) {
        // Popup open — close it ourselves by re-clicking the trigger
        // (Base UI Select toggles open state on click). stopImmediate
        // also blocks the parent Dialog from closing the whole modal.
        e.preventDefault();
        e.stopImmediatePropagation();
        trigger.click();
        trigger.focus({ preventScroll: true });
        return;
      }
      // Backspace must not steal the delete-char behavior from text
      // inputs. Esc has no input-side meaning and always exits.
      if (isBackspace) {
        const target = e.target as HTMLElement | null;
        const inText =
          target?.tagName === "INPUT" ||
          target?.tagName === "TEXTAREA" ||
          target?.getAttribute("contenteditable") === "true";
        if (inText) return;
      }
      e.preventDefault();
      e.stopImmediatePropagation();
      onClose();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onClose]);

  // Arrow Left/Right within the accent swatches row — common keyboard
  // pattern for color pickers. ArrowDown/Up is handled by the parent's
  // linear-focus handler so it walks across all editor rows uniformly.
  function onSwatchKeyDown(e: React.KeyboardEvent<HTMLButtonElement>) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    const root = editorRef.current;
    if (!root) return;
    const swatches = Array.from(
      root.querySelectorAll<HTMLButtonElement>("button[data-accent-swatch]"),
    );
    const current = e.currentTarget;
    const idx = swatches.indexOf(current);
    if (idx === -1) return;
    e.preventDefault();
    e.stopPropagation();
    const nextIdx = e.key === "ArrowRight"
      ? Math.min(idx + 1, swatches.length - 1)
      : Math.max(idx - 1, 0);
    swatches[nextIdx].focus();
  }

  function patch(fields: Partial<CustomTheme>) {
    onChange({ ...custom, ...fields });
  }

  function patchFont(slot: "interface" | "terminal" | "markdown", id: string) {
    const list =
      slot === "interface" ? INTERFACE_FONTS : slot === "terminal" ? TERMINAL_FONTS : MARKDOWN_FONTS;
    const opt = list.find((f) => f.id === id);
    if (!opt) return;
    onChange({ ...custom, fonts: { ...custom.fonts, [slot]: opt.stack } });
  }

  return (
    <div className="space-y-4" data-theme-editor ref={editorRef}>
      <div className="flex items-center gap-2">
        <Tooltip>
          <TooltipTrigger render={<Button variant="ghost" size="sm" onClick={onClose} />}>
            <ArrowLeft size={14} />
            Back
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">Back (Esc)</TooltipContent>
        </Tooltip>
        <Input
          value={custom.label}
          onChange={(e) => patch({ label: e.target.value })}
          className="h-7 max-w-xs text-[12px]"
          aria-label="Theme name"
        />
      </div>

      <section className="grid gap-2">
        <Label>Accent</Label>
        <div className="flex flex-wrap items-center gap-2 px-2">
          {ACCENT_PRESETS.map((p) => {
            const active = p.value.toLowerCase() === custom.primary.toLowerCase();
            return (
              <button
                key={p.value}
                type="button"
                onClick={() => patch({ primary: p.value })}
                onKeyDown={onSwatchKeyDown}
                aria-label={p.label}
                title={p.label}
                data-accent-swatch
                className={`nergal-focus-ring size-6 rounded-full border-2 transition-transform ${active ? "scale-110 border-foreground" : "border-border hover:scale-105"}`}
                style={{ backgroundColor: p.value }}
              />
            );
          })}
          <span className="ml-2 flex items-center gap-2">
            <Input
              value={custom.primary}
              onChange={(e) => patch({ primary: e.target.value })}
              className="h-7 w-28 font-mono text-[11px]"
              aria-label="Accent hex"
              placeholder="#22d3ee"
            />
            <span
              className="size-6 shrink-0 rounded-full border border-border"
              style={{ backgroundColor: custom.primary }}
              aria-label={`Current accent: ${custom.primary}`}
              title={custom.primary}
            />
          </span>
        </div>
        {/* Full color picker pad — saturation/value square + hue slider.
            react-colorful is keyboard-accessible (Tab into pad, arrows
            move within it). The wrapper class scopes our local sizing
            overrides. */}
        <div className="nergal-color-picker mt-2 px-2">
          <HexColorPicker
            color={normalizeHexForPicker(custom.primary)}
            onChange={(hex) => patch({ primary: hex })}
          />
        </div>
      </section>

      <section className="grid gap-3">
        <Label>Fonts</Label>
        <FontSelectRow
          label="Interface"
          options={INTERFACE_FONTS}
          valueId={interfaceFontId}
          stack={custom.fonts.interface}
          onChange={(id) => patchFont("interface", id)}
        />
        <FontSelectRow
          label="Terminal"
          options={TERMINAL_FONTS}
          valueId={terminalFontId}
          stack={custom.fonts.terminal}
          onChange={(id) => patchFont("terminal", id)}
        />
        <FontSelectRow
          label="Markdown"
          options={MARKDOWN_FONTS}
          valueId={markdownFontId}
          stack={custom.fonts.markdown}
          onChange={(id) => patchFont("markdown", id)}
        />
      </section>

      <div className="flex items-center justify-between border-t border-border/50 pt-3">
        <span className="text-xs text-muted-foreground">
          Changes apply live. Click <strong>Save</strong> to persist across sessions.
        </span>
        <Button variant="ghost" size="sm" onClick={onDelete} className="text-destructive hover:text-destructive">
          <Trash2 size={12} />
          Delete theme
        </Button>
      </div>
    </div>
  );
}

/// `<input type="color">` only accepts strict 6-digit hex. Strip alpha and
/// normalize 3-digit shorthand so a CSS color like "#abc" or "#22d3eecc"
/// still yields a valid swatch instead of an empty picker. Returns the
/// canonical 6-digit form for the swatch; the source `primary` value (which
/// may include alpha) is preserved untouched in the actual config.
function normalizeHexForPicker(value: string): string {
  const trimmed = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) return trimmed.toLowerCase();
  if (/^#[0-9a-fA-F]{3}$/.test(trimmed)) {
    const r = trimmed[1], g = trimmed[2], b = trimmed[3];
    return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
  }
  if (/^#[0-9a-fA-F]{8}$/.test(trimmed)) return trimmed.slice(0, 7).toLowerCase();
  return "#000000";
}

function FontSelectRow({
  label,
  options,
  valueId,
  stack,
  onChange,
}: {
  label: string;
  options: FontOption[];
  valueId: string;
  stack: string;
  onChange: (id: string) => void;
}) {
  const selectOptions = options.map((o) => ({ value: o.id, label: o.label }));
  return (
    <div className="grid grid-cols-[110px_minmax(0,1fr)_60px] items-center gap-2 text-sm">
      <span className="text-xs text-muted-foreground">{label}</span>
      <Select
        value={valueId}
        onValueChange={onChange}
        options={selectOptions}
        className="h-8"
      />
      <span
        className="text-[14px] text-muted-foreground"
        style={{ fontFamily: stack }}
        title={stack}
      >
        Aa Bb
      </span>
    </div>
  );
}


function AppearanceSection({
  config,
  setConfig,
  handleTextChange,
  handleToggleChange,
}: {
  config: Config;
  setConfig: (updater: (prev: Config) => Config) => void;
  handleTextChange: (key: StringConfigKey, value: string) => void;
  handleToggleChange: (key: BooleanConfigKey, value: boolean) => void;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [returnFocusId, setReturnFocusId] = useState<string | null>(null);
  /// Held out of `config.custom_themes` so the picker doesn't sprout empty
  /// customs on every editor open + close with no edits.
  const [draftCustom, setDraftCustom] = useState<CustomTheme | null>(null);

  const customs = config.custom_themes;
  const editingCustom = editingId
    ? customs.find((c) => c.id === editingId)
      ?? (draftCustom?.id === editingId ? draftCustom : null)
    : null;

  const builtinCards = VISIBLE_THEMES;
  const customCards = customs.map((c) => ({
    custom: c,
    theme: resolveCustomTheme(c),
  }));
  // normalizeThemeId only knows builtins, so it collapses a custom id to the
  // default — which sent the "active" check to the default card while a custom
  // theme was in use. Preserve a known custom id; normalize everything else.
  const activeId = customs.some((c) => c.id === config.theme_mode)
    ? config.theme_mode
    : normalizeThemeId(config.theme_mode);

  function selectTheme(id: string) {
    handleTextChange("theme_mode", id);
  }

  function customizeBuiltin(baseId: string) {
    const fork = forkBuiltinTheme(baseId, customs);
    setDraftCustom(fork);
    setReturnFocusId(fork.id);
    setEditingId(fork.id);
  }

  function editCustom(customId: string) {
    selectTheme(customId);
    setReturnFocusId(customId);
    setEditingId(customId);
  }

  function updateCustom(next: CustomTheme) {
    setConfig((prev) => {
      const exists = prev.custom_themes.some((c) => c.id === next.id);
      if (exists) {
        return {
          ...prev,
          custom_themes: prev.custom_themes.map((c) => (c.id === next.id ? next : c)),
        };
      }
      return {
        ...prev,
        custom_themes: [...prev.custom_themes, next],
        theme_mode: next.id,
      };
    });
    setDraftCustom(null);
  }

  async function deleteCustom(customId: string, prompt = true) {
    if (prompt) {
      const target = customs.find((c) => c.id === customId);
      const ok = await swalConfirm({
        title: "Delete custom theme?",
        body: `<strong>${target?.label ?? "Untitled"}</strong> will be removed permanently. This cannot be undone.`,
        confirmLabel: "Delete",
        cancelLabel: "Cancel",
        kind: "warning",
        destructive: true,
      });
      if (!ok) return;
    }
    setConfig((prev) => {
      const removed = prev.custom_themes.find((c) => c.id === customId);
      const next: Config = {
        ...prev,
        custom_themes: prev.custom_themes.filter((c) => c.id !== customId),
      };
      // If the deleted custom was the active theme, fall back to its base
      // so the picker doesn't render an unselected state.
      if (prev.theme_mode === customId && removed) {
        next.theme_mode = removed.base_id;
      }
      return next;
    });
    if (editingId === customId) setEditingId(null);
    // Land focus on the first theme card so keyboard nav stays alive
    // after the deleted card unmounts (otherwise focus falls to body).
    requestAnimationFrame(() => {
      const first = document.querySelector<HTMLElement>("[data-theme-card]");
      first?.focus({ preventScroll: true });
    });
  }

  // Keyboard-first: while a theme card is focused, "E" opens the editor
  // (fork-then-edit for builtins, edit in place for customs) and "D"
  // deletes the focused custom (with confirm). Both keys are surfaced as
  // <kbd> chips in the card's hover/focus state.
  useEffect(() => {
    if (editingId) return;
    function onKeyDown(e: KeyboardEvent) {
      const isE = e.key === "e" || e.key === "E";
      const isD = e.key === "d" || e.key === "D";
      if (!isE && !isD) return;
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      const target = document.activeElement as HTMLElement | null;
      const card = target?.closest<HTMLElement>("[data-theme-card]");
      if (!card) return;
      const id = card.dataset.themeId;
      if (!id) return;
      const isCustom = customs.some((c) => c.id === id);
      if (isE) {
        e.preventDefault();
        e.stopPropagation();
        if (isCustom) editCustom(id);
        else customizeBuiltin(id);
      } else if (isD && isCustom) {
        e.preventDefault();
        e.stopPropagation();
        void deleteCustom(id);
      }
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editingId, customs, config.theme_mode]);

  // After exiting the editor, return focus to the matching theme card so
  // the keyboard flow doesn't strand the user at body.
  useEffect(() => {
    if (editingId) return;
    if (!returnFocusId) return;
    const id = returnFocusId;
    setReturnFocusId(null);
    requestAnimationFrame(() => {
      const card = document.querySelector<HTMLElement>(
        `[data-theme-card][data-theme-id="${CSS.escape(id)}"]`,
      );
      card?.focus({ preventScroll: true });
    });
  }, [editingId, returnFocusId]);

  if (editingCustom) {
    const isDraft = draftCustom?.id === editingCustom.id;
    return (
      <ThemeEditor
        custom={editingCustom}
        onChange={updateCustom}
        onClose={() => {
          setEditingId(null);
          if (isDraft) setDraftCustom(null);
        }}
        onDelete={() => {
          if (isDraft) {
            setDraftCustom(null);
            setEditingId(null);
          } else {
            void deleteCustom(editingCustom.id);
          }
        }}
      />
    );
  }

  return (
    <div className="space-y-4">
      <div className="grid gap-2">
        <Label>Skin</Label>
        <div className="grid grid-cols-2 gap-2">
          {builtinCards.map((theme) => (
            <ThemePreviewCard
              key={theme.id}
              theme={theme}
              selected={activeId === theme.id}
              onSelect={() => selectTheme(theme.id)}
              onCustomize={() => customizeBuiltin(theme.id)}
            />
          ))}
        </div>

        {customCards.length > 0 && (
          <>
            <Label className="mt-2">Custom</Label>
            <div className="grid grid-cols-2 gap-2">
              {customCards.map(({ custom, theme }) => (
                <ThemePreviewCard
                  key={theme.id}
                  theme={theme}
                  selected={activeId === theme.id}
                  isCustom
                  onSelect={() => selectTheme(theme.id)}
                  onEdit={() => editCustom(custom.id)}
                  onDelete={() => deleteCustom(custom.id)}
                />
              ))}
            </div>
          </>
        )}

        <p className="text-xs text-muted-foreground">
          <kbd className="rounded border border-border/60 bg-secondary px-1 font-mono text-[10px]">E</kbd> on a focused theme to fork & edit, or click <Sliders size={11} className="inline align-text-bottom" />.
          Custom themes inherit surfaces from the base and override accent + fonts.
        </p>
      </div>

      <div className="flex items-start gap-3 text-sm">
        <Switch
          id="setting-sidebar_dot_grid"
          checked={config.sidebar_dot_grid}
          onCheckedChange={(v) => handleToggleChange("sidebar_dot_grid", v)}
          aria-label="Sidebar dot grid"
          className="mt-0.5"
        />
        <label htmlFor="setting-sidebar_dot_grid" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">Sidebar dot grid</span>
          <span className="text-xs text-muted-foreground">Radial dot texture behind the left sidebar islands.</span>
        </label>
      </div>

      <div className="flex items-start gap-3 text-sm">
        <Switch
          id="setting-panel_focus_pulse"
          checked={config.panel_focus_pulse}
          onCheckedChange={(v) => handleToggleChange("panel_focus_pulse", v)}
          aria-label="Pulse focus border"
          className="mt-0.5"
        />
        <label htmlFor="setting-panel_focus_pulse" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">Pulse focus border</span>
          <span className="text-xs text-muted-foreground">Brief accent flash on focus change instead of a permanent accent border (legacy v3-ux behavior).</span>
        </label>
      </div>

      <div className="flex items-start gap-3 text-sm">
        <Switch
          id="setting-panel_glow"
          checked={config.panel_glow}
          onCheckedChange={(v) => handleToggleChange("panel_glow", v)}
          aria-label="Panel glow"
          className="mt-0.5"
        />
        <label htmlFor="setting-panel_glow" className="flex flex-col gap-0.5 cursor-pointer">
          <span className="font-medium">Panel glow</span>
          <span className="text-xs text-muted-foreground">Wrap the active panel border with a soft accent-color halo (Hyprland-style).</span>
        </label>
      </div>
    </div>
  );
}

type SectionId = "paths" | "agents" | "editor" | "appearance" | "terminal" | "keymap" | "mcp" | "scratchpad" | "obsidian" | "clickup" | "linear" | "about";

const SECTIONS: { id: SectionId; label: string; icon: typeof FolderTree }[] = [
  { id: "paths", label: "Paths", icon: FolderTree },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "editor", label: "Editor", icon: Pencil },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "terminal", label: "Terminal", icon: Terminal },
  { id: "keymap", label: "Keymap", icon: Keyboard },
  { id: "mcp", label: "MCP", icon: Network },
  { id: "scratchpad", label: "Scratchpad", icon: NotebookText },
  { id: "obsidian", label: "Obsidian", icon: ObsidianIcon },
  { id: "clickup", label: "ClickUp", icon: ClickUpIcon },
  { id: "linear", label: "Linear", icon: LinearIcon },
  { id: "about", label: "About", icon: Info },
];

type InstallSource = "deb" | "appimage" | "dev" | "unknown";

interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  releaseUrl: string;
  releaseNotes: string | null;
  debAssetUrl: string | null;
  debAssetSize: number | null;
  appimageAssetUrl: string | null;
  appimageAssetSize: number | null;
}

interface CurrentReleaseInfo {
  version: string;
  notes: string | null;
  releaseUrl: string;
}

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up_to_date"; latest: string }
  | { kind: "available"; result: UpdateCheckResult }
  | { kind: "downloading" }
  | { kind: "downloaded"; path: string }
  | { kind: "appimage_downloading"; downloaded: number; total: number | null }
  | { kind: "appimage_installed" }
  | { kind: "error"; message: string };

function debFilename(result: UpdateCheckResult): string {
  return result.debAssetUrl?.split("/").pop() ?? `Nergal_${result.latestVersion}_amd64.deb`;
}

function AboutSection({ appVersion }: { appVersion: string }) {
  const [installSource, setInstallSource] = useState<InstallSource>("unknown");
  const [updateState, setUpdateState] = useState<UpdateState>({ kind: "idle" });
  const [currentRelease, setCurrentRelease] = useState<CurrentReleaseInfo | null>(null);
  const [missingBinaries, setMissingBinaries] = useState<string[]>([]);
  const pushToast = useSetAtom(toastsAtom);

  useEffect(() => {
    invoke<InstallSource>("get_install_source")
      .then(setInstallSource)
      .catch(() => setInstallSource("unknown"));
    invoke<CurrentReleaseInfo | null>("get_current_release_notes")
      .then(setCurrentRelease)
      .catch(() => setCurrentRelease(null));
    invoke<{ missingBinaries: string[] }>("check_system_health")
      .then((health) => setMissingBinaries(health.missingBinaries))
      .catch(() => setMissingBinaries([]));
  }, []);

  async function handleCheck() {
    setUpdateState({ kind: "checking" });
    try {
      const result = await invoke<UpdateCheckResult>("check_app_update");
      if (!result.hasUpdate) {
        setUpdateState({ kind: "up_to_date", latest: result.latestVersion });
        return;
      }
      // A previous visit may have already staged the asset — land on
      // "downloaded" instead of offering a redundant re-download.
      if (installSource === "deb" && result.debAssetUrl) {
        const staged = await invoke<string | null>("find_downloaded_update", {
          filename: debFilename(result),
          expectedSize: result.debAssetSize,
        }).catch(() => null);
        if (staged) {
          setUpdateState({ kind: "downloaded", path: staged });
          return;
        }
      }
      setUpdateState({ kind: "available", result });
    } catch (err) {
      setUpdateState({ kind: "error", message: String(err) });
    }
  }

  async function handleDownloadDeb(result: UpdateCheckResult) {
    if (!result.debAssetUrl) return;
    setUpdateState({ kind: "downloading" });
    try {
      const path = await invoke<string>("download_app_update", {
        url: result.debAssetUrl,
        filename: debFilename(result),
        expectedSize: result.debAssetSize,
      });
      setUpdateState({ kind: "downloaded", path });
    } catch (err) {
      setUpdateState({ kind: "error", message: String(err) });
    }
  }

  async function handleAppImageUpdate() {
    setUpdateState({ kind: "appimage_downloading", downloaded: 0, total: null });
    try {
      const update = await checkUpdater();
      if (!update) {
        setUpdateState({
          kind: "error",
          message: "Updater plugin reported no update. The latest.json manifest may not be published yet.",
        });
        return;
      }
      let totalBytes: number | null = null;
      let received = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          totalBytes = event.data.contentLength ?? null;
          setUpdateState({ kind: "appimage_downloading", downloaded: 0, total: totalBytes });
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          setUpdateState({ kind: "appimage_downloading", downloaded: received, total: totalBytes });
        } else if (event.event === "Finished") {
          setUpdateState({ kind: "appimage_installed" });
        }
      });
    } catch (err) {
      setUpdateState({ kind: "error", message: String(err) });
    }
  }

  async function handleRelaunch() {
    try {
      await relaunch();
    } catch (err) {
      console.error("[about] relaunch failed:", err);
    }
  }

  async function handleReveal(path: string) {
    try {
      await invoke("reveal_in_downloads", { path });
    } catch (err) {
      pushToast({ message: "Could not reveal the download", description: String(err), type: "error" });
    }
  }

  function handleOpenReleaseUrl(url: string) {
    openShell(url).catch((err) => console.error("[about] open url failed:", err));
  }

  async function handleCopyDiagnostics() {
    try {
      const bundle = await invoke<string>("collect_diagnostics");
      await invoke("terminal_clipboard_write", { text: bundle });
      pushToast({ message: "Diagnostics copied", description: "Paste into your bug report's Logs section.", type: "success" });
    } catch (e) {
      pushToast({ message: "Copy failed", description: String(e), type: "error" });
    }
  }

  function handleReportIssue() {
    const body = [
      "**Describe the bug**",
      "",
      "",
      "**Steps to reproduce**",
      "1. ",
      "",
      "**Environment**",
      `- Nergal: v${appVersion}`,
      `- Install source: ${sourceLabel}`,
      "",
      "**Logs**",
      "<!-- Click \"Copy diagnostics\" in Settings › About, then paste here -->",
      "",
    ].join("\n");
    const url = `https://github.com/Mufdi/nergal/issues/new?body=${encodeURIComponent(body)}`;
    openShell(url).catch((err) => console.error("[about] report issue failed:", err));
  }

  const sourceLabel = {
    deb: ".deb (system install)",
    appimage: "AppImage (portable)",
    dev: "Dev build",
    unknown: "Unknown",
  }[installSource];

  return (
    <div className="space-y-5">
      {missingBinaries.length > 0 && (
        <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs text-amber-500">
          <AlertTriangle size={12} className="mt-0.5 shrink-0" />
          <span>
            Missing system tools:{" "}
            <code className="rounded bg-amber-500/20 px-1 font-mono text-[10px]">
              {missingBinaries.join(", ")}
            </code>
            . Some features (git panels, file reveal, updates) won't work. Install them with{" "}
            <code className="rounded bg-amber-500/20 px-1 font-mono text-[10px]">
              sudo apt install {missingBinaries.map((b) => (b.startsWith("xdg-") ? "xdg-utils" : b)).filter((v, i, a) => a.indexOf(v) === i).join(" ")}
            </code>
            .
          </span>
        </div>
      )}
      <div className="grid gap-1">
        <Label>Application</Label>
        <div className="rounded-md border border-border/40 bg-card/50 p-3 text-sm">
          <div className="flex items-baseline justify-between">
            <span className="text-base font-semibold">Nergal</span>
            <span className="font-mono text-xs text-muted-foreground tabular-nums">
              v{appVersion || "—"}
            </span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">{sourceLabel}</p>
        </div>
      </div>

      {currentRelease?.notes && (
        <div className="grid gap-2">
          <Label>What's new in v{currentRelease.version}</Label>
          <div className="rounded-md border border-border/40 bg-card/30 p-3">
            <ReleaseNotesMarkdown content={currentRelease.notes} />
          </div>
          <button
            type="button"
            onClick={() => handleOpenReleaseUrl(currentRelease.releaseUrl)}
            className="inline-flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
          >
            <ExternalLink size={11} />
            Open on GitHub
          </button>
        </div>
      )}

      <div className="grid gap-2">
        <Label>Updates</Label>
        <UpdateActions
          installSource={installSource}
          state={updateState}
          onCheck={handleCheck}
          onDownloadDeb={handleDownloadDeb}
          onReveal={handleReveal}
          onOpenRelease={handleOpenReleaseUrl}
          onUpdateAppImage={handleAppImageUpdate}
          onRelaunch={handleRelaunch}
        />
      </div>

      <div className="grid gap-2">
        <Label>Diagnostics</Label>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={handleReportIssue}
            className="inline-flex w-fit items-center gap-1.5 rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            <Bug size={12} /> Report an issue
          </button>
          <button
            type="button"
            onClick={handleCopyDiagnostics}
            className="inline-flex w-fit items-center gap-1.5 rounded-md border border-border/40 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          >
            <ClipboardCopy size={12} /> Copy diagnostics
          </button>
          <button
            type="button"
            onClick={async () => {
              try {
                await invoke("open_log_file");
              } catch (e) {
                pushToast({ message: "No log file", description: String(e), type: "info" });
              }
            }}
            className="inline-flex w-fit items-center gap-1.5 rounded-md border border-border/40 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          >
            <NotebookText size={12} /> Open log file
          </button>
        </div>
        <p className="text-[10px] text-muted-foreground/60">
          Report an issue opens a prefilled GitHub issue — paste the copied diagnostics into its Logs section. The log file is only written when Nergal is launched from the app launcher (not a terminal/dev run).
        </p>
      </div>

      <div className="grid gap-1 text-xs text-muted-foreground">
        <button
          type="button"
          onClick={() => handleOpenReleaseUrl("https://github.com/Mufdi/nergal")}
          className="inline-flex w-fit items-center gap-1 text-left hover:text-foreground"
        >
          <ExternalLink size={11} />
          github.com/Mufdi/nergal
        </button>
        <button
          type="button"
          onClick={() => handleOpenReleaseUrl("https://github.com/Mufdi/nergal/releases")}
          className="inline-flex w-fit items-center gap-1 text-left hover:text-foreground"
        >
          <ExternalLink size={11} />
          Release history
        </button>
      </div>
    </div>
  );
}

function ReleaseNotesMarkdown({ content }: { content: string }) {
  return (
    <div className="prose-invert max-w-none text-[11px]">
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: (props) => (
            <a
              {...props}
              target="_blank"
              rel="noreferrer noopener"
              className="text-primary hover:underline"
            />
          ),
          h1: (props) => <h2 className="mb-1 mt-2 text-xs font-semibold" {...props} />,
          h2: (props) => <h3 className="mb-1 mt-2 text-xs font-semibold" {...props} />,
          h3: (props) => <h4 className="mb-1 mt-2 text-[11px] font-semibold" {...props} />,
          p: (props) => <p className="mb-1 leading-snug" {...props} />,
          ul: (props) => <ul className="mb-1 ml-4 list-disc" {...props} />,
          ol: (props) => <ol className="mb-1 ml-4 list-decimal" {...props} />,
          li: (props) => <li className="mb-0.5" {...props} />,
          code: (props) => (
            <code className="rounded bg-muted px-1 font-mono text-[10px]" {...props} />
          ),
          pre: (props) => (
            <pre className="my-1 overflow-x-auto rounded bg-muted p-2 font-mono text-[10px]" {...props} />
          ),
        }}
      >
        {content}
      </Markdown>
    </div>
  );
}

function UpdateActions({
  installSource,
  state,
  onCheck,
  onDownloadDeb,
  onReveal,
  onOpenRelease,
  onUpdateAppImage,
  onRelaunch,
}: {
  installSource: InstallSource;
  state: UpdateState;
  onCheck: () => void;
  onDownloadDeb: (result: UpdateCheckResult) => void;
  onReveal: (path: string) => void;
  onOpenRelease: (url: string) => void;
  onUpdateAppImage: () => void;
  onRelaunch: () => void;
}) {
  const button = renderUpdateButton({
    installSource,
    state,
    onCheck,
    onDownloadDeb,
    onReveal,
    onUpdateAppImage,
    onRelaunch,
  });

  const supplementary = renderUpdateSupplementary({
    installSource,
    state,
    onOpenRelease,
  });

  return (
    <div className="grid gap-3">
      {installSource === "dev" && (
        <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs text-amber-500">
          <AlertTriangle size={12} className="mt-0.5 shrink-0" />
          <span>
            Dev build detected. The check runs against GitHub Releases, but
            installing the new bundle is disabled — pull{" "}
            <code className="rounded bg-amber-500/20 px-1 font-mono text-[10px]">main</code> and
            rebuild instead.
          </span>
        </div>
      )}
      {button}
      {supplementary}
    </div>
  );
}

function renderUpdateButton({
  installSource,
  state,
  onCheck,
  onDownloadDeb,
  onReveal,
  onUpdateAppImage,
  onRelaunch,
}: {
  installSource: InstallSource;
  state: UpdateState;
  onCheck: () => void;
  onDownloadDeb: (result: UpdateCheckResult) => void;
  onReveal: (path: string) => void;
  onUpdateAppImage: () => void;
  onRelaunch: () => void;
}) {
  switch (state.kind) {
    case "idle":
      return (
        <Button variant="default" size="sm" onClick={onCheck} className="w-fit">
          <RefreshCw size={12} />
          Update
        </Button>
      );
    case "checking":
      return (
        <Button variant="default" size="sm" disabled className="w-fit">
          <RefreshCw size={12} className="animate-spin" />
          Checking…
        </Button>
      );
    case "up_to_date":
      return (
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                variant="outline"
                size="sm"
                onClick={onCheck}
                className="w-fit text-emerald-500"
              />
            }
          >
            <CheckCircle2 size={12} />
            Up to date (v{state.latest})
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">Click to check again</TooltipContent>
        </Tooltip>
      );
    case "available": {
      const { result } = state;
      if (installSource === "deb" && result.debAssetUrl) {
        const debSize = result.debAssetSize ? formatBytes(result.debAssetSize) : null;
        return (
          <Button
            variant="default"
            size="sm"
            onClick={() => onDownloadDeb(result)}
            className="w-fit"
          >
            <Download size={12} />
            Download v{result.latestVersion}
            {debSize ? ` (${debSize})` : ""}
          </Button>
        );
      }
      if (installSource === "appimage") {
        const size = result.appimageAssetSize ? formatBytes(result.appimageAssetSize) : null;
        return (
          <Button variant="default" size="sm" onClick={onUpdateAppImage} className="w-fit">
            <Download size={12} />
            Install v{result.latestVersion}
            {size ? ` (${size})` : ""}
          </Button>
        );
      }
      return (
        <Button variant="outline" size="sm" disabled className="w-fit">
          <Download size={12} />
          v{result.latestVersion} available
        </Button>
      );
    }
    case "downloading":
      return (
        <Button variant="default" size="sm" disabled className="w-fit">
          <Download size={12} className="animate-pulse" />
          Downloading…
        </Button>
      );
    case "downloaded":
      return (
        <Button variant="outline" size="sm" onClick={() => onReveal(state.path)} className="w-fit">
          <FolderOpen size={12} />
          Reveal in file manager
        </Button>
      );
    case "appimage_downloading": {
      const pct = state.total ? Math.floor((state.downloaded / state.total) * 100) : null;
      return (
        <Button variant="default" size="sm" disabled className="w-fit">
          <Download size={12} className="animate-pulse" />
          Installing{pct !== null ? ` ${pct}%` : "…"}
        </Button>
      );
    }
    case "appimage_installed":
      return (
        <Button variant="default" size="sm" onClick={onRelaunch} className="w-fit">
          <RefreshCw size={12} />
          Restart to apply
        </Button>
      );
    case "error":
      return (
        <Button variant="outline" size="sm" onClick={onCheck} className="w-fit">
          <RefreshCw size={12} />
          Retry
        </Button>
      );
  }
}

function renderUpdateSupplementary({
  installSource,
  state,
  onOpenRelease,
}: {
  installSource: InstallSource;
  state: UpdateState;
  onOpenRelease: (url: string) => void;
}) {
  switch (state.kind) {
    case "available": {
      const { result } = state;
      return (
        <div className="grid gap-2">
          <p className="text-xs text-muted-foreground">
            Current: v{result.currentVersion}
          </p>
          {installSource === "appimage" && (
            <p className="text-xs text-muted-foreground">
              The installer replaces the running AppImage in place. Nergal needs to restart afterwards.
            </p>
          )}
          {result.releaseNotes && (
            <details className="rounded-md border border-border/40 bg-card/30 p-2 text-xs" open>
              <summary className="cursor-pointer select-none text-foreground">
                What's new in v{result.latestVersion}
              </summary>
              <div className="prose-invert mt-2 max-w-none text-[11px]">
                <Markdown
                  remarkPlugins={[remarkGfm]}
                  components={{
                    a: (props) => (
                      <a
                        {...props}
                        target="_blank"
                        rel="noreferrer noopener"
                        className="text-primary hover:underline"
                      />
                    ),
                    h1: (props) => <h2 className="mb-1 mt-2 text-xs font-semibold" {...props} />,
                    h2: (props) => <h3 className="mb-1 mt-2 text-xs font-semibold" {...props} />,
                    h3: (props) => <h4 className="mb-1 mt-2 text-[11px] font-semibold" {...props} />,
                    p: (props) => <p className="mb-1 leading-snug" {...props} />,
                    ul: (props) => <ul className="mb-1 ml-4 list-disc" {...props} />,
                    ol: (props) => <ol className="mb-1 ml-4 list-decimal" {...props} />,
                    li: (props) => <li className="mb-0.5" {...props} />,
                    code: (props) => (
                      <code className="rounded bg-muted px-1 font-mono text-[10px]" {...props} />
                    ),
                    pre: (props) => (
                      <pre className="my-1 overflow-x-auto rounded bg-muted p-2 font-mono text-[10px]" {...props} />
                    ),
                  }}
                >
                  {result.releaseNotes}
                </Markdown>
              </div>
            </details>
          )}
          <button
            type="button"
            onClick={() => onOpenRelease(result.releaseUrl)}
            className="inline-flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
          >
            <ExternalLink size={11} />
            Open on GitHub
          </button>
        </div>
      );
    }
    case "downloaded":
      return (
        <p className="text-xs text-muted-foreground">
          <CheckCircle2 size={11} className="inline align-text-bottom text-emerald-500" />{" "}
          Saved to <code className="rounded bg-muted px-1 font-mono text-[10px]">{state.path}</code>.
          Open it with your package manager to install — nergal keeps running on the current version.
        </p>
      );
    case "appimage_downloading":
      return (
        <p className="text-xs text-muted-foreground">
          Downloading signed AppImage and verifying the signature against the embedded pubkey…
        </p>
      );
    case "appimage_installed":
      return (
        <p className="text-xs text-muted-foreground">
          <CheckCircle2 size={11} className="inline align-text-bottom text-emerald-500" />{" "}
          Installed in place. Restart Nergal to load the new version.
        </p>
      );
    case "error":
      return (
        <p className="text-xs text-destructive">
          <XCircle size={11} className="inline align-text-bottom" /> {state.message}
        </p>
      );
    default:
      return null;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function SettingsPanel({ open, onOpenChange }: SettingsProps) {
  const [config, setConfig] = useAtom(configAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const [editors, setEditors] = useState<EditorInfo[]>([]);
  const [activeSection, setActiveSection] = useState<SectionId>("paths");
  const agents = useAtomValue(availableAgentsAtom);
  const setAvailableAgents = useSetAtom(availableAgentsAtom);
  const [rescanning, setRescanning] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const navRef = useRef<HTMLElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => setAppVersion(""));
  }, []);

  const resetObsidianDraft = useSetAtom(resetObsidianDraftAtom);
  useEffect(() => {
    if (open) resetObsidianDraft();
  }, [open, resetObsidianDraft]);

  // Deep-link into a section when a caller requested one (e.g. the update
  // toast's "Open About"). Consume the request so a later plain open lands on
  // whatever section the user last viewed.
  const requestedSection = useAtomValue(settingsRequestedSectionAtom);
  const clearRequestedSection = useSetAtom(settingsRequestedSectionAtom);
  useEffect(() => {
    if (open && requestedSection) {
      setActiveSection(requestedSection as SectionId);
      clearRequestedSection(null);
    }
  }, [open, requestedSection, clearRequestedSection]);

  const FOCUSABLE_SELECTOR =
    'input:not([type="hidden"]):not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"]), [role="combobox"]:not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"]), textarea:not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"]), button:not([disabled]):not([aria-disabled="true"]):not([tabindex="-1"])';

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
      if (appStore.get(keymapCaptureActiveAtom)) return;
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

  // Keyboard-first focus: on open or section change, land focus directly in
  // the content (first form field, or the selected/first theme card on the
  // appearance section). The active nav button still receives focus on the
  // tab-trap loop (see below) and Alt+1-6 still switches sections globally,
  // so the user can navigate everything from the keyboard without an extra
  // Tab to leave the rail. Double-rAF defers past BaseUI Dialog's
  // capture-phase focus trap that would otherwise reclaim focus on reopen.
  useEffect(() => {
    if (!open) return;
    let raf2: number | null = null;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        if (activeSection === "appearance") {
          const cards = contentRef.current?.querySelectorAll<HTMLButtonElement>(
            "[data-theme-card]",
          );
          if (cards && cards.length > 0) {
            const selectedId = normalizeThemeId(config.theme_mode);
            const target =
              Array.from(cards).find((c) => c.dataset.themeId === selectedId) ?? cards[0];
            target.focus({ preventScroll: true });
            return;
          }
        }
        const focusable = contentRef.current?.querySelector<HTMLElement>(
          FOCUSABLE_SELECTOR,
        );
        focusable?.focus({ preventScroll: true });
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      if (raf2 !== null) cancelAnimationFrame(raf2);
    };
  }, [open, activeSection, config.theme_mode]);

  // Arrow-key navigation across the dialog content.
  //   - Theme card grid (`[data-theme-card]`): genuine 2D nav, 2 cols, clamps
  //     at edges. Right/Left within row, Down/Up between rows.
  //   - Form fields elsewhere: linear ArrowDown/Up between focusables;
  //     Left/Right pass through to the field (so cursor nav inside text
  //     inputs keeps working). Comboboxes (BaseUI Select) and textareas
  //     keep their native arrow handling — we no-op for those.
  // Capture + stopPropagation prevents BaseUI Dialog from intercepting.
  useEffect(() => {
    if (!open) return;
    const CARD_COLS = 2;
    function handleArrows(e: KeyboardEvent) {
      if (appStore.get(keymapCaptureActiveAtom)) return;
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
      if (!target) return;

      // Branch 1: theme card 2D nav. When at the edge of the grid in the
      // requested direction, fall through to Branch 2 so the user can
      // leave the grid into adjacent fields (switches below, etc.).
      if (target.closest("[data-theme-card]")) {
        const cards = contentRef.current?.querySelectorAll<HTMLButtonElement>(
          "[data-theme-card]",
        );
        if (cards && cards.length > 0) {
          const list = Array.from(cards);
          const current = target.closest<HTMLButtonElement>("[data-theme-card]");
          const idx = current ? list.indexOf(current) : -1;
          if (idx !== -1) {
            const row = Math.floor(idx / CARD_COLS);
            const col = idx % CARD_COLS;
            const lastIdx = list.length - 1;
            const lastRow = Math.floor(lastIdx / CARD_COLS);
            let nextIdx = idx;
            switch (e.key) {
              case "ArrowRight":
                if (col < CARD_COLS - 1 && idx + 1 <= lastIdx) nextIdx = idx + 1;
                break;
              case "ArrowLeft":
                if (col > 0) nextIdx = idx - 1;
                break;
              case "ArrowDown":
                if (row < lastRow && idx + CARD_COLS <= lastIdx) nextIdx = idx + CARD_COLS;
                break;
              case "ArrowUp":
                if (row > 0) nextIdx = idx - CARD_COLS;
                break;
            }
            if (nextIdx !== idx) {
              e.preventDefault();
              e.stopPropagation();
              const next = list[nextIdx];
              next.focus();
              next.scrollIntoView({ block: "nearest", inline: "nearest" });
              return;
            }
            // Edge of grid in this direction — fall through to Branch 2
            // for ArrowDown/Up so we step into the next focusable field.
          }
        }
      }

      // Branch 2: form-field linear nav. Skip targets that own arrow keys
      // natively so the user keeps cursor/option control. Comboboxes (our
      // base-ui Select trigger) intentionally INCLUDED — the trigger has
      // its own onKeyDown that prevents the popup from opening on arrow
      // keys, so we treat selects as regular linear-nav stops.
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const tag = target.tagName;
      const role = target.getAttribute("role");
      if (tag === "TEXTAREA" || role === "listbox") return;
      if (!contentRef.current?.contains(target)) return;

      const focusables = Array.from(
        contentRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
      );
      const idx = focusables.indexOf(target);
      if (idx === -1) return;

      const forward = e.key === "ArrowDown";
      const nextIdx = forward
        ? Math.min(idx + 1, focusables.length - 1)
        : Math.max(idx - 1, 0);
      if (nextIdx === idx) return;

      e.preventDefault();
      e.stopPropagation();
      const next = focusables[nextIdx];
      next.focus();
      next.scrollIntoView({ block: "nearest", inline: "nearest" });
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
      if (appStore.get(keymapCaptureActiveAtom)) return;
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
    // Per-workspace OpenSpec override rides the same Save (it lives in the DB,
    // not the global config). Skip a no-op write; match-default clears it.
    const od = appStore.get(openspecDirDraftAtom);
    if (od && od.value.trim() !== od.baseline.trim()) {
      const next = od.value.trim() === od.defaultDir.trim() ? null : od.value.trim() || null;
      invoke("set_workspace_openspec_dir", { workspaceId: od.workspaceId, openspecDir: next })
        .then(() => invoke("watch_openspec_for_session", { sessionId: activeSessionId }).catch(() => {}))
        .catch((err) => console.error("[settings] set_workspace_openspec_dir failed:", err));
    }
    onOpenChange(false);
  }

  // Ctrl/Cmd+Enter saves from anywhere in the dialog. Skip when focus is in a
  // textarea so multi-line inputs (future-proof) keep their newline behavior.
  useEffect(() => {
    if (!open) return;
    function handleSaveShortcut(e: KeyboardEvent) {
      if (appStore.get(keymapCaptureActiveAtom)) return;
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
            Configure paths and preferences. Press <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Alt+1</kbd>–<kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Alt+9</kbd> to jump between sections, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Tab</kbd> to enter the form, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Ctrl+Enter</kbd> to save, <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border">Ctrl+,</kbd> to toggle.
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
                  {idx < 9 && (
                    <kbd className="text-[10px] px-1 py-0.5 rounded bg-muted border border-border/40 text-muted-foreground">⌥{idx + 1}</kbd>
                  )}
                </button>
              );
            })}
          </nav>

          <div ref={contentRef} className="overflow-y-auto px-1 max-h-[60vh] scroll-py-3">
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
                <OpenSpecPathField />
                <EnvShellSuggestionsField />
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
              <AppearanceSection
                config={config}
                setConfig={setConfig}
                handleTextChange={handleTextChange}
                handleToggleChange={handleToggleChange}
              />
            )}

            {activeSection === "terminal" && (
              <div className="space-y-4 pt-1">
                {TOGGLE_FIELDS.map(({ key, label, help }) => (
                  <div key={key} className="flex items-start gap-3 text-sm">
                    <Switch
                      id={`setting-${key}`}
                      checked={config[key]}
                      onCheckedChange={(v) => handleToggleChange(key, v)}
                      aria-label={label}
                      className="mt-1"
                    />
                    <label htmlFor={`setting-${key}`} className="flex flex-col gap-0.5 cursor-pointer">
                      <span className="font-medium">{label}</span>
                      <span className="text-xs text-muted-foreground">{help}</span>
                    </label>
                  </div>
                ))}
              </div>
            )}

            {activeSection === "keymap" && <KeymapSection />}

            {activeSection === "mcp" && <McpSection />}

            {activeSection === "scratchpad" && <ScratchpadPathField />}

            {activeSection === "obsidian" && <ObsidianSection />}

            {activeSection === "clickup" && <ClickUpSection />}

            {activeSection === "linear" && <LinearSection />}

            {activeSection === "about" && <AboutSection appVersion={appVersion} />}
          </div>
        </div>

        <DialogFooter className="-mx-4 -mb-4 bg-transparent border-t-0 px-4 pt-2 pb-4">
          <div ref={footerRef} className="contents">
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            {activeSection === "obsidian" && (
              <ObsidianApplyButton />
            )}
            <Button onClick={handleSave}>
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
