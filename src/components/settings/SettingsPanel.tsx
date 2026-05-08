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
import { Switch } from "@/components/ui/switch";
import { Select } from "@/components/ui/select";
import { CheckCircle2, AlertTriangle, XCircle, Info, FolderTree, Bot, Pencil, Palette, Terminal, NotebookText, RefreshCw, Check, ArrowLeft, Trash2, Sliders } from "lucide-react";
import { HexColorPicker } from "react-colorful";
import { scratchpadPathAtom, reloadTabsFromBackend } from "@/stores/scratchpad";
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
              <span
                role="button"
                tabIndex={0}
                aria-label="Customize theme"
                onClick={(e) => { e.stopPropagation(); onCustomize(); }}
                onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); e.preventDefault(); onCustomize(); } }}
                className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
                title="Customize"
              >
                <Sliders size={11} />
              </span>
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
        <Button variant="ghost" size="sm" onClick={onClose} title="Back (Esc)">
          <ArrowLeft size={14} />
          Back
        </Button>
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
                className={`cluihud-focus-ring size-6 rounded-full border-2 transition-transform ${active ? "scale-110 border-foreground" : "border-border hover:scale-105"}`}
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
        <div className="cluihud-color-picker mt-2 px-2">
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

  const customs = config.custom_themes;
  const editingCustom = editingId
    ? customs.find((c) => c.id === editingId) ?? null
    : null;

  const builtinCards = VISIBLE_THEMES;
  const customCards = customs.map((c) => ({
    custom: c,
    theme: resolveCustomTheme(c),
  }));
  const activeId = normalizeThemeId(config.theme_mode);

  function selectTheme(id: string) {
    handleTextChange("theme_mode", id);
  }

  function customizeBuiltin(baseId: string) {
    const fork = forkBuiltinTheme(baseId, customs);
    setConfig((prev) => ({
      ...prev,
      custom_themes: [...prev.custom_themes, fork],
      theme_mode: fork.id,
    }));
    setReturnFocusId(fork.id);
    setEditingId(fork.id);
  }

  function editCustom(customId: string) {
    selectTheme(customId);
    setReturnFocusId(customId);
    setEditingId(customId);
  }

  function updateCustom(next: CustomTheme) {
    setConfig((prev) => ({
      ...prev,
      custom_themes: prev.custom_themes.map((c) => (c.id === next.id ? next : c)),
    }));
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
    return (
      <ThemeEditor
        custom={editingCustom}
        onChange={updateCustom}
        onClose={() => setEditingId(null)}
        onDelete={() => deleteCustom(editingCustom.id)}
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
              <AppearanceSection
                config={config}
                setConfig={setConfig}
                handleTextChange={handleTextChange}
                handleToggleChange={handleToggleChange}
              />
            )}

            {activeSection === "terminal" && (
              <div className="space-y-4">
                {TOGGLE_FIELDS.map(({ key, label, help }) => (
                  <div key={key} className="flex items-start gap-3 text-sm">
                    <Switch
                      id={`setting-${key}`}
                      checked={config[key]}
                      onCheckedChange={(v) => handleToggleChange(key, v)}
                      aria-label={label}
                      className="mt-0.5"
                    />
                    <label htmlFor={`setting-${key}`} className="flex flex-col gap-0.5 cursor-pointer">
                      <span className="font-medium">{label}</span>
                      <span className="text-xs text-muted-foreground">{help}</span>
                    </label>
                  </div>
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
